//! gRPC client for the f1r3fly node

mod blocks;
mod deploy;
pub mod query;

use crate::endpoint::Endpoint;
use crate::error::{Error, Result};
use f1r3fly_models::casper::v1::deploy_service_client::DeployServiceClient;
use f1r3fly_models::casper::v1::propose_service_client::ProposeServiceClient;
use secp256k1::SecretKey;
use std::sync::atomic::AtomicI64;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;
use tonic::transport::Channel;

const TIP_FLOOR_UNSET: i64 = -1;

/// Client for a node's gRPC API. Cloning shares its connection.
#[derive(Clone)]
pub struct F1r3flyApi {
    pub(crate) endpoint: Endpoint,
    channel: Arc<OnceCell<Channel>>,
    pub(crate) signing_key: Option<SecretKey>,
    pub(crate) tip_floor: Arc<AtomicI64>,
    pub(crate) exploratory_retry_budget: Option<Duration>,
    pub(crate) shard_id: String,
}

impl F1r3flyApi {
    /// A client for `endpoint` that reads and sends signed deploys. It signs
    /// nothing until [`with_signing_key`](Self::with_signing_key) gives it a key.
    pub fn new(endpoint: Endpoint) -> Self {
        F1r3flyApi {
            endpoint,
            channel: Arc::default(),
            signing_key: None,
            tip_floor: Arc::new(AtomicI64::new(TIP_FLOOR_UNSET)),
            exploratory_retry_budget: None,
            shard_id: crate::deploy::DEFAULT_SHARD_ID.to_string(),
        }
    }

    /// Sign deploys with `signing_key`, a hex secp256k1 private key.
    pub fn with_signing_key(mut self, signing_key: &str) -> Result<Self> {
        self.signing_key = Some(crate::utils::CryptoUtils::decode_private_key(signing_key)?);
        Ok(self)
    }

    /// Sign deploys for this shard: the node's `casper.shard-name`. Without
    /// it, deploys sign [`crate::deploy::DEFAULT_SHARD_ID`].
    pub fn with_shard_id(mut self, shard_id: impl Into<String>) -> Self {
        self.shard_id = shard_id.into();
        self
    }

    /// Cap how long exploratory deploys may spend retrying capacity
    /// rejections. Without a budget the full [`crate::utils::http::EXPLORATORY_RETRY_BACKOFF`]
    /// schedule is used.
    pub fn with_exploratory_retry_budget(mut self, budget: Duration) -> Self {
        self.exploratory_retry_budget = Some(budget);
        self
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// The channel to the node, connected on first use. A failed connection
    /// is not kept, so the next call tries again.
    async fn channel(&self) -> Result<Channel> {
        self.channel
            .get_or_try_init(|| async {
                let url = self.endpoint.to_string();
                let mut endpoint = tonic::transport::Endpoint::from_shared(url.clone())
                    .map_err(|e| Error::transport(&url, e))?;
                if let Some(tls) = self.endpoint.grpc_tls() {
                    endpoint = endpoint
                        .tls_config(tls)
                        .map_err(|e| Error::transport(&url, e))?;
                }
                endpoint
                    .connect()
                    .await
                    .map_err(|e| Error::transport(url, e))
            })
            .await
            .cloned()
    }

    pub(crate) async fn deploy_client(&self) -> Result<DeployServiceClient<Channel>> {
        Ok(DeployServiceClient::new(self.channel().await?))
    }

    pub(crate) async fn propose_client(&self) -> Result<ProposeServiceClient<Channel>> {
        Ok(ProposeServiceClient::new(self.channel().await?))
    }

    pub(crate) fn grpc_error(&self, status: tonic::Status) -> Error {
        Error::grpc(self.endpoint.to_string(), status)
    }
}

/// The node's refusal, as the `messages` of its `ServiceError`.
pub(crate) fn rejected(
    operation: &'static str,
    error: &f1r3fly_models::servicemodelapi::ServiceError,
) -> Error {
    Error::NodeRejected {
        operation,
        message: error.messages.join("; "),
    }
}
