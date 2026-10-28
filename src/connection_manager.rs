/// F1r3fly Connection Manager
///
/// Manages connections to F1r3fly nodes with connection reuse and pooling.
/// Provides a high-level async API for deploying Rholang code and querying state.
use crate::deploy::{DeployOptions, SignedDeploy, DEFAULT_SHARD_ID};
use crate::endpoint::Endpoint;
use crate::error::{Error, Result};
use crate::f1r3fly_api::{DeployDetail, DeployFinalizationStatus, DeployResult, F1r3flyApi};
use crate::node_http::NodeHttp;
use crate::utils::CryptoUtils;
use crate::vault::{build_transfer_rholang, TransferResult};
use log;
use secp256k1::PublicKey;
use std::env;
use std::str::FromStr;
use std::time::Duration;

/// A node's own gRPC port, on this host.
pub const DEFAULT_GRPC_URL: &str = "http://localhost:40401";
/// A node's own HTTP port, on this host.
pub const DEFAULT_HTTP_URL: &str = "http://localhost:40403";

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

/// Variables this crate no longer reads, with what replaced each.
const REMOVED_VARIABLES: [(&str, &str); 6] = [
    ("FIREFLY_HOST", "FIREFLY_GRPC_URL and FIREFLY_HTTP_URL"),
    ("FIREFLY_GRPC_PORT", "FIREFLY_GRPC_URL"),
    ("FIREFLY_HTTP_PORT", "FIREFLY_HTTP_URL"),
    ("FIREFLY_OBSERVER_HOST", "FIREFLY_OBSERVER_URL"),
    (
        "FIREFLY_OBSERVER_GRPC_PORT",
        "nothing: the observer is polled over HTTP only",
    ),
    ("FIREFLY_OBSERVER_HTTP_PORT", "FIREFLY_OBSERVER_URL"),
];

/// Refuse a set variable that is no longer read, which would otherwise leave
/// the client quietly talking to the default endpoints.
fn refuse_removed_variables(is_set: impl Fn(&str) -> bool) -> Result<()> {
    match REMOVED_VARIABLES.iter().find(|(var, _)| is_set(var)) {
        Some((var, replacement)) => Err(Error::invalid(
            var,
            format!("is no longer read; use {replacement}"),
        )),
        None => Ok(()),
    }
}

/// A timeout of `var` whole seconds, or `default` when it is unset. Zero is
/// refused: it would fail every request.
fn env_timeout(var: &'static str, default: Duration) -> Result<Duration> {
    match env_or::<u64>(var, default.as_secs())? {
        0 => Err(Error::invalid(var, "a timeout must be at least 1 second")),
        secs => Ok(Duration::from_secs(secs)),
    }
}

/// Configuration for F1r3fly node connection
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// The deploy node's gRPC API: deploys go here
    pub grpc: Endpoint,
    /// The deploy node's HTTP API: finalization status and deploy details
    pub http: Endpoint,
    /// A second node's HTTP API, polled alongside the deploy node for a
    /// deploy's verdict. Without one, only the deploy node is polled.
    pub observer: Option<Endpoint>,
    pub signing_key: String,
    /// Maximum seconds to wait for a deploy to finalize (default: 90)
    pub finalization_timeout_secs: u32,
    /// Seconds between finalization status polls (default: 2)
    pub poll_interval_secs: u64,
    /// Shard deploys are signed for: the node's `casper.shard-name` (default: `root`)
    pub shard_id: String,
}

impl ConnectionConfig {
    /// Load configuration from environment variables
    ///
    /// # Environment Variables
    ///
    /// - `FIREFLY_PRIVATE_KEY`: Private key for signing (REQUIRED)
    /// - `FIREFLY_GRPC_URL`: Deploy node's gRPC endpoint (default: [`DEFAULT_GRPC_URL`])
    /// - `FIREFLY_HTTP_URL`: Deploy node's HTTP endpoint (default: [`DEFAULT_HTTP_URL`])
    /// - `FIREFLY_OBSERVER_URL`: Observer's HTTP endpoint (default: none)
    /// - `FIREFLY_CA_CERT`: PEM file of the certificate authorities every
    ///   `https` endpoint trusts, in place of the bundled public roots
    /// - `FIREFLY_CONNECT_TIMEOUT`: Seconds a connection may take to establish
    ///   (default: [`crate::endpoint::DEFAULT_CONNECT_TIMEOUT`])
    /// - `FIREFLY_REQUEST_TIMEOUT`: Seconds one request may take
    ///   (default: [`crate::endpoint::DEFAULT_REQUEST_TIMEOUT`])
    /// - `FIREFLY_FINALIZATION_TIMEOUT`: Max seconds to wait for a deploy to finalize (default: 90)
    /// - `FIREFLY_SHARD_ID`: Shard deploys are signed for (default: "root")
    pub fn from_env() -> Result<Self> {
        refuse_removed_variables(|var| env::var_os(var).is_some())?;
        let signing_key = env_opt("FIREFLY_PRIVATE_KEY")?.ok_or(Error::MissingConfig {
            var: "FIREFLY_PRIVATE_KEY",
        })?;
        let ca_pem = env_opt("FIREFLY_CA_CERT")?
            .map(|path| {
                std::fs::read(&path).map_err(|cause| Error::Io {
                    path: path.into(),
                    cause,
                })
            })
            .transpose()?;
        let connect_timeout = env_timeout(
            "FIREFLY_CONNECT_TIMEOUT",
            crate::endpoint::DEFAULT_CONNECT_TIMEOUT,
        )?;
        let request_timeout = env_timeout(
            "FIREFLY_REQUEST_TIMEOUT",
            crate::endpoint::DEFAULT_REQUEST_TIMEOUT,
        )?;
        let endpoint = |var: &'static str, url: &str| -> Result<Endpoint> {
            let endpoint = Endpoint::parse(url)
                .map_err(|e| Error::invalid(var, e))?
                .with_connect_timeout(connect_timeout)
                .with_request_timeout(request_timeout);
            match &ca_pem {
                Some(pem) => endpoint.with_ca_certificates(pem.clone()),
                None => Ok(endpoint),
            }
        };

        Ok(Self {
            grpc: endpoint(
                "FIREFLY_GRPC_URL",
                &env_or("FIREFLY_GRPC_URL", DEFAULT_GRPC_URL.to_string())?,
            )?,
            http: endpoint(
                "FIREFLY_HTTP_URL",
                &env_or("FIREFLY_HTTP_URL", DEFAULT_HTTP_URL.to_string())?,
            )?,
            observer: env_opt("FIREFLY_OBSERVER_URL")?
                .map(|url| endpoint("FIREFLY_OBSERVER_URL", &url))
                .transpose()?,
            signing_key,
            finalization_timeout_secs: env_or("FIREFLY_FINALIZATION_TIMEOUT", 90)?,
            poll_interval_secs: 2,
            shard_id: env_or("FIREFLY_SHARD_ID", DEFAULT_SHARD_ID.to_string())?,
        })
    }

    /// A configuration for the deploy node at `grpc` and `http`, with no observer.
    pub fn new(grpc: Endpoint, http: Endpoint, signing_key: String) -> Self {
        Self {
            grpc,
            http,
            observer: None,
            signing_key,
            finalization_timeout_secs: 90,
            poll_interval_secs: 2,
            shard_id: DEFAULT_SHARD_ID.to_string(),
        }
    }

    /// Also poll the node whose HTTP API is `http` for a deploy's verdict.
    pub fn with_observer(mut self, http: Endpoint) -> Self {
        self.observer = Some(http);
        self
    }
}

type StatusAnswer = Result<Option<DeployFinalizationStatus>>;

/// Which of the two polled nodes gave an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusSource {
    DeployNode,
    Observer,
}

/// What one poll of the deploy node and the observer established.
#[derive(Debug)]
enum PollOutcome {
    Terminal {
        status: DeployFinalizationStatus,
        source: StatusSource,
    },
    /// `included_by`: a node whose answer names a block carrying the deploy.
    Pending { included_by: Option<StatusSource> },
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
/// the observer answered. `observer` is `None` when no observer is configured.
fn poll_outcome(
    deploy_id: &str,
    node: StatusAnswer,
    observer: Option<StatusAnswer>,
) -> Result<PollOutcome> {
    let node_unserved = matches!(node, Ok(None));
    let node = node.ok().flatten();
    let observer = observer.and_then(|answer| answer.ok().flatten());

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
        (Some(status), _) => {
            return Ok(PollOutcome::Terminal {
                status,
                source: StatusSource::DeployNode,
            })
        }
        (None, Some(status)) => {
            return Ok(PollOutcome::Terminal {
                status,
                source: StatusSource::Observer,
            })
        }
        (None, None) => {}
    }

    if node_unserved && observer.is_none() {
        return Ok(PollOutcome::Unserved);
    }
    let names_block = |status: &Option<DeployFinalizationStatus>| {
        status
            .as_ref()
            .is_some_and(|status| status.latest_block_hash.is_some())
    };
    let included_by = if names_block(&node) {
        Some(StatusSource::DeployNode)
    } else if names_block(&observer) {
        Some(StatusSource::Observer)
    } else {
        None
    };
    Ok(PollOutcome::Pending { included_by })
}

/// Either node holding the deploy settles it as landed, and either node's
/// "not found" settles it as not landed. Only when no node answered is it
/// unknown, which is the deploy node's lookup error. `observer` is `None` when
/// no observer is configured.
fn landed(
    node: Result<Option<DeployDetail>>,
    observer: Option<Result<Option<DeployDetail>>>,
) -> Result<bool> {
    match (node, observer) {
        (Ok(Some(_)), _) | (_, Some(Ok(Some(_)))) => Ok(true),
        (Ok(None), _) | (_, Some(Ok(None))) => Ok(false),
        (Err(error), _) => Err(error),
    }
}

/// Manages F1r3fly node connections with connection reuse
#[derive(Clone)]
pub struct F1r3flyConnectionManager {
    config: ConnectionConfig,
    api: F1r3flyApi,
    node: NodeHttp,
    observer: Option<NodeHttp>,
}

impl F1r3flyConnectionManager {
    /// Create a new connection manager from environment variables
    pub fn from_env() -> Result<Self> {
        Self::new(ConnectionConfig::from_env()?)
    }

    /// Create a new connection manager with explicit configuration
    pub fn new(config: ConnectionConfig) -> Result<Self> {
        let api = F1r3flyApi::new(config.grpc.clone())?
            .with_signing_key(&config.signing_key)?
            .with_shard_id(&config.shard_id);
        let node = NodeHttp::new(config.http.clone())?;
        let observer = config.observer.clone().map(NodeHttp::new).transpose()?;
        Ok(Self {
            config,
            api,
            node,
            observer,
        })
    }

    /// Get the connection configuration
    pub fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    /// The HTTP API of the node that gave an answer.
    fn http_of(&self, source: StatusSource) -> &NodeHttp {
        match source {
            StatusSource::DeployNode => &self.node,
            StatusSource::Observer => self
                .observer
                .as_ref()
                .expect("only a configured observer is polled, so only it can be a source"),
        }
    }

    /// The deploy's detail as `source` serves it over HTTP.
    async fn deploy_detail(
        &self,
        source: StatusSource,
        deploy_id: &str,
    ) -> Result<Option<DeployDetail>> {
        self.http_of(source).get_deploy_detail(deploy_id).await
    }

    /// Execute an exploratory deploy (read-only query)
    pub async fn query(&self, rholang_code: &str) -> Result<String> {
        let (result, _block_info, _cost) = self
            .api
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
        self.node
            .estimate_cost(rholang_code, deployer_pubkey_hex, None)
            .await
    }

    /// Deploy Rholang code without waiting for it to finalize. Returns its ID.
    pub async fn deploy(&self, rholang_code: &str, options: &DeployOptions) -> Result<String> {
        self.api.deploy(rholang_code, options).await
    }

    /// Wait for a deploy to reach a terminal finalization state (`Finalized` /
    /// `Failed` / `Expired`) via `/api/deploy-finalization-status/{sig}`.
    ///
    /// Each poll asks the deploy node and, when one is configured, the
    /// observer; the first terminal state either reports is the verdict. Fails
    /// immediately when the deploy node does not serve the endpoint and no
    /// observer answered, and when the budget elapses without a terminal state.
    pub async fn wait_for_deploy_finalization(
        &self,
        deploy_sig_hex: &str,
        total_timeout_secs: u64,
        poll_interval_secs: u64,
    ) -> Result<DeployFinalizationStatus> {
        self.await_verdict(deploy_sig_hex, total_timeout_secs, poll_interval_secs)
            .await
            .map(|(status, _)| status)
    }

    /// [`Self::wait_for_deploy_finalization`], also naming the node whose
    /// answer was the verdict.
    async fn await_verdict(
        &self,
        deploy_sig_hex: &str,
        total_timeout_secs: u64,
        poll_interval_secs: u64,
    ) -> Result<(DeployFinalizationStatus, StatusSource)> {
        let max_attempts = (total_timeout_secs / poll_interval_secs.max(1)).max(1) as u32;
        let mut execution_failure: Option<String> = None;
        let mut node_error = None;
        let mut observer_error = None;

        for attempt in 1..=max_attempts {
            let (node, observer) = tokio::join!(
                self.node.deploy_finalization_status(deploy_sig_hex),
                self.observer_finalization_status(deploy_sig_hex),
            );
            report_unavailable(
                "deploy node",
                &node,
                &mut node_error,
                deploy_sig_hex,
                attempt,
            );
            if let Some(observer) = &observer {
                report_unavailable(
                    "observer",
                    observer,
                    &mut observer_error,
                    deploy_sig_hex,
                    attempt,
                );
            }

            let included_by = match poll_outcome(deploy_sig_hex, node, observer)? {
                PollOutcome::Terminal { status, source } => {
                    tracing::debug!(
                        deploy_sig = deploy_sig_hex,
                        state = %status.state,
                        ?source,
                        attempt,
                        "Deploy reached terminal state"
                    );
                    return Ok((status, source));
                }
                PollOutcome::Unserved => {
                    return Err(self.node.finalization_status_unserved().await);
                }
                PollOutcome::Pending { included_by } => included_by,
            };

            if let (Some(source), None) = (included_by, &execution_failure) {
                execution_failure = self.failed_execution(source, deploy_sig_hex).await;
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

    /// The observer's answer, or `None` when no observer is configured.
    async fn observer_finalization_status(&self, deploy_sig_hex: &str) -> Option<StatusAnswer> {
        match &self.observer {
            Some(observer) => Some(observer.deploy_finalization_status(deploy_sig_hex).await),
            None => None,
        }
    }

    /// `source`'s description of a failed execution, as `block N (cost C): reason`.
    async fn failed_execution(&self, source: StatusSource, deploy_sig_hex: &str) -> Option<String> {
        match self.deploy_detail(source, deploy_sig_hex).await {
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
        options: &DeployOptions,
    ) -> Result<DeployResult> {
        let deploy_id = self.api.deploy(rholang_code, options).await?;
        tracing::info!(deploy_id = %deploy_id, "Deploy submitted");
        self.await_result(deploy_id).await
    }

    /// Send a deploy signed earlier, wait for canonical finalization, and read
    /// its result. Safe to repeat for a deploy whose fate is unknown, also
    /// after its validity window has closed: when the node refuses it, a deploy
    /// that the deploy node or the observer holds in a block is waited on
    /// rather than reported refused.
    pub async fn submit_and_wait(&self, deploy: &SignedDeploy) -> Result<DeployResult> {
        let deploy_id = match self.api.submit(deploy).await {
            Ok(deploy_id) => deploy_id,
            Err(refusal @ Error::NodeRejected { .. }) => {
                let deploy_id = deploy.id();
                if !self.landed(&deploy_id).await? {
                    return Err(refusal);
                }
                tracing::info!(deploy_id = %deploy_id, %refusal,
                    "The node refused a deploy that is already in a block");
                deploy_id
            }
            Err(error) => return Err(error),
        };
        tracing::info!(deploy_id = %deploy_id, "Deploy submitted");
        self.await_result(deploy_id).await
    }

    /// Whether the deploy node or the observer holds the deploy in a block.
    async fn landed(&self, deploy_id: &str) -> Result<bool> {
        let observer = async {
            match &self.observer {
                Some(observer) => Some(observer.get_deploy_detail(deploy_id).await),
                None => None,
            }
        };
        let (node, observer) = tokio::join!(self.node.get_deploy_detail(deploy_id), observer);
        landed(node, observer)
    }

    async fn await_result(&self, deploy_id: String) -> Result<DeployResult> {
        let (status, source) = self
            .await_verdict(
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
                    execution: self.failed_execution(source, &deploy_id).await,
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

        // Both reads come after finalization, so a failed read must not look
        // retryable: the deploy is already final. A deploy that writes nothing
        // to deployId reads as an empty payload, so every data error is a
        // failed read.
        let unreadable = |cause: Error| {
            tracing::warn!(deploy_id = %deploy_id, %block_hash, %cause, "Failed to read the deploy's result");
            Error::DeployResultUnreadable {
                deploy_id: deploy_id.clone(),
                block_hash: block_hash.clone(),
                cause: Box::new(cause),
            }
        };
        let data = self
            .api
            .get_data_at_deploy_id(&deploy_id, &block_hash)
            .await
            .map_err(unreadable);
        let detail = match self.deploy_detail(source, &deploy_id).await {
            Ok(Some(detail)) => Ok(detail),
            Ok(None) => Err(unreadable(Error::decode(
                "deploy detail",
                "the node that reported the deploy finalized does not know it",
            ))),
            Err(cause) => Err(unreadable(cause)),
        };

        Ok(DeployResult {
            deploy_id,
            block_hash,
            detail,
            data,
        })
    }

    /// The deploy node's gRPC client, signing with this connection's key.
    pub fn api(&self) -> &F1r3flyApi {
        &self.api
    }

    /// The deploy node's HTTP client.
    pub fn http(&self) -> &NodeHttp {
        &self.node
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

        let options = DeployOptions::default().with_phlo_limit(crate::vault::TRANSFER_PHLO_LIMIT);
        let result = self.deploy_and_wait(&rholang, &options).await?;
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
    fn a_removed_variable_is_refused_with_its_replacement() {
        assert!(refuse_removed_variables(|_| false).is_ok());
        let error = refuse_removed_variables(|var| var == "FIREFLY_HTTP_PORT").unwrap_err();
        assert!(
            matches!(error, Error::InvalidInput { what: "FIREFLY_HTTP_PORT", ref detail }
                if detail.contains("FIREFLY_HTTP_URL")),
            "{error}"
        );
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

    fn endpoint(url: &str) -> Endpoint {
        Endpoint::parse(url).unwrap()
    }

    #[test]
    fn public_futures_are_send() {
        let manager = F1r3flyConnectionManager::new(ConnectionConfig::new(
            endpoint(DEFAULT_GRPC_URL),
            endpoint(DEFAULT_HTTP_URL),
            "01".repeat(32),
        ))
        .unwrap();
        assert_send(&manager.transfer("1111", 1u8.into()));
        assert_send(&manager.deploy_and_wait("Nil", &DeployOptions::default()));
        assert_send(&manager.wait_for_deploy_finalization("sig", 1, 1));
        assert_send(&manager.query("Nil"));
        let api = manager.api();
        assert_send(&api.exploratory_deploy("Nil", None, false));
        assert_send(&api.propose());
        assert_send(&api.show_main_chain(1));
        assert_send(&manager.http().explore_deploy("Nil"));
    }

    #[test]
    fn a_new_config_has_no_observer() {
        let config = ConnectionConfig::new(
            endpoint("https://node.example:40401"),
            endpoint("https://node.example:40403"),
            "my_key".to_string(),
        );
        assert_eq!(config.observer, None);
        let observed = config.with_observer(endpoint("http://observer:40453"));
        assert_eq!(observed.observer, Some(endpoint("http://observer:40453")));
    }

    #[test]
    fn a_manager_refuses_a_malformed_signing_key() {
        let config = ConnectionConfig::new(
            endpoint(DEFAULT_GRPC_URL),
            endpoint(DEFAULT_HTTP_URL),
            "not hex".to_string(),
        );
        assert!(F1r3flyConnectionManager::new(config).is_err());
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
        super::poll_outcome("sig", node, Some(observer))
    }

    #[test]
    fn without_an_observer_the_deploy_node_alone_decides() {
        assert!(matches!(
            super::poll_outcome("sig", status("Finalized", Some("b1")), None),
            Ok(PollOutcome::Terminal {
                source: StatusSource::DeployNode,
                ..
            })
        ));
        assert!(matches!(
            super::poll_outcome("sig", Ok(None), None),
            Ok(PollOutcome::Unserved)
        ));
        assert!(matches!(
            super::poll_outcome("sig", unreachable(), None),
            Ok(PollOutcome::Pending { included_by: None })
        ));
        let lookup_failed = || unreachable().map(|_| None);
        assert!(!super::landed(Ok(None), None).unwrap());
        assert!(super::landed(Ok(Some(DeployDetail::finalized("sig", false))), None).unwrap());
        assert!(matches!(
            super::landed(lookup_failed(), None),
            Err(Error::Transport { .. })
        ));
    }

    #[test]
    fn a_terminal_state_from_the_deploy_node_ends_the_wait_while_the_observer_is_pending() {
        let outcome = poll_outcome(status("Failed", Some("b1")), status("Pending", Some("b1")));
        assert!(
            matches!(outcome, Ok(PollOutcome::Terminal { ref status, source: StatusSource::DeployNode })
                if status.state == "Failed"),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_terminal_state_from_the_observer_ends_the_wait_while_the_node_is_unreachable() {
        let outcome = poll_outcome(unreachable(), status("Finalized", Some("b1")));
        assert!(
            matches!(outcome, Ok(PollOutcome::Terminal { ref status, source: StatusSource::Observer })
                if status.state == "Finalized"),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_verdict_from_the_observer_is_read_back_from_the_observer() {
        let outcome = poll_outcome(
            status("Pending", Some("b1")),
            status("Finalized", Some("b1")),
        );
        assert!(
            matches!(
                outcome,
                Ok(PollOutcome::Terminal {
                    source: StatusSource::Observer,
                    ..
                })
            ),
            "{outcome:?}"
        );
    }

    #[test]
    fn agreeing_verdicts_are_read_back_from_the_deploy_node() {
        let outcome = poll_outcome(
            status("Finalized", Some("b1")),
            status("Finalized", Some("b1")),
        );
        assert!(
            matches!(
                outcome,
                Ok(PollOutcome::Terminal {
                    source: StatusSource::DeployNode,
                    ..
                })
            ),
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
            Ok(PollOutcome::Pending { included_by: None })
        ));
    }

    #[test]
    fn a_refused_deploy_has_landed_when_either_node_holds_it() {
        let held = || Ok(Some(DeployDetail::finalized("sig", false)));
        let lookup_failed = || unreachable().map(|_| None);
        assert!(super::landed(held(), Some(lookup_failed())).unwrap());
        assert!(super::landed(lookup_failed(), Some(held())).unwrap());
        assert!(super::landed(Ok(None), Some(held())).unwrap());
    }

    #[test]
    fn a_refused_deploy_has_not_landed_when_a_node_does_not_know_it() {
        let lookup_failed = || unreachable().map(|_| None);
        assert!(!super::landed(Ok(None), Some(lookup_failed())).unwrap());
        assert!(!super::landed(lookup_failed(), Some(Ok(None))).unwrap());
    }

    #[test]
    fn whether_a_refused_deploy_landed_is_unknown_when_neither_node_answers() {
        let lookup_failed = || unreachable().map(|_| None);
        assert!(matches!(
            super::landed(lookup_failed(), Some(lookup_failed())),
            Err(Error::Transport { .. })
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
    fn a_pending_answer_naming_a_block_names_the_node_that_saw_it() {
        assert!(matches!(
            poll_outcome(status("Pending", None), status("Pending", Some("b1"))),
            Ok(PollOutcome::Pending {
                included_by: Some(StatusSource::Observer)
            })
        ));
        assert!(matches!(
            poll_outcome(status("Pending", Some("b1")), unreachable()),
            Ok(PollOutcome::Pending {
                included_by: Some(StatusSource::DeployNode)
            })
        ));
        assert!(matches!(
            poll_outcome(unreachable(), unreachable()),
            Ok(PollOutcome::Pending { included_by: None })
        ));
    }
}
