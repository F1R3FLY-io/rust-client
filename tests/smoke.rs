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

    let block = get_json(http_port(), &format!("/block/{}", hash))
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

    let block = get_json(http_port(), &format!("/block/{}?view=summary", hash))
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

    let blocks = get_json(http_port(), &format!("/blocks/{}/{}", start, lfb_num))
        .await
        .unwrap();
    let arr = blocks.as_array().unwrap();
    assert!(!arr.is_empty());

    for b in arr {
        let bn = b["blockInfo"]["blockNumber"].as_i64().unwrap();
        assert!(
            bn >= start && bn <= lfb_num,
            "block #{} outside range {}-{}",
            bn,
            start,
            lfb_num
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

    let result = get_json(http_port(), &format!("/is-finalized/{}", hash))
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

    let result = get_json(observer_http(), &format!("/validator/{}", pubkey))
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

    let fake = "aa".repeat(65);
    let result = get_json(observer_http(), &format!("/validator/{}", fake))
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
    let result = get_json(http_port(), &format!("/bond-status/{}", pubkey))
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

    let fake = "bb".repeat(65);
    let result = get_json(http_port(), &format!("/bond-status/{}", fake))
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

    let result = get_json(http_port(), &format!("/epoch?block_hash={}", hash))
        .await
        .unwrap();

    assert_eq!(result["blockHash"], hash);
    assert!(result["epochLength"].as_i64().unwrap() > 0);
}
