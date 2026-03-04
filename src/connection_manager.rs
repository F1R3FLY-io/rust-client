/// F1r3fly Connection Manager
///
/// Manages connections to F1r3fly nodes with connection reuse and pooling.
/// Provides a high-level async API for deploying Rholang code and querying state.
use crate::f1r3fly_api::F1r3flyApi;
use crate::vault::{build_transfer_rholang, TransferResult};
use crate::utils::CryptoUtils;
use log;
use secp256k1::PublicKey;
use std::env;

/// Configuration for F1r3fly node connection
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub node_host: String,
    pub grpc_port: u16,
    pub http_port: u16,
    pub signing_key: String,
    /// Maximum number of 1-second polling attempts when waiting for a deploy
    /// to be included in a block (default: 180)
    pub deploy_timeout_secs: u32,
}

impl ConnectionConfig {
    /// Load configuration from environment variables
    ///
    /// # Environment Variables
    ///
    /// - `FIREFLY_HOST`: Node hostname (default: "localhost")
    /// - `FIREFLY_GRPC_PORT`: gRPC port (default: 40401)
    /// - `FIREFLY_HTTP_PORT`: HTTP port (default: 40403)
    /// - `FIREFLY_PRIVATE_KEY`: Private key for signing (REQUIRED)
    /// - `FIREFLY_DEPLOY_TIMEOUT`: Max seconds to wait for deploy inclusion in a block (default: 180)
    pub fn from_env() -> Result<Self, ConnectionError> {
        let signing_key =
            env::var("FIREFLY_PRIVATE_KEY").map_err(|_| ConnectionError::MissingPrivateKey)?;

        Ok(Self {
            node_host: env::var("FIREFLY_HOST").unwrap_or_else(|_| "localhost".to_string()),
            grpc_port: env::var("FIREFLY_GRPC_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(40401),
            http_port: env::var("FIREFLY_HTTP_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(40403),
            signing_key,
            deploy_timeout_secs: env::var("FIREFLY_DEPLOY_TIMEOUT")
                .ok()
                .and_then(|t| t.parse().ok())
                .unwrap_or(180),
        })
    }

    /// Create a new configuration with explicit values
    pub fn new(node_host: String, grpc_port: u16, http_port: u16, signing_key: String) -> Self {
        Self {
            node_host,
            grpc_port,
            http_port,
            signing_key,
            deploy_timeout_secs: 180,
        }
    }
}

/// Error types for connection management
#[derive(Debug)]
pub enum ConnectionError {
    /// FIREFLY_PRIVATE_KEY environment variable not set
    MissingPrivateKey,
    /// Failed to connect to F1r3fly node
    ConnectionFailed(String),
    /// Failed to execute operation
    OperationFailed(String),
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPrivateKey => {
                write!(f, "FIREFLY_PRIVATE_KEY environment variable not set")
            }
            Self::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            Self::OperationFailed(e) => write!(f, "Operation failed: {}", e),
        }
    }
}

impl std::error::Error for ConnectionError {}

/// Manages F1r3fly node connections with connection reuse
#[derive(Clone)]
pub struct F1r3flyConnectionManager {
    config: ConnectionConfig,
}

impl F1r3flyConnectionManager {
    /// Create a new connection manager from environment variables
    pub fn from_env() -> Result<Self, ConnectionError> {
        let config = ConnectionConfig::from_env()?;
        Ok(Self { config })
    }

    /// Create a new connection manager with explicit configuration
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config }
    }

    /// Get the connection configuration
    pub fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    fn api(&self) -> F1r3flyApi<'_> {
        F1r3flyApi::new(
            &self.config.signing_key,
            &self.config.node_host,
            self.config.grpc_port,
        )
    }

    /// Execute an exploratory deploy (read-only query)
    pub async fn query(&self, rholang_code: &str) -> Result<String, ConnectionError> {
        let api = self.api();
        let (result, _block_info) = api
            .exploratory_deploy(rholang_code, None, false)
            .await
            .map_err(|e| ConnectionError::OperationFailed(e.to_string()))?;
        Ok(result)
    }

    /// Deploy Rholang code to the blockchain
    pub async fn deploy(&self, rholang_code: &str) -> Result<String, ConnectionError> {
        let api = self.api();
        api.deploy_with_phlo_limit(rholang_code, 500_000, "rholang")
            .await
            .map_err(|e| ConnectionError::OperationFailed(e.to_string()))
    }

    /// Deploy Rholang code with a specific timestamp
    ///
    /// Required for insertSigned compatibility where the deploy timestamp
    /// must match the signature timestamp.
    pub async fn deploy_with_timestamp(
        &self,
        rholang_code: &str,
        timestamp_millis: i64,
    ) -> Result<String, ConnectionError> {
        let api = self.api();
        api.deploy_with_timestamp_and_phlo_limit(
            rholang_code,
            "rholang",
            Some(timestamp_millis),
            500_000,
        )
        .await
        .map_err(|e| ConnectionError::OperationFailed(e.to_string()))
    }

    /// Wait for a deploy to be included in a block
    pub async fn wait_for_deploy(
        &self,
        deploy_id: &str,
        max_attempts: u32,
    ) -> Result<String, ConnectionError> {
        let api = self.api();
        let check_interval_sec = 1;

        for attempt in 1..=max_attempts {
            let result = api
                .get_deploy_block_hash(deploy_id, self.config.http_port)
                .await
                .map_err(|e| ConnectionError::OperationFailed(e.to_string()))?;

            match result {
                Some(block_hash) => {
                    log::debug!(
                        "Deploy {} found in block {} after {} attempts",
                        deploy_id,
                        block_hash,
                        attempt
                    );
                    return Ok(block_hash);
                }
                None => {
                    if attempt >= max_attempts {
                        return Err(ConnectionError::OperationFailed(format!(
                            "Deploy not included in block after {} attempts",
                            max_attempts
                        )));
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(check_interval_sec)).await;
                }
            }
        }

        Err(ConnectionError::OperationFailed(
            "Deploy wait timeout".to_string(),
        ))
    }

    /// Wait for a block to be finalized
    pub async fn wait_for_finalization(
        &self,
        block_hash: &str,
        max_attempts: u32,
    ) -> Result<(), ConnectionError> {
        let api = self.api();
        let retry_delay_sec = 5;

        let is_finalized = api
            .is_finalized(block_hash, max_attempts, retry_delay_sec)
            .await
            .map_err(|e| ConnectionError::OperationFailed(e.to_string()))?;

        if is_finalized {
            Ok(())
        } else {
            Err(ConnectionError::OperationFailed(format!(
                "Block {} not finalized after {} attempts",
                block_hash, max_attempts
            )))
        }
    }

    /// Deploy Rholang code and wait for finalization
    pub async fn deploy_and_wait(
        &self,
        rholang_code: &str,
        max_block_wait_attempts: u32,
        max_finalization_attempts: u32,
    ) -> Result<(String, String), ConnectionError> {
        let deploy_id = self.deploy(rholang_code).await?;

        let block_hash = self
            .wait_for_deploy(&deploy_id, max_block_wait_attempts)
            .await?;

        self.wait_for_finalization(&block_hash, max_finalization_attempts)
            .await?;

        Ok((deploy_id, block_hash))
    }

    /// Get direct access to the underlying F1r3flyApi
    pub fn get_api(&self) -> F1r3flyApi<'_> {
        self.api()
    }

    // =========================================================================
    // Vault Operations
    // =========================================================================

    /// Transfer native tokens from this connection's vault to another address
    ///
    /// # Arguments
    ///
    /// * `to_address` - Recipient vault address (1111...)
    /// * `amount_dust` - Amount in dust (1 token = 100,000,000 dust)
    pub async fn transfer(
        &self,
        to_address: &str,
        amount_dust: u64,
    ) -> Result<TransferResult, ConnectionError> {
        crate::vault::validate_address(to_address)
            .map_err(|e| ConnectionError::OperationFailed(e))?;

        let from_address = self.get_address()?;

        log::info!(
            "Transferring {} dust ({:.8} tokens) from {} to {}",
            amount_dust,
            crate::vault::dust_to_tokens(amount_dust),
            from_address,
            to_address
        );

        let rholang = build_transfer_rholang(&from_address, to_address, amount_dust);

        let (deploy_id, block_hash) = self
            .deploy_and_wait(&rholang, self.config.deploy_timeout_secs, 20)
            .await?;

        log::info!(
            "Transfer complete: {} dust to {} (deploy: {})",
            amount_dust,
            to_address,
            deploy_id
        );

        Ok(TransferResult {
            deploy_id,
            block_hash,
            from_address,
            to_address: to_address.to_string(),
            amount_dust,
        })
    }

    /// Get the vault address for this connection's signing key
    pub fn get_address(&self) -> Result<String, ConnectionError> {
        let public_key = self.get_public_key()?;
        let pubkey_hex = hex::encode(public_key.serialize_uncompressed());
        CryptoUtils::generate_vault_address(&pubkey_hex)
            .map_err(|e| ConnectionError::OperationFailed(e.to_string()))
    }

    /// Get the public key for this connection's signing key
    pub fn get_public_key(&self) -> Result<PublicKey, ConnectionError> {
        let secret_key = CryptoUtils::decode_private_key(&self.config.signing_key)
            .map_err(|e| ConnectionError::OperationFailed(e.to_string()))?;
        Ok(CryptoUtils::derive_public_key(&secret_key))
    }

    /// Get the public key as hex string (uncompressed format)
    pub fn get_public_key_hex(&self) -> Result<String, ConnectionError> {
        let public_key = self.get_public_key()?;
        Ok(hex::encode(public_key.serialize_uncompressed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env_missing_key() {
        env::remove_var("FIREFLY_PRIVATE_KEY");
        let result = ConnectionConfig::from_env();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConnectionError::MissingPrivateKey
        ));
    }

    #[test]
    fn test_config_new() {
        let config =
            ConnectionConfig::new("example.com".to_string(), 9000, 9001, "my_key".to_string());
        assert_eq!(config.node_host, "example.com");
        assert_eq!(config.grpc_port, 9000);
        assert_eq!(config.http_port, 9001);
        assert_eq!(config.signing_key, "my_key");
    }
}
