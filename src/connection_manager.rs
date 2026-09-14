/// F1r3fly Connection Manager
///
/// Manages connections to F1r3fly nodes with connection reuse and pooling.
/// Provides a high-level async API for deploying Rholang code and querying state.
use crate::f1r3fly_api::F1r3flyApi;
use crate::utils::CryptoUtils;
use crate::vault::{build_transfer_rholang, TransferResult};
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
    /// Observer node hostname for finalization checks (defaults to node_host)
    pub observer_host: Option<String>,
    /// Observer node gRPC port for finalization checks (defaults to 40452)
    pub observer_grpc_port: u16,
    /// Observer node HTTP port, for endpoints the validators refuse — a shard
    /// serves exploratory deploys only from its read-only node (defaults to 40453)
    pub observer_http_port: u16,
    /// Maximum seconds to wait for a deploy to finalize (default: 90)
    pub finalization_timeout_secs: u32,
    /// Seconds between finalization status polls (default: 2)
    pub poll_interval_secs: u64,
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
    /// - `FIREFLY_FINALIZATION_TIMEOUT`: Max seconds to wait for a deploy to finalize (default: 90)
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
            observer_host: env::var("FIREFLY_OBSERVER_HOST").ok(),
            observer_grpc_port: env::var("FIREFLY_OBSERVER_GRPC_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(40452),
            observer_http_port: env::var("FIREFLY_OBSERVER_HTTP_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(40453),
            finalization_timeout_secs: env::var("FIREFLY_FINALIZATION_TIMEOUT")
                .ok()
                .and_then(|t| t.parse().ok())
                .unwrap_or(90),
            poll_interval_secs: 2,
        })
    }

    /// Create a new configuration with explicit values
    pub fn new(node_host: String, grpc_port: u16, http_port: u16, signing_key: String) -> Self {
        Self {
            node_host,
            grpc_port,
            http_port,
            signing_key,
            observer_host: None,
            observer_grpc_port: 40452,
            observer_http_port: 40453,
            finalization_timeout_secs: 90,
            poll_interval_secs: 2,
        }
    }

    /// Set observer node for finalization checks and read-only endpoints.
    ///
    /// Both ports are explicit: a shard maps them independently, and deriving
    /// one from the other silently sends requests to the wrong port.
    pub fn with_observer(mut self, host: String, grpc_port: u16, http_port: u16) -> Self {
        self.observer_host = Some(host);
        self.observer_grpc_port = grpc_port;
        self.observer_http_port = http_port;
        self
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
            Self::ConnectionFailed(e) => write!(f, "Connection failed: {e}"),
            Self::OperationFailed(e) => write!(f, "Operation failed: {e}"),
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

    fn api(&self) -> Result<F1r3flyApi<'_>, ConnectionError> {
        F1r3flyApi::new(
            &self.config.signing_key,
            &self.config.node_host,
            self.config.grpc_port,
        )
        .map_err(|e| ConnectionError::ConnectionFailed(e.to_string()))
    }

    fn observer_api(&self) -> Result<F1r3flyApi<'_>, ConnectionError> {
        let host = self
            .config
            .observer_host
            .as_deref()
            .unwrap_or(&self.config.node_host);
        F1r3flyApi::new(
            &self.config.signing_key,
            host,
            self.config.observer_grpc_port,
        )
        .map_err(|e| ConnectionError::ConnectionFailed(e.to_string()))
    }

    /// Execute an exploratory deploy (read-only query)
    pub async fn query(&self, rholang_code: &str) -> Result<String, ConnectionError> {
        let api = self.api()?;
        let (result, _block_info, _cost) = api
            .exploratory_deploy(rholang_code, None, false)
            .await
            .map_err(|e| ConnectionError::OperationFailed(e.to_string()))?;
        Ok(result)
    }

    /// Estimate phlogiston cost of Rholang code via the HTTP
    pub async fn estimate_cost(
        &self,
        rholang_code: &str,
        deployer_pubkey_hex: Option<&str>,
    ) -> Result<u64, ConnectionError> {
        let api = self.api()?;
        let response = api
            .estimate_cost(
                rholang_code,
                deployer_pubkey_hex,
                None,
                self.config.http_port,
            )
            .await
            .map_err(|e| ConnectionError::OperationFailed(e.to_string()))?;
        Ok(response)
    }

    /// Deploy Rholang code to the blockchain
    pub async fn deploy(&self, rholang_code: &str) -> Result<String, ConnectionError> {
        let api = self.api()?;
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
        let api = self.api()?;
        api.deploy_with_timestamp_and_phlo_limit(
            rholang_code,
            "rholang",
            Some(timestamp_millis),
            500_000,
        )
        .await
        .map_err(|e| ConnectionError::OperationFailed(e.to_string()))
    }

    /// Wait for a deploy to reach a terminal finalization state (`Finalized` /
    /// `Failed` / `Expired`) via `/api/deploy-finalization-status/{sig}`.
    ///
    /// Queries the observer first and the deploy node when the observer does not
    /// answer. Fails immediately when neither node serves the endpoint, and when
    /// the budget elapses without a terminal state.
    pub async fn wait_for_deploy_finalization(
        &self,
        deploy_sig_hex: &str,
        total_timeout_secs: u64,
        poll_interval_secs: u64,
    ) -> Result<crate::f1r3fly_api::DeployFinalizationStatus, ConnectionError> {
        let observer_api = self.observer_api()?;
        let node_api = self.api();
        let max_attempts = (total_timeout_secs / poll_interval_secs.max(1)).max(1) as u32;

        for attempt in 1..=max_attempts {
            // Map errors to String immediately: the un-Send `Box<dyn Error>`
            // must not live across the poll sleep below, or every future built
            // on this loop (transfer, deploy_and_wait) stops being Send.
            let observer = observer_api
                .deploy_finalization_status(deploy_sig_hex, self.config.observer_http_port)
                .await
                .map_err(|e| e.to_string());

            let status = match observer {
                Ok(Some(status)) => Some(status),
                observer => {
                    let node = match &node_api {
                        Ok(api) => Some(
                            api.deploy_finalization_status(deploy_sig_hex, self.config.http_port)
                                .await
                                .map_err(|e| e.to_string()),
                        ),
                        Err(_) => None,
                    };
                    match (observer, node) {
                        (_, Some(Ok(Some(status)))) => Some(status),
                        (Ok(None), Some(Ok(None)) | None) => {
                            return Err(ConnectionError::OperationFailed(
                                "the node does not serve /api/deploy-finalization-status \
                                 (requires f1r3node-rust v0.4.15 or later)"
                                    .to_string(),
                            ));
                        }
                        (observer, node) => {
                            tracing::warn!(
                                deploy_sig = deploy_sig_hex,
                                attempt,
                                observer = ?observer,
                                node = ?node,
                                "deploy-finalization-status unavailable; will retry"
                            );
                            None
                        }
                    }
                }
            };

            if let Some(status) = status.filter(|status| status.is_terminal()) {
                tracing::debug!(
                    deploy_sig = deploy_sig_hex,
                    state = %status.state,
                    attempt,
                    "Deploy reached terminal state"
                );
                return Ok(status);
            }

            if attempt < max_attempts {
                tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval_secs)).await;
            }
        }

        Err(ConnectionError::OperationFailed(format!(
            "Deploy {deploy_sig_hex} did not reach terminal state within {total_timeout_secs}s"
        )))
    }

    /// Deploy Rholang code, wait for canonical finalization, and read result.
    ///
    /// Polls `/api/deploy-finalization-status`, which reports whether the
    /// deploy's effects are in canonical state rather than whether some
    /// containing block finalized. A node that does not serve the endpoint is
    /// an error.
    pub async fn deploy_and_wait(
        &self,
        rholang_code: &str,
        bigger_phlo: bool,
        expiration_timestamp: i64,
    ) -> Result<crate::f1r3fly_api::DeployResult, ConnectionError> {
        let phlo_limit: i64 = if bigger_phlo { 5_000_000_000 } else { 50_000 };
        self.deploy_and_wait_with_phlo_limit(rholang_code, phlo_limit, expiration_timestamp)
            .await
    }

    /// Same as [`deploy_and_wait`](Self::deploy_and_wait) but with an explicit
    /// phlo limit instead of the 50k/5B `bigger_phlo` mapping. Use when the
    /// deploy's cost is known to exceed 50k phlo but a 5B limit would demand
    /// more balance than the deployer holds (e.g. vault transfers).
    pub async fn deploy_and_wait_with_phlo_limit(
        &self,
        rholang_code: &str,
        phlo_limit: i64,
        expiration_timestamp: i64,
    ) -> Result<crate::f1r3fly_api::DeployResult, ConnectionError> {
        let api = self.api()?;

        // Phase 1: Deploy
        let deploy_id = api
            .deploy_with_phlo_limit_and_expiration(
                rholang_code,
                phlo_limit,
                "rholang",
                expiration_timestamp,
            )
            .await
            .map_err(|e| ConnectionError::OperationFailed(format!("Deploy failed: {e}")))?;
        tracing::info!(deploy_id = %deploy_id, "Deploy submitted");

        // Phase 2: Wait for a terminal finalization state
        let status = self
            .wait_for_deploy_finalization(
                &deploy_id,
                self.config.finalization_timeout_secs as u64,
                self.config.poll_interval_secs,
            )
            .await?;
        let block_hash = match status.state.as_str() {
            "Finalized" => {
                let block_hash = status.latest_block_hash.ok_or_else(|| {
                    ConnectionError::OperationFailed(
                        "Finalized state without latest_block_hash (node bug)".to_string(),
                    )
                })?;
                tracing::info!(block_hash = %block_hash, "Deploy canonically finalized");
                block_hash
            }
            "Failed" => {
                return Err(ConnectionError::OperationFailed(format!(
                    "Deploy {deploy_id} failed during execution (Rholang error or insufficient phlo)"
                )));
            }
            "Expired" => {
                return Err(ConnectionError::OperationFailed(format!(
                    "Deploy {deploy_id} expired without canonical inclusion"
                )));
            }
            other => {
                return Err(ConnectionError::OperationFailed(format!(
                    "Deploy {deploy_id} reached unexpected state {other}"
                )));
            }
        };

        // Phase 3: Read deploy result AFTER finalization. A deploy that writes
        // nothing to deployId reads as an empty payload, so every error is a
        // failed read.
        let data = api
            .get_data_at_deploy_id(&deploy_id, &block_hash)
            .await
            .map_err(|e| {
                tracing::warn!(deploy_id = %deploy_id, error = %e, "Failed to read deployId data");
                format!("reading deployId data at block {block_hash}: {e}")
            });

        // Phase 4: Get deploy execution details
        // May fail on older nodes that don't support ?view=detail
        let detail = match api
            .get_deploy_detail(&deploy_id, self.config.http_port)
            .await
        {
            Ok(detail) => detail,
            Err(e) => {
                tracing::info!("Deploy detail not available: {}", e);
                None
            }
        };

        Ok(crate::f1r3fly_api::DeployResult {
            deploy_id,
            block_hash,
            block_number: detail.as_ref().map(|d| d.block_number),
            cost: detail.as_ref().map(|d| d.cost),
            errored: detail.as_ref().map(|d| d.errored).unwrap_or(false),
            system_deploy_error: detail
                .and_then(|d| d.system_deploy_error.filter(|s| !s.is_empty())),
            data,
        })
    }

    /// Get direct access to the underlying F1r3flyApi
    pub fn get_api(&self) -> Result<F1r3flyApi<'_>, ConnectionError> {
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
        crate::vault::validate_address(to_address).map_err(ConnectionError::OperationFailed)?;

        let from_address = self.get_address()?;

        log::info!(
            "Transferring {} dust ({:.8} tokens) from {} to {}",
            amount_dust,
            crate::vault::dust_to_tokens(amount_dust),
            from_address,
            to_address
        );

        let rholang = build_transfer_rholang(&from_address, to_address, amount_dust);

        let result = self
            .deploy_and_wait_with_phlo_limit(&rholang, crate::vault::TRANSFER_PHLO_LIMIT, 0)
            .await?;

        crate::vault::check_transfer_result(&result).map_err(|e| {
            ConnectionError::OperationFailed(format!("{e} (deploy {})", result.deploy_id))
        })?;

        tracing::info!(
        deploy_id = %result.deploy_id,
        to_address,
        amount_dust,
        "Transfer complete"
        );

        Ok(TransferResult {
            deploy_id: result.deploy_id,
            block_hash: result.block_hash,
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
