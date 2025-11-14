// Signing utilities for F1r3node deploys
//
// This module provides signing functions used by both gRPC and HTTP clients.

use blake2::{Blake2b512, Digest};
use secp256k1::{Message as Secp256k1Message, Secp256k1, SecretKey};

/// Sign deploy data using secp256k1
///
/// This creates a signature over the deploy data using the Blake2b-256 hash
/// and secp256k1 ECDSA signing.
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
    // Hash the deploy data with timestamp
    let mut hasher = Blake2b512::new();
    hasher.update(data);
    hasher.update(&timestamp.to_le_bytes());
    let hash = hasher.finalize();
    
    // Take first 32 bytes for secp256k1 message
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&hash[..32]);

    // Sign with secp256k1
    let secp = Secp256k1::new();
    let message = Secp256k1Message::from_digest(digest);
    let signature = secp.sign_ecdsa(&message, private_key);

    // Return DER-encoded signature
    Ok(signature.serialize_der().to_vec())
}

/// Errors that can occur during signing
#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("Failed to sign data: {0}")]
    SigningFailed(String),
}

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

        // Signature should be in DER format (typically 70-72 bytes)
        assert!(signature.len() >= 70 && signature.len() <= 72);
    }

    #[test]
    fn test_sign_deploy_data_deterministic() {
        let private_key = test_private_key();
        let data = b"new x in { x!(1) }";
        let timestamp = 1234567890i64;

        let sig1 = sign_deploy_data(data, timestamp, &private_key).unwrap();
        let sig2 = sign_deploy_data(data, timestamp, &private_key).unwrap();

        // Same input should produce same signature
        assert_eq!(sig1, sig2);
    }
}

