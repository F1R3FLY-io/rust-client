/// F1r3fly Connection Manager
///
/// Manages connections to F1r3fly nodes with connection reuse and pooling.
/// Provides a high-level async API for deploying Rholang code and querying state.
use crate::error::{Error, Result};
use crate::f1r3fly_api::{DeployFinalizationStatus, DeployResult, F1r3flyApi};
use crate::utils::CryptoUtils;
use crate::vault::{build_transfer_rholang, TransferResult};
use log;
use secp256k1::PublicKey;
use std::env;
use std::str::FromStr;
use std::time::Duration;

fn env_opt(var: &'static str) -> Result<Option<String>> {
    match env::var(var) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(value)) => {
            Err(Error::invalid(var, format!("{value:?} is not unicode")))
        }
    }
}

/// `var` parsed, or `default` when it is unset. A set but malformed value is
/// an error rather than a silent fallback.
fn env_or<T: FromStr>(var: &'static str, default: T) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    match env_opt(var)? {
        Some(value) => value
            .parse()
            .map_err(|e| Error::invalid(var, format!("{value:?}: {e}"))),
        None => Ok(default),
    }
}

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
    pub fn from_env() -> Result<Self> {
        let signing_key = env_opt("FIREFLY_PRIVATE_KEY")?.ok_or(Error::MissingConfig {
            var: "FIREFLY_PRIVATE_KEY",
        })?;

        Ok(Self {
            node_host: env_or("FIREFLY_HOST", "localhost".to_string())?,
            grpc_port: env_or("FIREFLY_GRPC_PORT", 40401)?,
            http_port: env_or("FIREFLY_HTTP_PORT", 40403)?,
            signing_key,
            observer_host: env_opt("FIREFLY_OBSERVER_HOST")?,
            observer_grpc_port: env_or("FIREFLY_OBSERVER_GRPC_PORT", 40452)?,
            observer_http_port: env_or("FIREFLY_OBSERVER_HTTP_PORT", 40453)?,
            finalization_timeout_secs: env_or("FIREFLY_FINALIZATION_TIMEOUT", 90)?,
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

type StatusAnswer = Result<Option<DeployFinalizationStatus>>;

/// What one poll of the deploy node and the observer established.
#[derive(Debug)]
enum PollOutcome {
    Terminal(DeployFinalizationStatus),
    /// `included`: an answer names a block carrying the deploy.
    Pending {
        included: bool,
    },
    /// The deploy node cannot serve the endpoint and the observer gave no answer.
    Unserved,
}

/// Warn when a node starts failing or fails differently; repeats of the same
/// failure on later polls go to debug, so an unreachable observer does not
/// print a warning on every poll.
fn report_unavailable(
    source: &str,
    answer: &StatusAnswer,
    last_error: &mut Option<String>,
    deploy_sig_hex: &str,
    attempt: u32,
) {
    match answer {
        Err(error) => {
            let error = error.to_string();
            if last_error.as_ref() == Some(&error) {
                tracing::debug!(deploy_sig = deploy_sig_hex, attempt, source, %error,
                    "deploy-finalization-status still unavailable");
            } else {
                tracing::warn!(deploy_sig = deploy_sig_hex, attempt, source, %error,
                    "deploy-finalization-status unavailable; will retry");
                *last_error = Some(error);
            }
        }
        Ok(_) => *last_error = None,
    }
}

/// Terminal states are write-once on a node, so whichever node reports one first
/// has the verdict. Two nodes reporting different terminal states is an error,
/// never a choice. A 404 (`Ok(None)`) from the deploy node is conclusive unless
/// the observer answered.
fn poll_outcome(
    deploy_id: &str,
    node: StatusAnswer,
    observer: StatusAnswer,
) -> Result<PollOutcome> {
    let node_unserved = matches!(node, Ok(None));
    let node = node.ok().flatten();
    let observer = observer.ok().flatten();

    let terminal = |status: &Option<DeployFinalizationStatus>| {
        status
            .as_ref()
            .filter(|status| status.is_terminal())
            .cloned()
    };
    match (terminal(&node), terminal(&observer)) {
        (Some(from_node), Some(from_observer)) if from_node.state != from_observer.state => {
            return Err(Error::ConflictingVerdicts {
                deploy_id: deploy_id.to_string(),
                node: from_node.state,
                observer: from_observer.state,
            });
        }
        (Some(status), _) | (None, Some(status)) => return Ok(PollOutcome::Terminal(status)),
        (None, None) => {}
    }

    if node_unserved && observer.is_none() {
        return Ok(PollOutcome::Unserved);
    }
    let included = [&node, &observer]
        .into_iter()
        .flatten()
        .any(|status| status.latest_block_hash.is_some());
    Ok(PollOutcome::Pending { included })
}

/// Manages F1r3fly node connections with connection reuse
#[derive(Clone)]
pub struct F1r3flyConnectionManager {
    config: ConnectionConfig,
}

impl F1r3flyConnectionManager {
    /// Create a new connection manager from environment variables
    pub fn from_env() -> Result<Self> {
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

    fn api(&self) -> Result<F1r3flyApi<'_>> {
        F1r3flyApi::new(
            &self.config.signing_key,
            &self.config.node_host,
            self.config.grpc_port,
        )
    }

    fn observer_host(&self) -> &str {
        self.config
            .observer_host
            .as_deref()
            .unwrap_or(&self.config.node_host)
    }

    fn observer_api(&self) -> Result<F1r3flyApi<'_>> {
        F1r3flyApi::new(
            &self.config.signing_key,
            self.observer_host(),
            self.config.observer_grpc_port,
        )
    }

    /// Execute an exploratory deploy (read-only query)
    pub async fn query(&self, rholang_code: &str) -> Result<String> {
        let (result, _block_info, _cost) = self
            .api()?
            .exploratory_deploy(rholang_code, None, false)
            .await?;
        Ok(result)
    }

    /// Estimate phlogiston cost of Rholang code via the HTTP
    pub async fn estimate_cost(
        &self,
        rholang_code: &str,
        deployer_pubkey_hex: Option<&str>,
    ) -> Result<u64> {
        self.api()?
            .estimate_cost(
                rholang_code,
                deployer_pubkey_hex,
                None,
                self.config.http_port,
            )
            .await
    }

    /// Deploy Rholang code to the blockchain
    pub async fn deploy(&self, rholang_code: &str) -> Result<String> {
        self.api()?
            .deploy_with_phlo_limit(rholang_code, 500_000, "rholang")
            .await
    }

    /// Deploy Rholang code with a specific timestamp
    ///
    /// Required for insertSigned compatibility where the deploy timestamp
    /// must match the signature timestamp.
    pub async fn deploy_with_timestamp(
        &self,
        rholang_code: &str,
        timestamp_millis: i64,
    ) -> Result<String> {
        self.api()?
            .deploy_with_timestamp_and_phlo_limit(
                rholang_code,
                "rholang",
                Some(timestamp_millis),
                500_000,
            )
            .await
    }

    /// Wait for a deploy to reach a terminal finalization state (`Finalized` /
    /// `Failed` / `Expired`) via `/api/deploy-finalization-status/{sig}`.
    ///
    /// Each poll asks the deploy node and the observer; the first terminal
    /// state either reports is the verdict. Fails immediately when the deploy
    /// node does not serve the endpoint and the observer gave no answer, and
    /// when the budget elapses without a terminal state.
    pub async fn wait_for_deploy_finalization(
        &self,
        deploy_sig_hex: &str,
        total_timeout_secs: u64,
        poll_interval_secs: u64,
    ) -> Result<DeployFinalizationStatus> {
        let node_api = self.api()?;
        let observer_api = self.observer_api()?;
        let max_attempts = (total_timeout_secs / poll_interval_secs.max(1)).max(1) as u32;
        let mut execution_failure: Option<String> = None;
        let mut node_error = None;
        let mut observer_error = None;

        for attempt in 1..=max_attempts {
            let (node, observer) = tokio::join!(
                node_api.deploy_finalization_status(deploy_sig_hex, self.config.http_port),
                observer_api
                    .deploy_finalization_status(deploy_sig_hex, self.config.observer_http_port),
            );
            report_unavailable(
                "deploy node",
                &node,
                &mut node_error,
                deploy_sig_hex,
                attempt,
            );
            report_unavailable(
                "observer",
                &observer,
                &mut observer_error,
                deploy_sig_hex,
                attempt,
            );

            let included = match poll_outcome(deploy_sig_hex, node, observer)? {
                PollOutcome::Terminal(status) => {
                    tracing::debug!(
                        deploy_sig = deploy_sig_hex,
                        state = %status.state,
                        attempt,
                        "Deploy reached terminal state"
                    );
                    return Ok(status);
                }
                PollOutcome::Unserved => {
                    return Err(Error::NodeTooOld {
                        endpoint: format!(
                            "http://{}:{}",
                            self.config.node_host, self.config.http_port
                        ),
                        feature: "/api/deploy-finalization-status",
                        requirement: "f1r3node-rust v0.4.15 or later",
                    });
                }
                PollOutcome::Pending { included } => included,
            };

            if included && execution_failure.is_none() {
                execution_failure = self.failed_execution(&node_api, deploy_sig_hex).await;
                if let Some(description) = &execution_failure {
                    tracing::warn!(
                        deploy_sig = deploy_sig_hex,
                        "Deploy executed with an error in {description}; \
                         waiting for the verdict"
                    );
                }
            }

            if attempt < max_attempts {
                tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval_secs)).await;
            }
        }

        Err(Error::FinalizationTimeout {
            deploy_id: deploy_sig_hex.to_string(),
            waited: Duration::from_secs(total_timeout_secs),
            execution: execution_failure,
        })
    }

    /// The deploy node's description of a failed execution, as `block N (cost C): reason`.
    async fn failed_execution(&self, api: &F1r3flyApi<'_>, deploy_sig_hex: &str) -> Option<String> {
        match api
            .get_deploy_detail(deploy_sig_hex, self.config.http_port)
            .await
        {
            Ok(Some(detail)) => detail.execution_failure().map(|failure| {
                format!(
                    "block {} (cost {}): {failure}",
                    detail.block_number, detail.cost
                )
            }),
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(deploy_sig = deploy_sig_hex, %error, "Could not read deploy detail");
                None
            }
        }
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
    ) -> Result<DeployResult> {
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
    ) -> Result<DeployResult> {
        let api = self.api()?;

        // Phase 1: Deploy
        let deploy_id = api
            .deploy_with_phlo_limit_and_expiration(
                rholang_code,
                phlo_limit,
                "rholang",
                expiration_timestamp,
            )
            .await?;
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
                    Error::decode(
                        "deploy finalization status",
                        "Finalized without latest_block_hash",
                    )
                })?;
                tracing::info!(block_hash = %block_hash, "Deploy canonically finalized");
                block_hash
            }
            "Failed" => {
                return Err(Error::DeployFailed {
                    execution: self.failed_execution(&api, &deploy_id).await,
                    deploy_id,
                });
            }
            "Expired" => return Err(Error::DeployExpired { deploy_id }),
            other => {
                return Err(Error::decode(
                    "deploy finalization state",
                    format!("{other} for deploy {deploy_id}"),
                ));
            }
        };

        // Phase 3: Read deploy result AFTER finalization. A deploy that writes
        // nothing to deployId reads as an empty payload, so every error is a
        // failed read.
        // A failed read must not look retryable: the deploy is already final.
        let data = api
            .get_data_at_deploy_id(&deploy_id, &block_hash)
            .await
            .map_err(|cause| {
                tracing::warn!(deploy_id = %deploy_id, %block_hash, %cause, "Failed to read deployId data");
                Error::DeployResultUnreadable {
                    deploy_id: deploy_id.clone(),
                    block_hash: block_hash.clone(),
                    cause: Box::new(cause),
                }
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

        Ok(DeployResult {
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
    pub fn get_api(&self) -> Result<F1r3flyApi<'_>> {
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
    /// * `amount_dust` - Amount in raw base units
    pub async fn transfer(
        &self,
        to_address: &str,
        amount_dust: num_bigint::BigUint,
    ) -> Result<TransferResult> {
        crate::vault::validate_address(to_address)?;

        let from_address = self.get_address()?;

        log::info!("Transferring {amount_dust} raw units from {from_address} to {to_address}");

        let rholang = build_transfer_rholang(&from_address, to_address, &amount_dust)?;

        let result = self
            .deploy_and_wait_with_phlo_limit(&rholang, crate::vault::TRANSFER_PHLO_LIMIT, 0)
            .await?;
        let deploy_id = result.deploy_id.clone();
        let block_hash = result.block_hash.clone();
        crate::vault::check_transfer_result(result)?;

        tracing::info!(
        deploy_id = %deploy_id,
        to_address,
        amount_dust = %amount_dust,
        "Transfer complete"
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
    pub fn get_address(&self) -> Result<String> {
        let public_key = self.get_public_key()?;
        let pubkey_hex = hex::encode(public_key.serialize_uncompressed());
        CryptoUtils::generate_vault_address(&pubkey_hex)
    }

    /// Get the public key for this connection's signing key
    pub fn get_public_key(&self) -> Result<PublicKey> {
        let secret_key = CryptoUtils::decode_private_key(&self.config.signing_key)?;
        Ok(CryptoUtils::derive_public_key(&secret_key))
    }

    /// Get the public key as hex string (uncompressed format)
    pub fn get_public_key_hex(&self) -> Result<String> {
        let public_key = self.get_public_key()?;
        Ok(hex::encode(public_key.serialize_uncompressed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransportError;

    #[test]
    fn test_config_from_env_missing_key() {
        env::remove_var("FIREFLY_PRIVATE_KEY");
        assert!(matches!(
            ConnectionConfig::from_env(),
            Err(Error::MissingConfig {
                var: "FIREFLY_PRIVATE_KEY"
            })
        ));
    }

    #[test]
    fn a_malformed_variable_is_an_error_not_the_default() {
        assert!(matches!(
            env_or::<u16>("PATH", 40401),
            Err(Error::InvalidInput { what: "PATH", .. })
        ));
        assert_eq!(
            env_or::<u16>("NODE_CLI_TEST_SURELY_UNSET_VARIABLE", 40401).unwrap(),
            40401
        );
    }

    fn assert_send<T: Send>(_: &T) {}

    #[test]
    fn public_futures_are_send() {
        let manager = F1r3flyConnectionManager::new(ConnectionConfig::new(
            "localhost".into(),
            40412,
            40413,
            "01".repeat(32),
        ));
        assert_send(&manager.transfer("1111", 1u8.into()));
        assert_send(&manager.deploy_and_wait("Nil", false, 0));
        assert_send(&manager.wait_for_deploy_finalization("sig", 1, 1));
        assert_send(&manager.query("Nil"));
        let api = F1r3flyApi::new_readonly("localhost", 40452);
        assert_send(&api.exploratory_deploy("Nil", None, false));
        assert_send(&api.propose());
        assert_send(&api.show_main_chain(1));
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

    fn status(state: &str, block: Option<&str>) -> StatusAnswer {
        Ok(Some(DeployFinalizationStatus {
            state: state.to_string(),
            rejection_count: 0,
            latest_block_hash: block.map(str::to_string),
        }))
    }

    fn timed_out(endpoint: &str) -> StatusAnswer {
        Err(Error::transport(
            endpoint,
            TransportError::TimedOut(Duration::from_secs(1)),
        ))
    }

    fn unreachable() -> StatusAnswer {
        timed_out("http://observer:40453")
    }

    fn poll_outcome(node: StatusAnswer, observer: StatusAnswer) -> Result<PollOutcome> {
        super::poll_outcome("sig", node, observer)
    }

    #[test]
    fn a_terminal_state_from_the_deploy_node_ends_the_wait_while_the_observer_is_pending() {
        let outcome = poll_outcome(status("Failed", Some("b1")), status("Pending", Some("b1")));
        assert!(
            matches!(outcome, Ok(PollOutcome::Terminal(ref s)) if s.state == "Failed"),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_terminal_state_from_the_observer_ends_the_wait_while_the_node_is_unreachable() {
        let outcome = poll_outcome(unreachable(), status("Finalized", Some("b1")));
        assert!(
            matches!(outcome, Ok(PollOutcome::Terminal(ref s)) if s.state == "Finalized"),
            "{outcome:?}"
        );
    }

    #[test]
    fn conflicting_terminal_states_are_an_error() {
        let outcome = poll_outcome(
            status("Failed", Some("b1")),
            status("Finalized", Some("b1")),
        );
        assert!(
            matches!(outcome, Err(Error::ConflictingVerdicts { ref node, ref observer, .. })
                if node == "Failed" && observer == "Finalized"),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_deploy_node_that_does_not_serve_the_endpoint_is_conclusive_when_the_observer_is_unreachable(
    ) {
        assert!(matches!(
            poll_outcome(Ok(None), unreachable()),
            Ok(PollOutcome::Unserved)
        ));
        assert!(matches!(
            poll_outcome(Ok(None), Ok(None)),
            Ok(PollOutcome::Unserved)
        ));
    }

    #[test]
    fn an_answering_observer_keeps_the_wait_alive_when_the_deploy_node_does_not_serve_the_endpoint()
    {
        assert!(matches!(
            poll_outcome(Ok(None), status("Pending", None)),
            Ok(PollOutcome::Pending { included: false })
        ));
    }

    #[test]
    fn a_failure_is_remembered_until_the_node_answers_again() {
        let first = unreachable().unwrap_err().to_string();
        let mut last = None;
        report_unavailable("observer", &unreachable(), &mut last, "sig", 1);
        assert_eq!(last.as_deref(), Some(first.as_str()));
        report_unavailable("observer", &unreachable(), &mut last, "sig", 2);
        assert_eq!(last.as_deref(), Some(first.as_str()));
        report_unavailable("observer", &status("Pending", None), &mut last, "sig", 3);
        assert_eq!(last, None);
        let other = timed_out("http://elsewhere:40453");
        report_unavailable("observer", &other, &mut last, "sig", 4);
        assert_eq!(last, Some(other.unwrap_err().to_string()));
    }

    #[test]
    fn a_pending_answer_naming_a_block_marks_the_deploy_included() {
        assert!(matches!(
            poll_outcome(status("Pending", None), status("Pending", Some("b1"))),
            Ok(PollOutcome::Pending { included: true })
        ));
        assert!(matches!(
            poll_outcome(unreachable(), unreachable()),
            Ok(PollOutcome::Pending { included: false })
        ));
    }
}
