//! Integration smoke tests against a running F1R3FLY node.
//!
//! Works against both standalone nodes and multi-validator shards.
//! Configure via environment variables:
//!   F1R3FLY_HOST          (default: localhost)
//!   F1R3FLY_HTTP_PORT     (default: 40413, validator1 HTTP)
//!   F1R3FLY_OBSERVER_HTTP (default: 40453, readonly HTTP)
//!
//! Standalone (CI):  F1R3FLY_HTTP_PORT=40463 F1R3FLY_OBSERVER_HTTP=40463
//! Shard (local):    defaults work (40413 validator, 40453 readonly)
//!
//! Run: cargo test --test smoke --release
//! Skip if no node: tests return Ok(()) when connection fails.

use reqwest::Client;
use serde_json::Value;

/// A structurally valid secp256k1 public key (the generator point G, i.e. the
/// public key for private key 1) that no test shard ever bonds. The node
/// validates key structure and rejects malformed keys with 400, so
/// unknown-validator tests must send a real curve point.
const UNBONDED_PUBKEY: &str = "0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8";

fn host() -> String {
    std::env::var("F1R3FLY_HOST").unwrap_or_else(|_| "localhost".into())
}
fn http_port() -> u16 {
    std::env::var("F1R3FLY_HTTP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40413)
}
fn observer_http() -> u16 {
    std::env::var("F1R3FLY_OBSERVER_HTTP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40453)
}
fn grpc_port() -> u16 {
    std::env::var("F1R3FLY_GRPC_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40412)
}

/// Funded genesis signing key — same default as scripts/smoke_test.sh.
const FUNDED_SIGNING_KEY: &str = "5f668a7ee96d944a4494cc947e4005e172d7ab3461ee5538f1f2a45a835e9657";
/// Secondary genesis vault address used as the transfer recipient.
const TRANSFER_RECIPIENT: &str = "11112oRqNpmKjfFCGgH6bw5csjBqVgb4PVRP5S98tTNjDeqdWNJr2L";

fn api_url(port: u16, path: &str) -> String {
    format!("http://{}:{}/api{}", host(), port, path)
}

async fn get_json(port: u16, path: &str) -> Option<Value> {
    let url = api_url(port, path);
    match Client::new().get(&url).send().await {
        Ok(resp) if resp.status().is_success() => resp.json().await.ok(),
        _ => None,
    }
}

async fn post_json(port: u16, path: &str, body: Value) -> Option<Value> {
    let url = api_url(port, path);
    match Client::new().post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => resp.json().await.ok(),
        _ => None,
    }
}

async fn get_status_code(port: u16, path: &str) -> Option<u16> {
    let url = api_url(port, path);
    Client::new()
        .get(&url)
        .send()
        .await
        .ok()
        .map(|r| r.status().as_u16())
}

async fn post_status_code(port: u16, path: &str, body: Value) -> Option<u16> {
    let url = api_url(port, path);
    Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .ok()
        .map(|r| r.status().as_u16())
}

/// Skip test if node not reachable
async fn require_shard() -> bool {
    get_json(http_port(), "/status").await.is_some()
}

/// True when validator and observer point to the same node (standalone mode)
fn is_standalone() -> bool {
    http_port() == observer_http()
}

// ============================================================================
// Status
// ============================================================================

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_status_fields() {
    if !require_shard().await {
        return;
    }

    let status = get_json(http_port(), "/status").await.unwrap();

    // Core fields
    assert!(status["version"]["api"].is_string());
    assert!(status["version"]["node"].is_string());
    assert!(status["address"].is_string());
    assert!(status["networkId"].is_string());
    assert!(status["shardId"].is_string());
    assert!(status["peers"].is_number());
    assert!(status["nodes"].is_number());
    assert!(status["minPhloPrice"].is_number());

    // Token metadata
    assert!(status["nativeTokenName"].is_string());
    assert!(status["nativeTokenSymbol"].is_string());
    assert!(status["nativeTokenDecimals"].is_number());

    // Operational state (Phase 4b)
    assert!(status["lastFinalizedBlockNumber"].is_number());
    assert!(status["isValidator"].is_boolean());
    assert!(status["isReadOnly"].is_boolean());
    assert!(status["isReady"].is_boolean());
    assert!(status["currentEpoch"].is_number());
    assert!(status["epochLength"].is_number());

    assert_eq!(status["isReady"], true, "node should be ready");
    assert!(
        status["epochLength"].as_i64().unwrap() > 0,
        "epochLength should be > 0"
    );
}

// ============================================================================
// Blocks
// ============================================================================

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_last_finalized_block_full() {
    if !require_shard().await {
        return;
    }

    let lfb = get_json(http_port(), "/last-finalized-block")
        .await
        .unwrap();

    assert!(lfb["blockInfo"].is_object(), "missing blockInfo");
    assert!(lfb["deploys"].is_array(), "full view should have deploys");

    let info = &lfb["blockInfo"];
    assert!(info["blockHash"].is_string());
    assert!(info["blockNumber"].as_i64().unwrap() >= 0);
    assert!(
        info["isFinalized"].as_bool().unwrap(),
        "LFB should be finalized"
    );
    assert!(info["faultTolerance"].is_number());
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_last_finalized_block_summary() {
    if !require_shard().await {
        return;
    }

    let lfb = get_json(http_port(), "/last-finalized-block?view=summary")
        .await
        .unwrap();

    assert!(lfb["blockInfo"].is_object(), "missing blockInfo");
    assert!(lfb.get("deploys").is_none(), "summary should omit deploys");
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_block_by_hash() {
    if !require_shard().await {
        return;
    }

    let lfb = get_json(http_port(), "/last-finalized-block")
        .await
        .unwrap();
    let hash = lfb["blockInfo"]["blockHash"].as_str().unwrap();

    let block = get_json(http_port(), &format!("/block/{hash}"))
        .await
        .unwrap();

    assert_eq!(block["blockInfo"]["blockHash"], hash);
    assert!(block["blockInfo"]["isFinalized"].as_bool().unwrap());
    assert!(block["deploys"].is_array(), "full view should have deploys");
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_block_summary_view() {
    if !require_shard().await {
        return;
    }

    let lfb = get_json(http_port(), "/last-finalized-block")
        .await
        .unwrap();
    let hash = lfb["blockInfo"]["blockHash"].as_str().unwrap();

    let block = get_json(http_port(), &format!("/block/{hash}?view=summary"))
        .await
        .unwrap();

    assert!(block["blockInfo"].is_object());
    assert!(
        block.get("deploys").is_none(),
        "summary should omit deploys"
    );
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_blocks_list_summary_default() {
    if !require_shard().await {
        return;
    }

    let blocks = get_json(http_port(), "/blocks/5").await.unwrap();
    let arr = blocks.as_array().unwrap();
    assert!(!arr.is_empty(), "should have blocks");

    for b in arr {
        assert!(b["blockInfo"].is_object(), "should have blockInfo wrapper");
        assert!(b["blockInfo"]["blockHash"].is_string());
        assert!(
            b.get("deploys").is_none(),
            "summary default should omit deploys"
        );
    }
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_blocks_list_full_view() {
    if !require_shard().await {
        return;
    }

    let blocks = get_json(http_port(), "/blocks/5?view=full").await.unwrap();
    let arr = blocks.as_array().unwrap();
    assert!(!arr.is_empty());

    let has_deploys = arr.iter().any(|b| b.get("deploys").is_some());
    assert!(
        has_deploys,
        "full view should include deploys on at least one block"
    );
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_blocks_by_height_range() {
    if !require_shard().await {
        return;
    }

    let lfb = get_json(http_port(), "/last-finalized-block")
        .await
        .unwrap();
    let lfb_num = lfb["blockInfo"]["blockNumber"].as_i64().unwrap();
    let start = (lfb_num - 2).max(0);

    let blocks = get_json(http_port(), &format!("/blocks/{start}/{lfb_num}"))
        .await
        .unwrap();
    let arr = blocks.as_array().unwrap();
    assert!(!arr.is_empty());

    for b in arr {
        let bn = b["blockInfo"]["blockNumber"].as_i64().unwrap();
        assert!(
            bn >= start && bn <= lfb_num,
            "block #{bn} outside range {start}-{lfb_num}"
        );
    }
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_is_finalized() {
    if !require_shard().await {
        return;
    }

    let lfb = get_json(http_port(), "/last-finalized-block")
        .await
        .unwrap();
    let hash = lfb["blockInfo"]["blockHash"].as_str().unwrap();

    let result = get_json(http_port(), &format!("/is-finalized/{hash}"))
        .await
        .unwrap();
    assert_eq!(result, true);
}

// ============================================================================
// Deploys
// ============================================================================

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_prepare_deploy() {
    if !require_shard().await {
        return;
    }

    let result = get_json(http_port(), "/prepare-deploy").await.unwrap();

    assert!(result["seqNumber"].is_number());
    assert!(result.get("names").is_some());
}

// ============================================================================
// Exploratory Deploy
// ============================================================================

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_explore_deploy() {
    if !require_shard().await {
        return;
    }

    let body = serde_json::json!({"term": "new ret in { ret!(42) }"});
    let result = post_json(observer_http(), "/explore-deploy", body)
        .await
        .unwrap();

    assert!(result["cost"].as_u64().unwrap() > 0, "cost should be > 0");
    assert!(result["expr"].is_array());
    assert!(result["block"].is_object());
}

// ============================================================================
// High-Level Query Endpoints
// ============================================================================

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_epoch() {
    if !require_shard().await {
        return;
    }

    // Works on all node types
    let result = get_json(http_port(), "/epoch").await.unwrap();

    assert!(result["currentEpoch"].is_number());
    assert!(result["epochLength"].as_i64().unwrap() > 0);
    assert!(result["quarantineLength"].is_number());
    assert!(result["blocksUntilNextEpoch"].as_i64().unwrap() > 0);
    assert!(result["lastFinalizedBlockNumber"].is_number());
    assert!(result["blockHash"].is_string());

    // Derived field check
    let lfb = result["lastFinalizedBlockNumber"].as_i64().unwrap();
    let epoch_len = result["epochLength"].as_i64().unwrap();
    let expected_epoch = lfb / epoch_len;
    assert_eq!(result["currentEpoch"].as_i64().unwrap(), expected_epoch);
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_validators() {
    if !require_shard().await {
        return;
    }

    let result = get_json(observer_http(), "/validators").await.unwrap();

    assert!(result["validators"].is_array());
    assert!(result["totalStake"].as_i64().unwrap() > 0);
    assert!(result["blockNumber"].is_number());
    assert!(result["blockHash"].is_string());

    let validators = result["validators"].as_array().unwrap();
    let min = if is_standalone() { 1 } else { 2 };
    assert!(
        validators.len() >= min,
        "expected at least {} validators, got {}",
        min,
        validators.len()
    );

    for v in validators {
        assert!(v["publicKey"].is_string());
        assert!(v["stake"].as_i64().unwrap() > 0);
    }
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_validator_bonded() {
    if !require_shard().await {
        return;
    }

    // Get a real validator pubkey
    let validators = get_json(observer_http(), "/validators").await.unwrap();
    let pubkey = validators["validators"][0]["publicKey"].as_str().unwrap();

    let result = get_json(observer_http(), &format!("/validator/{pubkey}"))
        .await
        .unwrap();

    assert_eq!(result["publicKey"], pubkey);
    assert_eq!(result["isBonded"], true);
    assert!(result["stake"].as_i64().unwrap() > 0);
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_validator_unknown() {
    if !require_shard().await {
        return;
    }

    let result = get_json(observer_http(), &format!("/validator/{UNBONDED_PUBKEY}"))
        .await
        .unwrap();

    assert_eq!(result["isBonded"], false);
    assert!(result["stake"].is_null());
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_bond_status_bonded() {
    if !require_shard().await {
        return;
    }

    // Get real pubkey from LFB bonds
    let lfb = get_json(http_port(), "/last-finalized-block")
        .await
        .unwrap();
    let pubkey = lfb["blockInfo"]["bonds"][0]["validator"].as_str().unwrap();

    // Works on all node types (no exploratory deploy)
    let result = get_json(http_port(), &format!("/bond-status/{pubkey}"))
        .await
        .unwrap();

    assert_eq!(result["publicKey"], pubkey);
    assert_eq!(result["isBonded"], true);
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_bond_status_unknown() {
    if !require_shard().await {
        return;
    }

    let result = get_json(http_port(), &format!("/bond-status/{UNBONDED_PUBKEY}"))
        .await
        .unwrap();

    assert_eq!(result["isBonded"], false);
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_epoch_rewards() {
    if !require_shard().await {
        return;
    }

    let result = get_json(observer_http(), "/epoch/rewards").await.unwrap();

    assert!(result["rewards"].is_object());
    assert!(result["blockNumber"].is_number());
    assert!(result["blockHash"].is_string());

    // Rewards should be an ExprMap
    assert!(result["rewards"]["ExprMap"].is_object());
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_estimate_cost() {
    if !require_shard().await {
        return;
    }

    let body = serde_json::json!({"term": "new ret in { ret!(42) }"});
    let result = post_json(observer_http(), "/estimate-cost", body)
        .await
        .unwrap();

    assert!(result["cost"].as_u64().unwrap() > 0);
    assert!(result["blockNumber"].is_number());
    assert!(result["blockHash"].is_string());
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_estimate_cost_invalid_syntax() {
    if !require_shard().await {
        return;
    }

    let body = serde_json::json!({"term": "invalid {{{{ rholang"});
    let url = api_url(observer_http(), "/estimate-cost");
    let resp = Client::new().post(&url).json(&body).send().await.unwrap();

    assert!(!resp.status().is_success(), "invalid syntax should fail");
}

// ============================================================================
// Removed Endpoints
// ============================================================================

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_removed_data_at_name_returns_404() {
    if !require_shard().await {
        return;
    }

    let body = serde_json::json!({"name": {"UnforgDeploy": {"data": "abc"}}, "depth": 1});
    let code = post_status_code(http_port(), "/data-at-name", body)
        .await
        .unwrap();
    assert_eq!(code, 404);
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_removed_transactions_returns_404() {
    if !require_shard().await {
        return;
    }

    let code = get_status_code(http_port(), "/transactions/abc123")
        .await
        .unwrap();
    assert_eq!(code, 404);
}

// ============================================================================
// View Parameter Edge Cases
// ============================================================================

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_unknown_view_defaults_to_full() {
    if !require_shard().await {
        return;
    }

    // Unknown view should return full (with deploys)
    let block = get_json(http_port(), "/last-finalized-block?view=bogus")
        .await
        .unwrap();
    assert!(
        block["deploys"].is_array(),
        "unknown view should default to full"
    );
}

// ============================================================================
// Query with explicit block_hash
// ============================================================================

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_epoch_with_block_hash() {
    if !require_shard().await {
        return;
    }

    let lfb = get_json(http_port(), "/last-finalized-block")
        .await
        .unwrap();
    let hash = lfb["blockInfo"]["blockHash"].as_str().unwrap();

    let result = get_json(http_port(), &format!("/epoch?block_hash={hash}"))
        .await
        .unwrap();

    assert_eq!(result["blockHash"], hash);
    assert!(result["epochLength"].as_i64().unwrap() > 0);
}

// ============================================================================
// Deploy finalization status
// ============================================================================

/// Unknown sig should return Pending with empty fields, not 404 or 500.
/// The endpoint deliberately reports `pending_unknown` for sigs the deploy
/// index has never seen, so polling clients can keep retrying without
/// distinguishing "just submitted" from "actually unknown".
#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_deploy_finalization_status_unknown_sig() {
    if !require_shard().await {
        return;
    }

    let unknown_sig = "0".repeat(64);
    let result = get_json(
        observer_http(),
        &format!("/deploy-finalization-status/{unknown_sig}"),
    )
    .await
    .unwrap_or_else(|| panic!("endpoint returned non-200 for unknown sig"));

    assert_eq!(result["state"], "Pending");
    assert_eq!(result["rejection_count"], 0);
    assert!(result["latest_block_hash"].is_null());
}

/// 0x prefix should be tolerated on the sig path parameter (the CLI
/// strips it; this confirms the endpoint also accepts it).
#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_deploy_finalization_status_invalid_hex_returns_400() {
    if !require_shard().await {
        return;
    }

    let bad_hex = "not-hex-at-all";
    let code = get_status_code(
        observer_http(),
        &format!("/deploy-finalization-status/{bad_hex}"),
    )
    .await
    .unwrap_or(0);

    assert_eq!(code, 400, "expected 400 for invalid hex, got {code}");
}

/// Response shape regression: state, rejection_count, latest_block_hash.
/// Locks the JSON contract so unintentional renames break this test.
#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_deploy_finalization_status_response_shape() {
    if !require_shard().await {
        return;
    }

    let unknown_sig = "1".repeat(64);
    let result = get_json(
        observer_http(),
        &format!("/deploy-finalization-status/{unknown_sig}"),
    )
    .await
    .unwrap();

    let obj = result.as_object().expect("response is a JSON object");
    assert!(obj.contains_key("state"));
    assert!(obj.contains_key("rejection_count"));
    assert!(obj.contains_key("latest_block_hash"));
    assert!(obj["state"].is_string());
    assert!(obj["rejection_count"].is_u64());
}

#[tokio::test]
#[ignore] // Requires running node. Run with: cargo test --test smoke --release -- --ignored
async fn test_vault_transfer_succeeds_on_chain() {
    if !require_shard().await {
        return;
    }

    // Regression test: the RevVault transfer contract costs well over the 50k
    // default deploy limit (~249k on f1r3node-rust v0.4.23), so a transfer
    // deployed with the default limit consumes it fully and errors on-chain —
    // while transfer() still returns Ok, because TransferResult carries no
    // execution outcome. The failure is only visible in the deploy detail,
    // which is what this test asserts on.
    //
    // Runs against the Rust node only: CI gates the cargo smoke step on
    // matrix.node == 'rust' (the Scala node is deprecated).
    let mut config = node_cli::ConnectionConfig::new(
        host(),
        grpc_port(),
        http_port(),
        FUNDED_SIGNING_KEY.to_string(),
    );
    config.observer_grpc_port = grpc_port();

    let manager = node_cli::F1r3flyConnectionManager::new(config);
    let transfer = manager
        .transfer(TRANSFER_RECIPIENT, 1_000)
        .await
        .expect("transfer should deploy and finalize");

    let detail = get_json(http_port(), &format!("/deploy/{}", transfer.deploy_id))
        .await
        .expect("deploy detail should be readable after finalization");
    assert_eq!(
        detail["errored"], false,
        "transfer deploy errored on-chain (phlo limit exhausted?): {detail}"
    );
    let cost = detail["cost"].as_u64().expect("deploy detail carries cost");
    assert!(
        cost <= node_cli::vault::TRANSFER_PHLO_LIMIT as u64,
        "transfer cost {cost} exceeds TRANSFER_PHLO_LIMIT — raise the limit"
    );
}
