//! gRPC client for the f1r3fly node

mod blocks;
mod deploy;
mod http;
pub mod query;

use secp256k1::SecretKey;
use std::sync::atomic::AtomicI64;
use std::sync::Arc;
use std::time::Duration;

const TIP_FLOOR_UNSET: i64 = -1;

/// Client for interacting with the F1r3fly node via gRPC and HTTP
pub struct F1r3flyApi<'a> {
    pub(crate) signing_key: Option<SecretKey>,
    pub(crate) node_host: &'a str,
    pub(crate) grpc_port: u16,
    pub(crate) tip_floor: Arc<AtomicI64>,
    pub(crate) exploratory_retry_budget: Option<Duration>,
}

impl<'a> F1r3flyApi<'a> {
    pub fn new(
        signing_key: &str,
        node_host: &'a str,
        grpc_port: u16,
    ) -> std::result::Result<Self, crate::error::NodeCliError> {
        let key_bytes = hex::decode(signing_key)?;
        let key_array: [u8; 32] = key_bytes.try_into().map_err(|_| {
            crate::error::NodeCliError::crypto_invalid_private_key("key must be 32 bytes")
        })?;
        let secret_key = SecretKey::from_byte_array(key_array)?;
        Ok(F1r3flyApi {
            signing_key: Some(secret_key),
            node_host,
            grpc_port,
            tip_floor: Arc::new(AtomicI64::new(TIP_FLOOR_UNSET)),
            exploratory_retry_budget: None,
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
        }
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
}
