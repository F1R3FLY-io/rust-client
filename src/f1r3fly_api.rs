//! F1r3fly API client types and re-exports
//!
//! The implementation is split across `grpc/` submodules:
//! - `grpc::deploy` deploy, propose, full_deploy, build_deploy_msg
//! - `grpc::query` exploratory_deploy, get_data_at_deploy_id, find_deploy_grpc
//! - `grpc::blocks` show_main_chain, get_blocks_by_height, is_finalized, tip sampling
//! - `grpc::http` get_deploy_block_hash, get_deploy_detail

use serde::{Deserialize, Serialize};

// Re-export the client and helpers from the grpc module
pub use crate::grpc::query::extract_par_data;
pub use crate::grpc::F1r3flyApi;

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
    pub native_token_decimals: u32,
    #[serde(rename = "peerList", default)]
    pub peer_list: Vec<serde_json::Value>,
    // Numeric and bool fields use Option so a missing field (e.g. from an older
    // Scala node) is distinguishable from a real zero/false value.
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

/// Result of a full deploy-and-wait operation
#[derive(Debug, Clone)]
pub struct DeployResult {
    pub deploy_id: String,
    pub block_hash: String,
    pub block_number: Option<i64>,
    pub cost: Option<u64>,
    pub errored: bool,
    pub system_deploy_error: Option<String>,
    pub data: Vec<f1r3fly_models::rhoapi::Par>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposeResult {
    Proposed(String),
    Skipped(String),
}
