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

pub(crate) fn default_resource_policy_hex() -> &'static str {
    static POLICY: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let policy =
            f1r3fly_casper::rust::util::rholang::costacc::owner_offer::default_resource_policy(
                "root",
            )
            .expect("the standard resource policy is valid");
        hex::encode(
            policy
                .encode()
                .expect("the standard resource policy encodes"),
        )
    });
    &POLICY
}

/// Client for interacting with the F1r3fly node via gRPC and HTTP
pub struct F1r3flyApi<'a> {
    pub(crate) signing_key: Option<SecretKey>,
    pub(crate) node_host: &'a str,
    pub(crate) grpc_port: u16,
    pub(crate) tip_floor: Arc<AtomicI64>,
    pub(crate) exploratory_retry_budget: Option<Duration>,
    /// Hexadecimal genesis resource policy whose schedule each offer signs
    pub(crate) resource_policy: &'a str,
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
            resource_policy: default_resource_policy_hex(),
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
            resource_policy: default_resource_policy_hex(),
        }
    }

    /// Cap how long exploratory deploys may spend retrying capacity
    /// rejections. Without a budget the full [`crate::utils::http::EXPLORATORY_RETRY_BACKOFF`]
    /// schedule is used.
    pub fn with_exploratory_retry_budget(mut self, budget: Duration) -> Self {
        self.exploratory_retry_budget = Some(budget);
        self
    }

    /// Sign offers under this genesis resource policy: the hexadecimal
    /// `resource-policy` of the target shard's genesis configuration. Without
    /// it, offers sign the standard node resource policy.
    pub fn with_resource_policy(mut self, resource_policy_hex: &'a str) -> Self {
        self.resource_policy = resource_policy_hex;
        self
    }

    pub(crate) fn grpc_url(&self) -> String {
        format!("http://{}:{}/", self.node_host, self.grpc_port)
    }
}
