use crate::f1r3fly_api::DeployStatus;
use serde::{Deserialize, Serialize};
use serde_json;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FinalizeStatus {
    Finalizing,
    Finalized,
    FinalizationError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressedDeployStatus {
    Deploying,
    Finalizing,
    Finalized,
    DeployError,
    FinalizationError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployCompressedInfo {
    status: CompressedDeployStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    msg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    block_hash: Option<String>,
}

impl DeployCompressedInfo {
    pub fn ok(status: CompressedDeployStatus, block_hash: Option<String>) -> Self {
        debug_assert!(!matches!(
            status,
            CompressedDeployStatus::DeployError | CompressedDeployStatus::FinalizationError
        ));
        Self {
            status,
            msg: None,
            block_hash,
        }
    }

    pub fn error(
        status: CompressedDeployStatus,
        err: impl Into<String>,
        block_hash: Option<String>,
    ) -> Self {
        debug_assert!(matches!(
            status,
            CompressedDeployStatus::DeployError | CompressedDeployStatus::FinalizationError
        ));
        Self {
            status,
            msg: Some(err.into()),
            block_hash,
        }
    }

    pub fn from_deploy(s: DeployStatus, block_hash: Option<String>) -> Self {
        match s {
            DeployStatus::Deploying => Self::ok(CompressedDeployStatus::Deploying, None),
            DeployStatus::Included => Self::ok(CompressedDeployStatus::Finalizing, block_hash),
            DeployStatus::DeployError(e) => {
                Self::error(CompressedDeployStatus::DeployError, e, None)
            }
        }
    }

    pub fn from_finalize(s: FinalizeStatus, block_hash: Option<String>) -> Self {
        match s {
            FinalizeStatus::Finalizing => Self::ok(CompressedDeployStatus::Finalizing, block_hash),
            FinalizeStatus::Finalized => Self::ok(CompressedDeployStatus::Finalized, block_hash),
            FinalizeStatus::FinalizationError(e) => {
                Self::error(CompressedDeployStatus::FinalizationError, e, block_hash)
            }
        }
    }

    pub fn status(&self) -> &CompressedDeployStatus {
        &self.status
    }
    pub fn msg(&self) -> Option<&str> {
        self.msg.as_deref()
    }
    pub fn block_hash(&self) -> Option<&str> {
        self.block_hash.as_deref()
    }
}

// Emoji constants
pub const EMOJI_SEARCH: &str = "🔍";
pub const EMOJI_SUCCESS: &str = "✅";
pub const EMOJI_ERROR: &str = "❌";
pub const EMOJI_INFO: &str = "📊";
pub const EMOJI_TIME: &str = "⏱️";
pub const EMOJI_ROCKET: &str = "🚀";
pub const EMOJI_FILE: &str = "📄";
pub const EMOJI_BLOCK: &str = "🧱";
pub const EMOJI_CONNECT: &str = "🔌";
pub const EMOJI_KEY: &str = "🔑";
pub const EMOJI_LINK: &str = "🔗";
pub const EMOJI_WARNING: &str = "⚠️";

// Output formatting functions
pub fn print_info(message: &str) {
    println!("{} {}", EMOJI_INFO, message);
}

pub fn print_success(message: &str) {
    println!("{} {}", EMOJI_SUCCESS, message);
}

pub fn print_error(message: &str) {
    println!("{} {}", EMOJI_ERROR, message);
}

pub fn print_search(message: &str) {
    println!("{} {}", EMOJI_SEARCH, message);
}

pub fn print_time(message: &str, duration: Duration) {
    println!("{} {}: {:.2?}", EMOJI_TIME, message, duration);
}

pub fn print_file_info(filename: &str, size: usize) {
    println!("{} Reading Rholang from: {}", EMOJI_FILE, filename);
    println!("{} Code size: {} bytes", EMOJI_INFO, size);
}

pub fn print_connection(host: &str, port: u16) {
    println!(
        "{} Connecting to F1r3fly node at {}:{}",
        EMOJI_CONNECT, host, port
    );
}

pub fn print_block_info(block_hash: &str) {
    println!("{} Block hash: {}", EMOJI_BLOCK, block_hash);
}

pub fn print_rocket(message: &str) {
    println!("{} {}", EMOJI_ROCKET, message);
}

pub fn print_key(key_type: &str, key_value: &str) {
    println!("{} {}: {}", EMOJI_KEY, key_type, key_value);
}

pub fn print_json_pretty(
    title: &str,
    json: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{} {}:", EMOJI_INFO, title);
    println!("{}", serde_json::to_string_pretty(json)?);
    Ok(())
}

pub fn print_warning(message: &str) {
    println!("{} {}", EMOJI_WARNING, message);
}

pub fn print_bond_status(is_bonded: bool) {
    if is_bonded {
        println!("{} {} Validator is BONDED", EMOJI_LINK, EMOJI_SUCCESS);
    } else {
        println!("{} {} Validator is NOT BONDED", EMOJI_LINK, EMOJI_ERROR);
    }
}

pub fn print_health_status(healthy: u32, total: u32) {
    println!("{} Healthy nodes: {}/{}", EMOJI_SUCCESS, healthy, total);
}

pub fn print_network_status(healthy: u32, total: u32) {
    if healthy == 0 {
        print_error("No healthy nodes found - check if network is running");
    } else if healthy == total {
        print_warning("All nodes healthy but some peer connections may be missing");
    } else {
        print_warning("Some nodes are unhealthy - check individual node logs");
    }
}

// Helper function to format operation results
pub fn format_operation_result(success: bool, operation: &str, duration: Duration) {
    if success {
        print_success(&format!("{} successful!", operation));
    } else {
        print_error(&format!("{} failed!", operation));
    }
    print_time("Time taken", duration);
}
