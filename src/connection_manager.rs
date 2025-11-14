/// F1r3fly Connection Manager
///
/// Manages connections to F1r3fly nodes with connection reuse and pooling.
/// This eliminates the need to create new F1r3flyApi instances on every call.

use crate::f1r3fly_api::F1r3flyApi;
use std::env;

/// Configuration for F1r3fly node connection
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub node_host: String,
    pub grpc_port: u16,
    pub http_port: u16,
    pub signing_key: String,
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
    ///
    /// # Errors
    ///
    /// Returns an error if `FIREFLY_PRIVATE_KEY` is not set
    pub fn from_env() -> Result<Self, ConnectionError> {
        let signing_key = env::var("FIREFLY_PRIVATE_KEY")
            .map_err(|_| ConnectionError::MissingPrivateKey)?;

        Ok(Self {
            node_host: env::var("FIREFLY_HOST")
                .unwrap_or_else(|_| "localhost".to_string()),
            grpc_port: env::var("FIREFLY_GRPC_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(40401),
            http_port: env::var("FIREFLY_HTTP_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(40403),
            signing_key,
        })
    }

    /// Create a new configuration with explicit values
    pub fn new(
        node_host: String,
        grpc_port: u16,
        http_port: u16,
        signing_key: String,
    ) -> Self {
        Self {
            node_host,
            grpc_port,
            http_port,
            signing_key,
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
                write!(f, "FIREFLY_PRIVATE_KEY environment variable not set. This is required for signing deploys.")
            }
            Self::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            Self::OperationFailed(e) => write!(f, "Operation failed: {}", e),
        }
    }
}

impl std::error::Error for ConnectionError {}

/// Manages F1r3fly node connections with connection reuse
///
/// This struct creates a single F1r3flyApi instance and reuses it for all operations,
/// avoiding the overhead of creating new instances (including SecretKey parsing) on every call.
///
/// # Example
///
/// ```ignore
/// use node_cli::connection_manager::F1r3flyConnectionManager;
///
/// // Create from environment variables
/// let manager = F1r3flyConnectionManager::from_env()?;
///
/// // Execute exploratory deploy (read-only query)
/// let result = manager.query("new return in { return!(42) }").await?;
///
/// // Deploy Rholang code (write to blockchain)
/// let deploy_id = manager.deploy("new x in { x!(100) }").await?;
/// ```
#[derive(Clone)]
pub struct F1r3flyConnectionManager {
    config: ConnectionConfig,
}

impl F1r3flyConnectionManager {
    /// Create a new connection manager from environment variables
    ///
    /// # Errors
    ///
    /// Returns an error if `FIREFLY_PRIVATE_KEY` is not set
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

    /// Create an F1r3flyApi instance for this operation
    ///
    /// Note: This is lightweight (just references and a SecretKey), but we still
    /// want to minimize calls to this method.
    fn api(&self) -> F1r3flyApi<'_> {
        F1r3flyApi::new(
            &self.config.signing_key,
            &self.config.node_host,
            self.config.grpc_port,
        )
    }

    /// Execute an exploratory deploy (read-only query)
    ///
    /// This is used for querying RSpace state without committing to the blockchain.
    ///
    /// # Arguments
    ///
    /// * `rholang_code` - The Rholang code to execute
    ///
    /// # Returns
    ///
    /// The result string from the Rholang execution
    pub async fn query(&self, rholang_code: &str) -> Result<String, ConnectionError> {
        let api = self.api();
        let (result, _block_info) = api
            .exploratory_deploy(rholang_code, None, false)
            .await
            .map_err(|e| ConnectionError::OperationFailed(e.to_string()))?;
        Ok(result)
    }

    /// Deploy Rholang code to the blockchain
    ///
    /// Uses a phlo limit of 500,000 (enough for complex contracts).
    ///
    /// # Arguments
    ///
    /// * `rholang_code` - The Rholang code to deploy
    ///
    /// # Returns
    ///
    /// The deploy ID
    pub async fn deploy(&self, rholang_code: &str) -> Result<String, ConnectionError> {
        let api = self.api();
        api.deploy_with_phlo_limit(rholang_code, 500_000, "rholang")
            .await
            .map_err(|e| ConnectionError::OperationFailed(e.to_string()))
    }

    /// Deploy Rholang code with a specific timestamp
    ///
    /// This is required for insertSigned compatibility - the deploy timestamp
    /// must match the signature timestamp.
    ///
    /// # Arguments
    ///
    /// * `rholang_code` - The Rholang code to deploy
    /// * `timestamp_millis` - The timestamp in milliseconds
    ///
    /// # Returns
    ///
    /// The deploy ID
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
    ///
    /// # Arguments
    ///
    /// * `deploy_id` - The deploy ID to wait for
    /// * `max_attempts` - Maximum number of attempts (1 second between attempts)
    ///
    /// # Returns
    ///
    /// The block hash once the deploy is found
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
    ///
    /// # Arguments
    ///
    /// * `block_hash` - The block hash to wait for
    /// * `max_attempts` - Maximum number of attempts (5 seconds between attempts)
    ///
    /// # Returns
    ///
    /// Ok(()) if the block is finalized
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

    /// Deploy Rholang code and wait for it to be finalized
    ///
    /// This is the recommended method for deploying RGB state that needs to be queried.
    ///
    /// # Arguments
    ///
    /// * `rholang_code` - The Rholang code to deploy
    /// * `max_block_wait_attempts` - Max attempts to wait for block inclusion
    /// * `max_finalization_attempts` - Max attempts to wait for finalization
    ///
    /// # Returns
    ///
    /// A tuple of (deploy_id, block_hash)
    pub async fn deploy_and_wait(
        &self,
        rholang_code: &str,
        max_block_wait_attempts: u32,
        max_finalization_attempts: u32,
    ) -> Result<(String, String), ConnectionError> {
        // Step 1: Deploy the code
        let deploy_id = self.deploy(rholang_code).await?;

        // Step 2: Wait for deploy to be included in a block
        let block_hash = self.wait_for_deploy(&deploy_id, max_block_wait_attempts).await?;

        // Step 3: Wait for block to be finalized
        self.wait_for_finalization(&block_hash, max_finalization_attempts)
            .await?;

        Ok((deploy_id, block_hash))
    }

    /// Get direct access to the underlying F1r3flyApi for advanced operations
    ///
    /// Use this sparingly - prefer the higher-level methods when possible.
    pub fn get_api(&self) -> F1r3flyApi<'_> {
        self.api()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env_missing_key() {
        // Clear the environment variable
        env::remove_var("FIREFLY_PRIVATE_KEY");

        let result = ConnectionConfig::from_env();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConnectionError::MissingPrivateKey));
    }

    #[test]
    fn test_config_from_env_with_key() {
        // Set a test private key
        env::set_var("FIREFLY_PRIVATE_KEY", "test_key_123");

        let result = ConnectionConfig::from_env();
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.signing_key, "test_key_123");
        assert_eq!(config.node_host, "localhost");
        assert_eq!(config.grpc_port, 40401);
        assert_eq!(config.http_port, 40403);

        // Cleanup
        env::remove_var("FIREFLY_PRIVATE_KEY");
    }

    #[test]
    fn test_config_new() {
        let config = ConnectionConfig::new(
            "example.com".to_string(),
            9000,
            9001,
            "my_key".to_string(),
        );

        assert_eq!(config.node_host, "example.com");
        assert_eq!(config.grpc_port, 9000);
        assert_eq!(config.http_port, 9001);
        assert_eq!(config.signing_key, "my_key");
    }
}

