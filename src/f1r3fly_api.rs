//! F1r3fly API client types and re-exports
//!
//! The gRPC client is split across `grpc/` submodules:
//! - `grpc::deploy` prepare, submit, deploy, propose
//! - `grpc::query` exploratory_deploy, get_data_at_deploy_id
//! - `grpc::blocks` show_main_chain, get_blocks_by_height, is_finalized, tip sampling
//!
//! The HTTP client is [`crate::node_http::NodeHttp`]. Values in results decode
//! to [`RhoValue`].

use f1r3fly_models::rhoapi::Par;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::rho_value::RhoValue;

pub use crate::grpc::F1r3flyApi;

/// A block a read ran against.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BlockRef {
    #[serde(rename = "blockHash")]
    pub hash: String,
    #[serde(rename = "blockNumber")]
    pub number: i64,
}

/// What an exploratory deploy returned over gRPC.
#[derive(Debug, Clone)]
pub struct ExploratoryResult {
    /// The data the term sent to the first name it declares, as in
    /// `new return in { return!(...) }`.
    pub data: Vec<Par>,
    pub block: BlockRef,
    pub cost: u64,
}

impl ExploratoryResult {
    pub fn values(&self) -> Result<Vec<RhoValue>> {
        self.data.iter().map(RhoValue::try_from).collect()
    }

    /// The one value the term returned.
    pub fn value(&self) -> Result<RhoValue> {
        match self.data.as_slice() {
            [par] => RhoValue::try_from(par),
            data => Err(Error::decode(
                "exploratory deploy result",
                format!("expected one value, got {}", data.len()),
            )),
        }
    }
}

/// What an exploratory deploy returned over HTTP, already decoded. The node's
/// HTTP encoding loses information; [`crate::rho_value`] lists what.
#[derive(Debug, Clone)]
pub struct ExploratoryValues {
    pub values: Vec<RhoValue>,
    pub block: BlockRef,
    pub cost: u64,
}

impl ExploratoryValues {
    /// Decode a `/api/explore-deploy` response.
    pub(crate) fn from_node_json(json: serde_json::Value) -> Result<Self> {
        #[derive(Deserialize)]
        struct Response {
            expr: Vec<serde_json::Value>,
            block: BlockRef,
            cost: u64,
        }
        let response: Response = serde_json::from_value(json)
            .map_err(|e| Error::decode("exploratory deploy response", e))?;
        Ok(ExploratoryValues {
            values: response
                .expr
                .iter()
                .map(RhoValue::from_node_json)
                .collect::<Result<_>>()?,
            block: response.block,
            cost: response.cost,
        })
    }

    /// The one value the term returned.
    pub fn value(&self) -> Result<&RhoValue> {
        match self.values.as_slice() {
            [value] => Ok(value),
            values => Err(Error::decode(
                "exploratory deploy result",
                format!("expected one value, got {}", values.len()),
            )),
        }
    }
}

/// A block as the node's block listings describe it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BlockSummary {
    #[serde(rename = "blockHash")]
    pub hash: String,
    #[serde(rename = "blockNumber")]
    pub number: i64,
    /// The author's public key, as hex.
    pub sender: String,
    /// Milliseconds since the Unix epoch, as the author stamped it.
    pub timestamp: i64,
    #[serde(rename = "isFinalized")]
    pub finalized: bool,
}

/// A vault's native-token balance at one block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Balance {
    pub address: String,
    pub dust: num_bigint::BigUint,
    pub block: BlockRef,
}

/// Node status from `/api/status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub version: serde_json::Value,
    pub address: String,
    #[serde(rename = "networkId")]
    pub network_id: String,
    #[serde(rename = "shardId")]
    pub shard_id: String,
    pub peers: i32,
    pub nodes: i32,
    #[serde(rename = "minPhloPrice")]
    pub min_phlo_price: i64,
    #[serde(rename = "nativeTokenName", default)]
    pub native_token_name: String,
    #[serde(rename = "nativeTokenSymbol", default)]
    pub native_token_symbol: String,
    #[serde(rename = "nativeTokenDecimals", default)]
    pub native_token_decimals: Option<u32>,
    #[serde(rename = "peerList", default)]
    pub peer_list: Vec<serde_json::Value>,
    // Numeric and bool fields use Option so a missing field is distinguishable
    // from a real zero/false value.
    #[serde(rename = "lastFinalizedBlockNumber", default)]
    pub last_finalized_block_number: Option<i64>,
    #[serde(rename = "isValidator", default)]
    pub is_validator: Option<bool>,
    #[serde(rename = "isReadOnly", default)]
    pub is_read_only: Option<bool>,
    #[serde(rename = "isReady", default)]
    pub is_ready: Option<bool>,
    #[serde(rename = "currentEpoch", default)]
    pub current_epoch: Option<i64>,
    #[serde(rename = "epochLength", default)]
    pub epoch_length: Option<i32>,
}

/// Deploy execution detail from the node's `/api/deploy/{id}` endpoint.
/// Unified DeployResponse — full view includes all fields,
/// summary view omits Optional fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployDetail {
    #[serde(rename = "deployId")]
    pub deploy_id: String,
    #[serde(rename = "blockHash")]
    pub block_hash: String,
    #[serde(rename = "blockNumber")]
    pub block_number: i64,
    pub timestamp: i64,
    pub cost: u64,
    pub errored: bool,
    #[serde(rename = "isFinalized")]
    pub is_finalized: bool,
    #[serde(default)]
    pub deployer: Option<String>,
    #[serde(default)]
    pub term: Option<String>,
    #[serde(rename = "systemDeployError", default)]
    pub system_deploy_error: Option<String>,
    #[serde(rename = "phloPrice", default)]
    pub phlo_price: Option<i64>,
    #[serde(rename = "phloLimit", default)]
    pub phlo_limit: Option<i64>,
    #[serde(rename = "sigAlgorithm", default)]
    pub sig_algorithm: Option<String>,
    #[serde(rename = "validAfterBlockNumber", default)]
    pub valid_after_block_number: Option<i64>,
}

impl DeployDetail {
    /// Why the deploy's execution failed, or `None` when it succeeded.
    pub fn execution_failure(&self) -> Option<String> {
        execution_failure(self.errored, self.system_deploy_error.as_deref())
    }

    /// A finalized deploy's detail with only its outcome set.
    #[cfg(test)]
    pub(crate) fn finalized(deploy_id: &str, errored: bool) -> Self {
        DeployDetail {
            deploy_id: deploy_id.to_string(),
            block_hash: "abcd".to_string(),
            block_number: 12,
            timestamp: 0,
            cost: 252_435,
            errored,
            is_finalized: true,
            deployer: None,
            term: None,
            system_deploy_error: None,
            phlo_price: None,
            phlo_limit: None,
            sig_algorithm: None,
            valid_after_block_number: None,
        }
    }
}

/// A system deploy (payment, refund) can fail without the user deploy being
/// flagged `errored`, so either signal is a failed execution.
fn execution_failure(errored: bool, system_deploy_error: Option<&str>) -> Option<String> {
    match system_deploy_error.filter(|error| !error.is_empty()) {
        Some(error) => Some(error.to_string()),
        None if errored => Some("execution errored".to_string()),
        None => None,
    }
}

/// Canonical-state finalization status for a deploy, from
/// `/api/deploy-finalization-status/{sig}`.
///
/// `state` is one of:
/// - `"Finalized"` — clean inclusion in canonical-finalized block. Terminal.
/// - `"Failed"` — Rholang execution itself failed. Terminal.
/// - `"Pending"` — not yet canonical-finalized, not expired. Keep polling.
/// - `"Expired"` — `valid_after_block_number + deploy_lifespan` elapsed
///   (LFB-anchored) without a clean canonical inclusion. Terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployFinalizationStatus {
    pub state: String,
    pub rejection_count: u32,
    pub latest_block_hash: Option<String>,
}

impl DeployFinalizationStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self.state.as_str(), "Finalized" | "Failed" | "Expired")
    }
}

/// Result of a full deploy-and-wait operation: a finalized deploy.
///
/// Both reads happen after finalization, so a failed read is a
/// [`crate::Error::DeployResultUnreadable`]: the deploy itself is final.
#[derive(Debug)]
pub struct DeployResult {
    pub deploy_id: String,
    pub block_hash: String,
    /// The deploy's execution as the deploy node recorded it: block, cost and
    /// whether it failed.
    pub detail: crate::error::Result<DeployDetail>,
    /// Data on the deploy's `deployId` channel. `Ok` with no elements when the
    /// deploy wrote nothing.
    pub data: crate::error::Result<Vec<f1r3fly_models::rhoapi::Par>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposeResult {
    Proposed(String),
    Skipped(String),
}

#[cfg(test)]
mod tests {
    use super::execution_failure;

    #[test]
    fn a_system_deploy_error_is_a_failure_even_when_not_flagged_errored() {
        assert_eq!(
            execution_failure(false, Some("Deploy payment failed: Insufficient funds")),
            Some("Deploy payment failed: Insufficient funds".to_string())
        );
    }

    #[test]
    fn an_errored_flag_alone_is_a_failure() {
        assert_eq!(
            execution_failure(true, None),
            Some("execution errored".to_string())
        );
        assert_eq!(
            execution_failure(true, Some("")),
            Some("execution errored".to_string())
        );
    }

    #[test]
    fn no_flag_and_no_error_is_success() {
        assert_eq!(execution_failure(false, None), None);
        assert_eq!(execution_failure(false, Some("")), None);
    }
}
