// Signing utilities for F1r3node deploys
//
// This module provides signing functions used by both gRPC and HTTP clients.

use blake2::{Blake2b, Digest};
use secp256k1::{Message as Secp256k1Message, Secp256k1, SecretKey};
use typenum::U32;

/// Sign deploy data using secp256k1
///
/// Creates a signature over the deploy data using Blake2b-256 hash
/// and secp256k1 ECDSA signing. Uses Blake2b-256 (not 512) to produce
/// a native 32-byte digest as required by secp256k1, consistent with
/// the gRPC deploy signing in `f1r3fly_api`.
///
/// # Arguments
///
/// * `data` - The serialized deploy data to sign
/// * `timestamp` - The deployment timestamp
/// * `private_key` - The secp256k1 private key
///
/// # Returns
///
/// The DER-encoded signature bytes
pub fn sign_deploy_data(
    data: &[u8],
    timestamp: i64,
    private_key: &SecretKey,
) -> Result<Vec<u8>, SigningError> {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(data);
    hasher.update(&timestamp.to_le_bytes());
    let hash = hasher.finalize();

    let mut digest = [0u8; 32];
    digest.copy_from_slice(&hash);

    let secp = Secp256k1::new();
    let message = Secp256k1Message::from_digest(digest);
    let signature = secp.sign_ecdsa(&message, private_key);

    Ok(signature.serialize_der().to_vec())
}

#[derive(Debug)]
pub enum SigningError {
    SigningFailed(String),
}

impl std::fmt::Display for SigningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SigningError::SigningFailed(msg) => write!(f, "Signing failed: {}", msg),
        }
    }
}

impl std::error::Error for SigningError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_private_key() -> SecretKey {
        SecretKey::from_slice(&[0x42; 32]).expect("32 bytes is valid")
    }

    #[test]
    fn test_sign_deploy_data() {
        let private_key = test_private_key();
        let data = b"new x in { x!(1) }";
        let timestamp = 1234567890i64;

        let signature = sign_deploy_data(data, timestamp, &private_key).unwrap();
        assert!(signature.len() >= 70 && signature.len() <= 72);
    }

    #[test]
    fn test_sign_deploy_data_deterministic() {
        let private_key = test_private_key();
        let data = b"new x in { x!(1) }";
        let timestamp = 1234567890i64;

        let sig1 = sign_deploy_data(data, timestamp, &private_key).unwrap();
        let sig2 = sign_deploy_data(data, timestamp, &private_key).unwrap();
        assert_eq!(sig1, sig2);
    }
}
