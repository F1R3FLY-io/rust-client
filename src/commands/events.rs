use crate::args::WatchBlocksArgs;
use crate::error::{NodeCliError, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// RChain blockchain event from WebSocket
#[derive(Debug, Deserialize)]
#[serde(tag = "event")]
#[serde(rename_all = "kebab-case")]
pub enum RChainEvent {
    Started {
        #[serde(rename = "schema-version")]
        schema_version: i32,
    },
    BlockCreated {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: BlockEventPayload,
    },
    BlockAdded {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: BlockEventPayload,
    },
    BlockFinalised {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: FinalizedBlockPayload,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BlockEventPayload {
    pub block_hash: String,
    pub parent_hashes: Vec<String>,
    pub justification_hashes: Vec<(String, String)>,
    pub deploy_ids: Vec<String>,
    pub creator: String,
    pub seq_num: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct FinalizedBlockPayload {
    pub block_hash: String,
}

/// Statistics for the watch session
struct EventStats {
    created: u32,
    added: u32,
    finalized: u32,
    total: u32,
}

impl EventStats {
    fn new() -> Self {
        Self {
            created: 0,
            added: 0,
            finalized: 0,
            total: 0,
        }
    }

    fn increment(&mut self, event: &RChainEvent) {
        self.total += 1;
        match event {
            RChainEvent::BlockCreated { .. } => self.created += 1,
            RChainEvent::BlockAdded { .. } => self.added += 1,
            RChainEvent::BlockFinalised { .. } => self.finalized += 1,
            RChainEvent::Started { .. } => {}
        }
    }

    fn print_summary(&self, duration: std::time::Duration) {
        println!("\n📊 Event Statistics:");
        println!("   Total Events: {}", self.total);
        println!("   - Created:   {}", self.created);
        println!("   - Added:     {}", self.added);
        println!("   - Finalized: {}", self.finalized);
        println!("   Duration:    {:.1}s", duration.as_secs_f64());
        if duration.as_secs() > 0 {
            let rate = self.total as f64 / duration.as_secs_f64();
            println!("   Rate:        {:.2} events/sec", rate);
        }
    }
}

/// Watch blocks command - connects to WebSocket and streams block events
pub async fn watch_blocks_command(args: &WatchBlocksArgs) -> Result<()> {
    let ws_url = format!("ws://{}:{}/ws/events", args.host, args.http_port);

    println!("🔌 Connecting to F1r3fly node WebSocket...");
    println!("   URL: {}", ws_url);

    if let Some(filter) = &args.filter {
        println!("   Filter: {}", filter);
    }
    println!();

    let mut stats = EventStats::new();
    let start_time = std::time::Instant::now();
    let mut retry_count = 0;
    const MAX_RETRIES: u32 = 10;
    const RETRY_DELAY_SECS: u64 = 10;

    loop {
        match connect_and_watch(&ws_url, args, &mut stats).await {
            Ok(_) => {
                // Normal exit
                break;
            }
            Err(e) => {
                retry_count += 1;
                
                // Check if we should stop retrying
                if !args.retry_forever && retry_count > MAX_RETRIES {
                    println!("❌ Max reconnection attempts ({}) reached", MAX_RETRIES);
                    return Err(e);
                }

                println!("⚠️  Connection lost: {}", e);
                
                if args.retry_forever {
                    println!(
                        "🔄 Reconnecting in {} seconds... (attempt {})",
                        RETRY_DELAY_SECS, retry_count
                    );
                } else {
                    println!(
                        "🔄 Reconnecting in {} seconds... (attempt {}/{})",
                        RETRY_DELAY_SECS, retry_count, MAX_RETRIES
                    );
                }
                
                tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
                println!("🔌 Reconnecting to {}...", ws_url);
            }
        }
    }

    let duration = start_time.elapsed();
    stats.print_summary(duration);

    Ok(())
}

async fn connect_and_watch(
    ws_url: &str,
    args: &WatchBlocksArgs,
    stats: &mut EventStats,
) -> Result<()> {
    let (ws_stream, _) = connect_async(ws_url).await.map_err(|e| {
        NodeCliError::network_connection_failed(&format!("WebSocket connection failed: {}", e))
    })?;

    println!("✅ Connected to node WebSocket");
    println!("👁️  Watching for block events... (Press Ctrl+C to stop)\n");

    let (mut _write, mut read) = ws_stream.split();

    // Set up Ctrl+C handler
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                println!("\n🛑 Shutting down gracefully...");
                return Ok(());
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_event(&text, args, stats) {
                            eprintln!("⚠️  Error processing event: {}", e);
                            continue;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(NodeCliError::network_connection_failed("WebSocket closed by server"));
                    }
                    Some(Err(e)) => {
                        return Err(NodeCliError::network_connection_failed(&format!("WebSocket error: {}", e)));
                    }
                    None => {
                        return Err(NodeCliError::network_connection_failed("WebSocket stream ended"));
                    }
                    _ => continue,
                }
            }
        }
    }
}

fn handle_event(text: &str, args: &WatchBlocksArgs, stats: &mut EventStats) -> Result<()> {
    let event: RChainEvent = serde_json::from_str(text)
        .map_err(|e| NodeCliError::from(format!("Failed to parse event: {}", e)))?;

    // Apply filter
    if let Some(filter) = &args.filter {
        let matches = match (&event, filter.as_str()) {
            (RChainEvent::BlockCreated { .. }, "created") => true,
            (RChainEvent::BlockAdded { .. }, "added") => true,
            (RChainEvent::BlockFinalised { .. }, "finalized" | "finalised") => true,
            _ => false,
        };

        if !matches {
            return Ok(());
        }
    }

    stats.increment(&event);

    // Display in pretty format with deploys shown
    display_pretty(&event);

    Ok(())
}

fn display_pretty(event: &RChainEvent) {
    match event {
        RChainEvent::Started { .. } => {
            println!("🚀 WebSocket connection started\n");
        }
        RChainEvent::BlockCreated { payload, .. } => {
            println!("🆕 Block Created");
            println!("├─ Hash:     {}", payload.block_hash);
            println!("├─ Creator:  {}", payload.creator);
            println!("├─ Seq Num:  {}", payload.seq_num);
            println!("├─ Parents:  {}", payload.parent_hashes.len());

            if !payload.deploy_ids.is_empty() {
                println!(
                    "└─ Deploys:  {} [{}]",
                    payload.deploy_ids.len(),
                    payload
                        .deploy_ids
                        .iter()
                        .take(3)
                        .map(|d| d.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            } else {
                println!("└─ Deploys:  {}", payload.deploy_ids.len());
            }
            println!();
        }
        RChainEvent::BlockAdded { payload, .. } => {
            println!("📦 Block Added");
            println!("├─ Hash:     {}", payload.block_hash);
            println!("├─ Creator:  {}", payload.creator);
            println!("├─ Seq Num:  {}", payload.seq_num);
            println!("├─ Parents:  {}", payload.parent_hashes.len());

            if !payload.deploy_ids.is_empty() {
                println!(
                    "└─ Deploys:  {} [{}]",
                    payload.deploy_ids.len(),
                    payload
                        .deploy_ids
                        .iter()
                        .take(3)
                        .map(|d| d.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            } else {
                println!("└─ Deploys:  {}", payload.deploy_ids.len());
            }
            println!();
        }
        RChainEvent::BlockFinalised { payload, .. } => {
            println!("✅ Block Finalized");
            println!("└─ Hash:     {}", payload.block_hash);
            println!();
        }
    }
}
