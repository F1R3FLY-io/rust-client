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

/// The genesis resource policy of the ASI shard, as the hexadecimal canonical
/// record bytes of its `casper.genesis-block-data.resource-policy`.
///
/// An owner-direct offer signs the schedule of the shard's genesis policy, and
/// the node refuses an offer whose schedule differs from the adopted policy.
/// This policy is the node's default resource schedule for shard `root`
/// (`owner_offer::default_resource_schedule`) with the ASI token: settlement
/// asset `ASI` and settlement unit `atomic-ASI` instead of `REV` and
/// `atomic-REV`. Every other field, 8 decimals included, is the default.
pub const ASI_RESOURCE_POLICY_HEX: &str = "0000000000000023663172336e6f64653a67656e657369732d7265736f757263652d706f6c6963793a763200000000000002db0000000000000019663172336e6f64653a70686c6f2d7363686564756c653a7631000000000000000800000000000000060000000000000004746573740000000000000004726f6f740000000000000003415349000000000000000a61746f6d69632d415349000000000000000108000000000000021c000000000000000400000004000000000000007b0000000000000007636f6d707574650000000000000004434f4d4d00000000000000204fe858a6554cf65acca907e41bbce0c6c1560ab65b78e1e52646a57d926fb3830000000000000020352ed97b8fa1be92c134dbcbeb944bd03b57c5fe5b7eba3aae55941507182e94000000000000000800000000000000010000000000000080000000000000000c696e74726f64756374696f6e0000000000000004627974650000000000000020bf524c59897ab7aa869ccf79c8a6f990959156084239619aa370d184e47e4a2f0000000000000020352ed97b8fa1be92c134dbcbeb944bd03b57c5fe5b7eba3aae55941507182e9400000000000000080000000000000001000000000000007c00000000000000087472616e736665720000000000000004627974650000000000000020c8384c199e6cde2c0cac6afa3f1a9bf8be50820bcb84686f107201c4cab60cd80000000000000020352ed97b8fa1be92c134dbcbeb944bd03b57c5fe5b7eba3aae55941507182e94000000000000000800000000000000010000000000000079000000000000000574726163650000000000000004627974650000000000000020908704256016b2861488c10ebb616755898ae031ac1ba4dad81ec91fd05d355d0000000000000020352ed97b8fa1be92c134dbcbeb944bd03b57c5fe5b7eba3aae55941507182e9400000000000000080000000000000001000000000000000800000000000000000000000000000008000000000000000100000000000000202595df679351956d36278953647cb3c4364ea869ae33975df380722339402d88000000000000000101";

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
            resource_policy: ASI_RESOURCE_POLICY_HEX,
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
            resource_policy: ASI_RESOURCE_POLICY_HEX,
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
    /// it, offers sign [`ASI_RESOURCE_POLICY_HEX`].
    pub fn with_resource_policy(mut self, resource_policy_hex: &'a str) -> Self {
        self.resource_policy = resource_policy_hex;
        self
    }

    pub(crate) fn grpc_url(&self) -> String {
        format!("http://{}:{}/", self.node_host, self.grpc_port)
    }
}
