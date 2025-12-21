use chrono::{TimeZone, Utc};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;

use crate::args::DagArgs;
use crate::dag::{BlockStatus, DagApp, DagBlock, DagEvent};
use crate::error::NodeCliError;

/// Run the DAG visualization command
pub async fn run_dag(args: &DagArgs) -> Result<(), NodeCliError> {
    println!("Loading blocks from {}:{}...", args.host, args.http_port);

    // Create channel for WebSocket events
    let (tx, rx) = mpsc::channel::<DagEvent>(100);

    // Create the app
    let mut app = DagApp::new();
    app.renderer.show_deploys = args.show_deploys;

    // Load initial blocks
    let blocks = fetch_initial_blocks(&args.host, args.http_port, args.depth).await?;
    app.load_blocks(blocks);

    // Set up event receiver if live mode
    if !args.no_live {
        app = app.with_event_receiver(rx);

        // Spawn WebSocket listener (same port as HTTP API)
        let ws_url = format!("ws://{}:{}/ws/events", args.host, args.http_port);
        let api_base = format!("http://{}:{}", args.host, args.http_port);
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = run_websocket_listener(ws_url, api_base, tx_clone).await {
                eprintln!("WebSocket error: {}", e);
            }
        });
    }

    // Run the TUI
    app.run().await.map_err(|e| NodeCliError::io_error(&e.to_string()))?;

    Ok(())
}

/// Fetch initial blocks from the API
async fn fetch_initial_blocks(
    host: &str,
    port: u16,
    depth: usize,
) -> Result<Vec<DagBlock>, NodeCliError> {
    let url = format!("http://{}:{}/api/blocks/{}", host, port, depth);

    let response = reqwest::get(&url)
        .await
        .map_err(|e| NodeCliError::http_error(&e.to_string()))?;

    if !response.status().is_success() {
        return Err(NodeCliError::http_error(&format!(
            "Failed to fetch blocks: {}",
            response.status()
        )));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| NodeCliError::http_error(&e.to_string()))?;

    let mut blocks = Vec::new();

    // Parse the response - it should be an array of blocks
    if let Some(block_array) = body.as_array() {
        for block_json in block_array {
            if let Some(block) = parse_block_json(block_json) {
                blocks.push(block);
            }
        }
    }

    Ok(blocks)
}

/// Parse a block from JSON
fn parse_block_json(json: &serde_json::Value) -> Option<DagBlock> {
    let hash = json.get("blockHash")?.as_str()?.to_string();
    let block_number = json.get("blockNumber")?.as_i64()?;
    let timestamp_ms = json.get("timestamp")?.as_i64().unwrap_or(0);
    let timestamp = Utc.timestamp_millis_opt(timestamp_ms).single().unwrap_or_else(Utc::now);
    let creator = json.get("sender")?.as_str()?.to_string();
    let seq_num = json.get("seqNum")?.as_i64().unwrap_or(0);

    let parents: Vec<String> = json
        .get("parentsHashList")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let deploy_count = json.get("deployCount")?.as_i64().unwrap_or(0) as u32;

    // Assume finalized for historical blocks
    let status = BlockStatus::Finalized;

    let mut block = DagBlock::new(
        hash,
        block_number,
        timestamp,
        creator,
        seq_num,
        parents,
        deploy_count,
        status,
    );

    // Optional fields
    if let Some(shard) = json.get("shardId").and_then(|s| s.as_str()) {
        block.shard_id = shard.to_string();
    }
    if let Some(pre) = json.get("preStateHash").and_then(|s| s.as_str()) {
        block.pre_state_hash = pre.to_string();
    }
    if let Some(post) = json.get("postStateHash").and_then(|s| s.as_str()) {
        block.post_state_hash = post.to_string();
    }

    Some(block)
}

/// Fetch a single block by hash from the API with retries
async fn fetch_block_by_hash(api_base: &str, hash: &str) -> Option<DagBlock> {
    // Retry a few times with delays - the block might not be available immediately
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        let url = format!("{}/api/block/{}", api_base, hash);
        if let Ok(response) = reqwest::get(&url).await {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                // Response format: {"blockInfo": {...}, "deploys": [...]}
                if let Some(block_info) = body.get("blockInfo") {
                    if let Some(block) = parse_block_info_json(block_info) {
                        return Some(block);
                    }
                }
            }
        }
    }
    None
}

/// Parse a block from the /api/block/{hash} response format
fn parse_block_info_json(json: &serde_json::Value) -> Option<DagBlock> {
    let hash = json.get("blockHash")?.as_str()?.to_string();
    let block_number = json.get("blockNumber")?.as_i64()?;
    let timestamp_ms = json.get("timestamp")?.as_i64().unwrap_or(0);
    let timestamp = Utc.timestamp_millis_opt(timestamp_ms).single().unwrap_or_else(Utc::now);
    let creator = json.get("sender")?.as_str()?.to_string();
    let seq_num = json.get("seqNum")?.as_i64().unwrap_or(0);

    let parents: Vec<String> = json
        .get("parentsHashList")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let deploy_count = json.get("deployCount")?.as_i64().unwrap_or(0) as u32;

    // Assume finalized for fetched blocks
    let status = BlockStatus::Finalized;

    let mut block = DagBlock::new(
        hash,
        block_number,
        timestamp,
        creator,
        seq_num,
        parents,
        deploy_count,
        status,
    );

    // Optional fields
    if let Some(shard) = json.get("shardId").and_then(|s| s.as_str()) {
        block.shard_id = shard.to_string();
    }
    if let Some(pre) = json.get("preStateHash").and_then(|s| s.as_str()) {
        block.pre_state_hash = pre.to_string();
    }
    if let Some(post) = json.get("postStateHash").and_then(|s| s.as_str()) {
        block.post_state_hash = post.to_string();
    }

    Some(block)
}

/// Run the WebSocket listener for real-time events
async fn run_websocket_listener(
    ws_url: String,
    api_base: String,
    tx: mpsc::Sender<DagEvent>,
) -> Result<(), NodeCliError> {
    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| NodeCliError::websocket_error(&e.to_string()))?;

    let (_, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                if let Ok(event) = parse_websocket_event(&text) {
                    // For all block events, fetch full block info via HTTP
                    // to get the correct block number
                    let enriched_event = match &event {
                        DagEvent::BlockCreated(block) => {
                            if let Some(mut full_block) = fetch_block_by_hash(&api_base, &block.hash).await {
                                full_block.status = BlockStatus::Created;
                                DagEvent::BlockCreated(full_block)
                            } else {
                                event
                            }
                        }
                        DagEvent::BlockAdded(hash) => {
                            // Fetch full block and return as BlockCreated with Added status
                            // This ensures we have block_number even if we missed BlockCreated
                            if let Some(mut full_block) = fetch_block_by_hash(&api_base, hash).await {
                                full_block.status = BlockStatus::Added;
                                DagEvent::BlockCreated(full_block)
                            } else {
                                event
                            }
                        }
                        DagEvent::BlockFinalized(hash) => {
                            // Fetch full block and return as BlockCreated with Finalized status
                            if let Some(mut full_block) = fetch_block_by_hash(&api_base, hash).await {
                                full_block.status = BlockStatus::Finalized;
                                DagEvent::BlockCreated(full_block)
                            } else {
                                event
                            }
                        }
                        _ => event,
                    };

                    if tx.send(enriched_event).await.is_err() {
                        // Receiver dropped, exit
                        break;
                    }
                }
            }
            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                break;
            }
            Err(e) => {
                let _ = tx.send(DagEvent::Error(e.to_string())).await;
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

/// Parse a WebSocket event into a DagEvent
/// The node sends events in this format:
/// {"event": "block-created", "schema-version": 1, "payload": {...}}
fn parse_websocket_event(text: &str) -> Result<DagEvent, NodeCliError> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| NodeCliError::parse_error(&e.to_string()))?;

    // Get event type (kebab-case)
    let event_type = json
        .get("event")
        .and_then(|e| e.as_str())
        .unwrap_or("");

    let payload = json.get("payload");

    match event_type {
        "block-created" => {
            if let Some(p) = payload {
                let block = parse_event_block(p, BlockStatus::Created)?;
                return Ok(DagEvent::BlockCreated(block));
            }
        }
        "block-added" => {
            if let Some(p) = payload {
                if let Some(hash) = p.get("block-hash").and_then(|h| h.as_str()) {
                    return Ok(DagEvent::BlockAdded(hash.to_string()));
                }
            }
        }
        "block-finalised" => {
            if let Some(p) = payload {
                if let Some(hash) = p.get("block-hash").and_then(|h| h.as_str()) {
                    return Ok(DagEvent::BlockFinalized(hash.to_string()));
                }
            }
        }
        "started" => {
            // Initial connection event, ignore
            return Err(NodeCliError::parse_error("Ignoring started event"));
        }
        _ => {}
    }

    Err(NodeCliError::parse_error(&format!("Unknown event type: {}", event_type)))
}

/// Parse a block from WebSocket event payload (kebab-case fields)
/// Note: WebSocket events contain seq-num (validator sequence) not block number.
/// We set block_number to -1 to indicate it needs to be fetched.
fn parse_event_block(payload: &serde_json::Value, status: BlockStatus) -> Result<DagBlock, NodeCliError> {
    let hash = payload
        .get("block-hash")
        .and_then(|h| h.as_str())
        .ok_or_else(|| NodeCliError::parse_error("Missing block-hash"))?
        .to_string();

    let creator = payload
        .get("creator")
        .and_then(|c| c.as_str())
        .unwrap_or("unknown")
        .to_string();

    let seq_num = payload
        .get("seq-num")
        .and_then(|s| s.as_i64())
        .unwrap_or(0);

    let parents: Vec<String> = payload
        .get("parent-hashes")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let deploy_count = payload
        .get("deploys")
        .and_then(|d| d.as_array())
        .map(|arr| arr.len() as u32)
        .unwrap_or(0);

    // WebSocket events don't include blockNumber, only seqNum.
    // Use -1 as placeholder; the block will be refetched via HTTP for accurate info.
    let block = DagBlock::new(
        hash,
        -1, // Block number unknown from WebSocket event
        Utc::now(),
        creator,
        seq_num,
        parents,
        deploy_count,
        status,
    );

    Ok(block)
}
