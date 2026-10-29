//! Building and signing deploys, separately from sending them.
//!
//! A deploy's ID is its signature, so a [`SignedDeploy`] knows its ID before
//! any node has seen it: a caller can record the deploy, send it, and after a
//! crash or a timeout send the same bytes again without deploying twice.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use blake2::{Blake2b, Digest};
use f1r3fly_models::casper::DeployDataProto;
use f1r3fly_models::ByteString;
use prost::Message as _;
use secp256k1::ecdsa::Signature;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use typenum::U32;

use crate::error::{Error, Result};

/// Phlo limit of a deploy when the caller does not choose one.
pub const DEFAULT_PHLO_LIMIT: u64 = 50_000;
/// Phlo price of a deploy when the caller does not choose one.
pub const DEFAULT_PHLO_PRICE: i64 = 1;
/// Shard a client signs for when it is not configured with another.
pub const DEFAULT_SHARD_ID: &str = "root";

const SIG_ALGORITHM: &str = "secp256k1";
const LANGUAGE: &str = "rholang";

/// What a deploy commits to, beyond its term. All of it is signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployOptions {
    pub phlo_limit: u64,
    pub phlo_price: i64,
    /// Unix milliseconds after which the deploy is invalid; `None` never expires.
    pub expiration: Option<i64>,
    /// Exact Unix-millisecond timestamp to sign, for contracts that check it,
    /// such as `insertSigned`. `None` takes the next automatic timestamp.
    pub timestamp: Option<i64>,
}

impl Default for DeployOptions {
    fn default() -> Self {
        DeployOptions {
            phlo_limit: DEFAULT_PHLO_LIMIT,
            phlo_price: DEFAULT_PHLO_PRICE,
            expiration: None,
            timestamp: None,
        }
    }
}

impl DeployOptions {
    pub fn with_phlo_limit(mut self, phlo_limit: u64) -> Self {
        self.phlo_limit = phlo_limit;
        self
    }
}

/// A deploy with every field fixed but no signature yet.
#[derive(Debug, Clone, PartialEq)]
pub struct UnsignedDeploy {
    data: DeployDataProto,
}

impl UnsignedDeploy {
    /// Fix a deploy's fields. `valid_after_block_number` anchors its validity
    /// window; [`crate::F1r3flyApi::prepare`] takes it from the node's tip.
    pub fn new(
        term: impl Into<String>,
        options: &DeployOptions,
        valid_after_block_number: i64,
        shard_id: impl Into<String>,
    ) -> Result<Self> {
        let phlo_limit = i64::try_from(options.phlo_limit)
            .map_err(|_| Error::invalid("phlo limit", options.phlo_limit))?;
        if options.phlo_price < 1 {
            return Err(Error::invalid("phlo price", options.phlo_price));
        }
        if let Some(expiration) = options.expiration.filter(|e| *e <= 0) {
            return Err(Error::invalid("expiration", expiration));
        }
        Ok(UnsignedDeploy {
            data: DeployDataProto {
                term: term.into(),
                timestamp: options.timestamp.unwrap_or_else(next_timestamp),
                phlo_price: options.phlo_price,
                phlo_limit,
                valid_after_block_number,
                shard_id: shard_id.into(),
                language: String::new(),
                sig: ByteString::new(),
                deployer: ByteString::new(),
                sig_algorithm: String::new(),
                expiration_timestamp: options.expiration.unwrap_or(0),
                parameters: Vec::new(),
            },
        })
    }

    /// The 32 bytes a signer signs: Blake2b-256 of the deploy's fields, with
    /// the signature, deployer and language left empty.
    pub fn digest(&self) -> [u8; 32] {
        digest(&self.data)
    }

    pub fn sign(self, key: &SecretKey) -> SignedDeploy {
        let secp = Secp256k1::signing_only();
        let signature = secp.sign_ecdsa(Message::from_digest(self.digest()), key);
        self.into_signed(&key.public_key(&secp), &signature)
    }

    /// Attach a signature made elsewhere, such as by a hardware key. The
    /// signature is DER-encoded secp256k1 ECDSA over [`digest`](Self::digest)
    /// and is verified before it is accepted. An upper-half S value is
    /// replaced by its lower-half equivalent, the only form nodes accept, so
    /// the deploy's ID is the normalized signature.
    pub fn with_signature(
        self,
        public_key: &PublicKey,
        der_signature: &[u8],
    ) -> Result<SignedDeploy> {
        let mut signature = Signature::from_der(der_signature)
            .map_err(|e| Error::invalid("deploy signature", e))?;
        signature.normalize_s();
        verify(&self.digest(), public_key, &signature)?;
        Ok(self.into_signed(public_key, &signature))
    }

    fn into_signed(mut self, public_key: &PublicKey, signature: &Signature) -> SignedDeploy {
        self.data.sig = ByteString::from(signature.serialize_der().to_vec());
        self.data.deployer = ByteString::from(public_key.serialize_uncompressed().to_vec());
        self.data.sig_algorithm = SIG_ALGORITHM.into();
        self.data.language = LANGUAGE.into();
        SignedDeploy { data: self.data }
    }
}

/// A signed deploy, ready to send to any node of its shard.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedDeploy {
    data: DeployDataProto,
}

impl SignedDeploy {
    /// The deploy's ID: its signature, hex-encoded.
    pub fn id(&self) -> String {
        hex::encode(&self.data.sig)
    }

    /// The deploy as bytes, to store and send again later.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.data.encode_to_vec()
    }

    /// A deploy stored with [`to_bytes`](Self::to_bytes). Its signature is
    /// verified, so corrupted or tampered bytes are rejected here rather than
    /// by the node. An upper-half S signature is rejected, not normalized:
    /// normalizing would change the ID the deploy was stored under.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let data =
            DeployDataProto::decode(bytes).map_err(|e| Error::invalid("stored deploy", e))?;
        if data.sig_algorithm != SIG_ALGORITHM {
            return Err(Error::invalid(
                "stored deploy",
                format!("signature algorithm {:?}", data.sig_algorithm),
            ));
        }
        let public_key = PublicKey::from_slice(&data.deployer)
            .map_err(|e| Error::invalid("stored deploy", e))?;
        let signature =
            Signature::from_der(&data.sig).map_err(|e| Error::invalid("stored deploy", e))?;
        let mut normalized = signature;
        normalized.normalize_s();
        if normalized != signature {
            return Err(Error::invalid(
                "stored deploy",
                "signature has an upper-half S value, which nodes reject",
            ));
        }
        verify(&digest(&data), &public_key, &signature)?;
        Ok(SignedDeploy { data })
    }

    pub(crate) fn proto(&self) -> &DeployDataProto {
        &self.data
    }
}

fn digest(data: &DeployDataProto) -> [u8; 32] {
    let projection = DeployDataProto {
        language: String::new(),
        sig: ByteString::new(),
        deployer: ByteString::new(),
        sig_algorithm: String::new(),
        ..data.clone()
    };
    Blake2b::<U32>::digest(projection.encode_to_vec()).into()
}

fn verify(digest: &[u8; 32], public_key: &PublicKey, signature: &Signature) -> Result<()> {
    Secp256k1::verification_only()
        .verify_ecdsa(Message::from_digest(*digest), signature, public_key)
        .map_err(|e| Error::invalid("deploy signature", e))
}

static LAST_TIMESTAMP: AtomicI64 = AtomicI64::new(0);

/// The current time in Unix milliseconds, but strictly later than any
/// timestamp this process handed out before, so two otherwise identical
/// deploys never share a signature, even when the clock steps back.
fn next_timestamp() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is set before 1970")
        .as_millis() as i64;
    let previous = LAST_TIMESTAMP
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |last| {
            Some(now.max(last + 1))
        })
        .expect("the update closure always returns Some");
    now.max(previous + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn key() -> SecretKey {
        SecretKey::from_byte_array([0x42; 32]).expect("32 bytes is a valid key")
    }

    fn unsigned(timestamp: Option<i64>) -> UnsignedDeploy {
        let options = DeployOptions {
            timestamp,
            ..DeployOptions::default()
        };
        UnsignedDeploy::new("new x in { x!(1) }", &options, 7, DEFAULT_SHARD_ID).unwrap()
    }

    #[test]
    fn a_signed_deploy_verifies_against_its_digest() {
        let deploy = unsigned(Some(1_000));
        let digest = deploy.digest();
        let signed = deploy.sign(&key());
        let secp = Secp256k1::new();
        let signature = Signature::from_der(&signed.proto().sig).unwrap();
        assert!(verify(&digest, &key().public_key(&secp), &signature).is_ok());
        assert_eq!(signed.id(), hex::encode(&signed.proto().sig));
        assert_eq!(signed.proto().language, LANGUAGE);
        assert_eq!(signed.proto().shard_id, DEFAULT_SHARD_ID);
    }

    #[test]
    fn signing_is_unchanged_from_the_scheme_the_node_verifies() {
        let signed = unsigned(Some(1_000)).sign(&key());
        let expected = DeployDataProto {
            term: "new x in { x!(1) }".into(),
            timestamp: 1_000,
            phlo_price: 1,
            phlo_limit: 50_000,
            valid_after_block_number: 7,
            shard_id: "root".into(),
            language: String::new(),
            sig: ByteString::new(),
            deployer: ByteString::new(),
            sig_algorithm: String::new(),
            expiration_timestamp: 0,
            parameters: Vec::new(),
        };
        let digest: [u8; 32] = Blake2b::<U32>::digest(expected.encode_to_vec()).into();
        let signature = Signature::from_der(&signed.proto().sig).unwrap();
        let secp = Secp256k1::new();
        assert!(verify(&digest, &key().public_key(&secp), &signature).is_ok());
    }

    #[test]
    fn an_external_signature_is_accepted_only_when_it_verifies() {
        let secp = Secp256k1::new();
        let deploy = unsigned(Some(1_000));
        let good = secp.sign_ecdsa(Message::from_digest(deploy.digest()), &key());
        let signed = deploy
            .clone()
            .with_signature(&key().public_key(&secp), &good.serialize_der())
            .unwrap();
        assert_eq!(signed, deploy.clone().sign(&key()));

        let other = SecretKey::from_byte_array([0x43; 32]).unwrap();
        let wrong = secp.sign_ecdsa(Message::from_digest(deploy.digest()), &other);
        assert!(matches!(
            deploy.with_signature(&key().public_key(&secp), &wrong.serialize_der()),
            Err(Error::InvalidInput {
                what: "deploy signature",
                ..
            })
        ));
    }

    /// The same signature with `s` replaced by `n - s`: equally valid ECDSA,
    /// but in the upper half that libsecp256k1 refuses to verify.
    fn high_s(signature: &Signature) -> Signature {
        let compact = signature.serialize_compact();
        let s: [u8; 32] = compact[32..].try_into().unwrap();
        let mut flipped = compact;
        flipped[32..].copy_from_slice(
            &SecretKey::from_byte_array(s)
                .unwrap()
                .negate()
                .secret_bytes(),
        );
        Signature::from_compact(&flipped).unwrap()
    }

    #[test]
    fn an_external_high_s_signature_is_accepted_in_low_s_form() {
        let secp = Secp256k1::new();
        let deploy = unsigned(Some(1_000));
        let low = secp.sign_ecdsa(Message::from_digest(deploy.digest()), &key());
        let high = high_s(&low);
        assert_ne!(high, low);

        let signed = deploy
            .with_signature(&key().public_key(&secp), &high.serialize_der())
            .unwrap();
        assert_eq!(signed.proto().sig.to_vec(), low.serialize_der().to_vec());
    }

    #[test]
    fn a_stored_high_s_deploy_is_rejected_rather_than_given_a_new_id() {
        let signed = unsigned(Some(1_000)).sign(&key());
        let low = Signature::from_der(&signed.proto().sig).unwrap();
        let mut stored = signed.proto().clone();
        stored.sig = ByteString::from(high_s(&low).serialize_der().to_vec());

        let error = SignedDeploy::from_bytes(&stored.encode_to_vec()).unwrap_err();
        assert!(error.to_string().contains("upper-half S"), "{error}");
    }

    #[test]
    fn a_stored_deploy_round_trips_and_tampering_is_rejected() {
        let signed = unsigned(Some(1_000)).sign(&key());
        assert_eq!(
            SignedDeploy::from_bytes(&signed.to_bytes()).unwrap(),
            signed
        );

        let mut tampered = signed.proto().clone();
        tampered.phlo_limit += 1;
        assert!(SignedDeploy::from_bytes(&tampered.encode_to_vec()).is_err());
        assert!(SignedDeploy::from_bytes(b"not a deploy").is_err());
    }

    #[test]
    fn concurrent_automatic_timestamps_are_distinct() {
        let ids: HashSet<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        (0..50)
                            .map(|_| unsigned(None).sign(&key()).id())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect()
        });
        assert_eq!(ids.len(), 400);
    }

    #[test]
    fn an_explicit_timestamp_is_signed_exactly() {
        assert_eq!(unsigned(Some(1_234)).data.timestamp, 1_234);
        assert_eq!(
            unsigned(Some(1_234)).digest(),
            unsigned(Some(1_234)).digest()
        );
    }

    #[test]
    fn out_of_range_options_are_rejected() {
        let try_new = |options: DeployOptions| UnsignedDeploy::new("Nil", &options, 1, "root");
        assert!(try_new(DeployOptions::default().with_phlo_limit(u64::MAX)).is_err());
        assert!(try_new(DeployOptions {
            phlo_price: 0,
            ..DeployOptions::default()
        })
        .is_err());
        assert!(try_new(DeployOptions {
            expiration: Some(0),
            ..DeployOptions::default()
        })
        .is_err());
    }
}
