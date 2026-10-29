use crate::error::{Error, Result};
use f1r3fly_crypto::rust::public_key::PublicKey;
use f1r3fly_rholang::rust::interpreter::util::vault_address::VaultAddress;
use hex;
use secp256k1::rand;
use secp256k1::{PublicKey as Secp256k1PublicKey, Secp256k1, SecretKey};
use std::fs;
use std::path::Path;

pub struct CryptoUtils;

impl CryptoUtils {
    /// Decode a hex-encoded private key
    pub fn decode_private_key(private_key_hex: &str) -> Result<SecretKey> {
        let private_key_bytes =
            hex::decode(private_key_hex).map_err(|e| Error::invalid("private key", e))?;
        let key_array: [u8; 32] = private_key_bytes
            .try_into()
            .map_err(|_| Error::invalid("private key", "must be 32 bytes"))?;

        SecretKey::from_byte_array(key_array).map_err(|e| Error::invalid("private key", e))
    }

    /// Generate a new random key pair
    pub fn generate_key_pair() -> (SecretKey, Secp256k1PublicKey) {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::new(&mut rand::rng());
        let public_key = secret_key.public_key(&secp);
        (secret_key, public_key)
    }

    /// Derive public key from private key
    pub fn derive_public_key(private_key: &SecretKey) -> Secp256k1PublicKey {
        let secp = Secp256k1::new();
        private_key.public_key(&secp)
    }

    /// Serialize public key to hex (compressed or uncompressed)
    pub fn serialize_public_key(public_key: &Secp256k1PublicKey, compressed: bool) -> String {
        if compressed {
            hex::encode(public_key.serialize())
        } else {
            hex::encode(public_key.serialize_uncompressed())
        }
    }

    /// Serialize private key to hex
    pub fn serialize_private_key(private_key: &SecretKey) -> String {
        hex::encode(private_key.secret_bytes())
    }

    /// Generate vault address from public key
    pub fn generate_vault_address(public_key_hex: &str) -> Result<String> {
        let public_key_bytes =
            hex::decode(public_key_hex).map_err(|e| Error::invalid("public key", e))?;

        let public_key = PublicKey::from_bytes(&public_key_bytes);

        VaultAddress::from_public_key(&public_key)
            .map(|vault_address| vault_address.to_base58())
            .ok_or_else(|| Error::invalid("public key", "no vault address derives from it"))
    }

    /// Write key pair to files. On Unix the private key file is readable by
    /// its owner only, as `SigningKey::from_file` requires.
    pub fn write_key_pair_to_files(
        private_key: &SecretKey,
        public_key: &Secp256k1PublicKey,
        private_key_file: &Path,
        public_key_file: &Path,
        compressed: bool,
    ) -> Result<()> {
        let io_error = |path: &Path| {
            let path = path.to_path_buf();
            move |cause| Error::Io { path, cause }
        };

        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            options.mode(0o600);
            if private_key_file.exists() {
                fs::set_permissions(private_key_file, fs::Permissions::from_mode(0o600))
                    .map_err(io_error(private_key_file))?;
            }
        }
        std::io::Write::write_all(
            &mut options
                .open(private_key_file)
                .map_err(io_error(private_key_file))?,
            Self::serialize_private_key(private_key).as_bytes(),
        )
        .map_err(io_error(private_key_file))?;

        fs::write(
            public_key_file,
            Self::serialize_public_key(public_key, compressed),
        )
        .map_err(io_error(public_key_file))
    }

    /// Create secp256k1 context
    pub fn create_secp256k1_context() -> Secp256k1<secp256k1::All> {
        Secp256k1::new()
    }

    /// Validate hex string
    pub fn is_valid_hex(hex_str: &str) -> bool {
        hex::decode(hex_str).is_ok()
    }

    /// Validate private key format
    pub fn is_valid_private_key(private_key_hex: &str) -> bool {
        if let Ok(bytes) = hex::decode(private_key_hex) {
            let Ok(arr): std::result::Result<[u8; 32], _> = bytes.try_into() else {
                return false;
            };
            SecretKey::from_byte_array(arr).is_ok()
        } else {
            false
        }
    }

    /// Validate public key format
    pub fn is_valid_public_key(public_key_hex: &str) -> bool {
        if let Ok(bytes) = hex::decode(public_key_hex) {
            // PublicKey::from_bytes always succeeds, so just check if hex decode worked
            // and if the bytes are a reasonable length for a public key
            bytes.len() == 65 || bytes.len() == 33 // uncompressed or compressed
        } else {
            false
        }
    }
}
