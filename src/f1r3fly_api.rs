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

/// Deploy execution detail from the node's `/api/deploy/{id}` endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployDetail {
    #[serde(rename = "blockHash")]
    pub block_hash: String,
    #[serde(rename = "blockNumber")]
    pub block_number: i64,
    pub timestamp: i64,
    pub deployer: String,
    pub term: String,
    pub cost: u64,
    pub errored: bool,
    #[serde(rename = "systemDeployError")]
    pub system_deploy_error: String,
    #[serde(rename = "phloPrice")]
    pub phlo_price: i64,
    #[serde(rename = "phloLimit")]
    pub phlo_limit: i64,
    pub sig: String,
    #[serde(rename = "sigAlgorithm")]
    pub sig_algorithm: String,
    #[serde(rename = "validAfterBlockNumber")]
    pub valid_after_block_number: i64,
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
