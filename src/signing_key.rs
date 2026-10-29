//! The private key deploys are signed with.

use std::fmt;
use std::path::Path;

use secp256k1::{PublicKey, Secp256k1, SecretKey};

use crate::error::{Error, Result};
use crate::utils::CryptoUtils;

/// A secp256k1 private key. Its `Debug` output is redacted, so a key held in a
/// configuration cannot reach a log line or an error message.
///
/// Dropping it overwrites its bytes. That is best effort: the key is copied
/// into signing contexts, and the compiler may leave other copies in memory.
#[derive(Clone)]
pub struct SigningKey(SecretKey);

impl SigningKey {
    /// A key from its 64 hex characters.
    pub fn from_hex(hex: &str) -> Result<Self> {
        Ok(SigningKey(CryptoUtils::decode_private_key(hex.trim())?))
    }

    /// A key from a file holding its hex. On Unix the file must not be
    /// readable by group or others, the rule ssh applies to its keys.
    pub fn from_file(path: &Path) -> Result<Self> {
        let io_error = |cause| Error::Io {
            path: path.to_path_buf(),
            cause,
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path)
                .map_err(io_error)?
                .permissions()
                .mode();
            if mode & 0o077 != 0 {
                return Err(Error::invalid(
                    "private key file",
                    format!(
                        "{} is readable by group or others (mode {:o}); run chmod 600 on it",
                        path.display(),
                        mode & 0o777
                    ),
                ));
            }
        }
        let contents = std::fs::read_to_string(path).map_err(io_error)?;
        Self::from_hex(&contents)
    }

    pub fn secret_key(&self) -> &SecretKey {
        &self.0
    }

    pub fn public_key(&self) -> PublicKey {
        self.0.public_key(&Secp256k1::signing_only())
    }
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SigningKey(<redacted>)")
    }
}

impl Drop for SigningKey {
    fn drop(&mut self) {
        self.0.non_secure_erase();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "4242424242424242424242424242424242424242424242424242424242424242";

    #[test]
    fn debug_output_never_shows_the_key() {
        let key = SigningKey::from_hex(KEY).unwrap();
        assert_eq!(format!("{key:?}"), "SigningKey(<redacted>)");
        assert!(!format!("{:?}", Some(key)).contains("4242"));
    }

    #[cfg(unix)]
    #[test]
    fn a_key_file_others_can_read_is_refused() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("node-cli-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("key");
        std::fs::write(&path, format!("{KEY}\n")).unwrap();

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let error = SigningKey::from_file(&path).unwrap_err();
        assert!(error.to_string().contains("chmod 600"), "{error}");

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let key = SigningKey::from_file(&path).unwrap();
        assert_eq!(
            key.secret_key(),
            SigningKey::from_hex(KEY).unwrap().secret_key()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
