//! gRPC client for the f1r3fly node

mod blocks;
mod deploy;
mod http;
pub mod query;

use crate::error::{Error, Result};
use f1r3fly_models::casper::v1::deploy_service_client::DeployServiceClient;
use f1r3fly_models::casper::v1::propose_service_client::ProposeServiceClient;
use secp256k1::SecretKey;
use std::sync::atomic::AtomicI64;
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::Channel;

const TIP_FLOOR_UNSET: i64 = -1;

/// Client for interacting with the F1r3fly node via gRPC and HTTP
pub struct F1r3flyApi<'a> {
    pub(crate) signing_key: Option<SecretKey>,
    pub(crate) node_host: &'a str,
    pub(crate) grpc_port: u16,
    pub(crate) tip_floor: Arc<AtomicI64>,
    pub(crate) exploratory_retry_budget: Option<Duration>,
    pub(crate) shard_id: String,
}

impl<'a> F1r3flyApi<'a> {
    pub fn new(signing_key: &str, node_host: &'a str, grpc_port: u16) -> Result<Self> {
        Ok(F1r3flyApi {
            signing_key: Some(crate::utils::CryptoUtils::decode_private_key(signing_key)?),
            ..Self::new_readonly(node_host, grpc_port)
        })
    }

    /// Construct a client for read-only operations (exploratory deploys and
    /// HTTP reads). These paths never sign, so no key is required.
    pub fn new_readonly(node_host: &'a str, grpc_port: u16) -> Self {
        F1r3flyApi {
            signing_key: None,
            node_host,
            grpc_port,
            tip_floor: Arc::new(AtomicI64::new(TIP_FLOOR_UNSET)),
            exploratory_retry_budget: None,
            shard_id: crate::deploy::DEFAULT_SHARD_ID.to_string(),
        }
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

    pub(crate) fn grpc_url(&self) -> String {
        format!("http://{}:{}/", self.node_host, self.grpc_port)
    }

    pub(crate) async fn deploy_client(&self) -> Result<DeployServiceClient<Channel>> {
        let url = self.grpc_url();
        DeployServiceClient::connect(url.clone())
            .await
            .map_err(|e| Error::transport(url, e))
    }

    pub(crate) async fn propose_client(&self) -> Result<ProposeServiceClient<Channel>> {
        let url = self.grpc_url();
        ProposeServiceClient::connect(url.clone())
            .await
            .map_err(|e| Error::transport(url, e))
    }

    pub(crate) fn grpc_error(&self, status: tonic::Status) -> Error {
        Error::grpc(self.grpc_url(), status)
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
