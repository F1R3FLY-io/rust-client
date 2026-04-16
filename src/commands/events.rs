use crate::args::WatchEventsArgs;
use crate::error::{NodeCliError, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// F1R3FLY node event from WebSocket /ws/events endpoint.
///
/// The node defines 9 event types in F1r3flyEvent:
///   Block lifecycle:  block-created, block-added, block-finalised
///   Genesis ceremony: sent-unapproved-block, sent-approved-block,
///                     block-approval-received, approved-block-received
///   Node lifecycle:   entered-running-state, node-started
///
/// The "started" variant is a WebSocket handshake (not an F1r3flyEvent).
#[derive(Debug, Deserialize)]
#[serde(tag = "event")]
#[serde(rename_all = "kebab-case")]
pub enum NodeEvent {
    // WebSocket handshake
    Started {
        #[serde(rename = "schema-version")]
        schema_version: i32,
    },
    // Block lifecycle
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
    // Genesis ceremony
    SentUnapprovedBlock {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: BlockHashPayload,
    },
    SentApprovedBlock {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: BlockHashPayload,
    },
    BlockApprovalReceived {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: ApprovalPayload,
    },
    ApprovedBlockReceived {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: BlockHashPayload,
    },
    // Node lifecycle
    EnteredRunningState {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: BlockHashPayload,
    },
    NodeStarted {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: NodeStartedPayload,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BlockEventPayload {
    pub block_hash: String,
    pub parent_hashes: Vec<String>,
    pub justification_hashes: Vec<(String, String)>,
    pub deploys: Vec<BlockEventDeploy>,
    pub creator: String,
    pub seq_num: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BlockEventDeploy {
    pub id: String,
    pub cost: u64,
    pub deployer: String,
    pub errored: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct FinalizedBlockPayload {
    pub block_hash: String,
    pub parent_hashes: Vec<String>,
    pub justification_hashes: Vec<(String, String)>,
    pub deploys: Vec<BlockEventDeploy>,
    pub creator: String,
    pub seq_num: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BlockHashPayload {
    pub block_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ApprovalPayload {
    pub block_hash: String,
    pub sender: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeStartedPayload {
    pub address: String,
}

/// Statistics for the watch session
struct EventStats {
    created: u32,
    added: u32,
    finalized: u32,
    genesis: u32,
    lifecycle: u32,
    total: u32,
}

impl EventStats {
    fn new() -> Self {
        Self {
            created: 0,
            added: 0,
            finalized: 0,
            genesis: 0,
            lifecycle: 0,
            total: 0,
        }
    }

    fn increment(&mut self, event: &NodeEvent) {
        self.total += 1;
        match event {
            NodeEvent::BlockCreated { .. } => self.created += 1,
            NodeEvent::BlockAdded { .. } => self.added += 1,
            NodeEvent::BlockFinalised { .. } => self.finalized += 1,
            NodeEvent::SentUnapprovedBlock { .. }
            | NodeEvent::SentApprovedBlock { .. }
            | NodeEvent::BlockApprovalReceived { .. }
            | NodeEvent::ApprovedBlockReceived { .. } => self.genesis += 1,
            NodeEvent::EnteredRunningState { .. }
            | NodeEvent::NodeStarted { .. } => self.lifecycle += 1,
            NodeEvent::Started { .. } => {}
        }
    }

    fn print_summary(&self, duration: std::time::Duration) {
        println!("\n Event Statistics:");
        println!(" Total Events: {}", self.total);
        println!(" - Created:    {}", self.created);
        println!(" - Added:      {}", self.added);
        println!(" - Finalized:  {}", self.finalized);
        if self.genesis > 0 {
            println!(" - Genesis:    {}", self.genesis);
        }
        if self.lifecycle > 0 {
            println!(" - Lifecycle:  {}", self.lifecycle);
        }
        println!(" Duration:     {:.1}s", duration.as_secs_f64());
        if duration.as_secs() > 0 {
            let rate = self.total as f64 / duration.as_secs_f64();
            println!(" Rate:         {:.2} events/sec", rate);
        }
    }
}

/// Watch blocks command - connects to WebSocket and streams block events
pub async fn watch_events_command(args: &WatchEventsArgs) -> Result<()> {
    let ws_url = format!("ws://{}:{}/ws/events", args.host, args.http_port);

    println!(" Connecting to F1r3fly node WebSocket...");
    println!(" URL: {}", ws_url);

    if let Some(filter) = &args.filter {
        println!(" Filter: {}", filter);
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
                break;
            }
            Err(e) => {
                retry_count += 1;

                if !args.retry_forever && retry_count > MAX_RETRIES {
                    println!(" Max reconnection attempts ({}) reached", MAX_RETRIES);
                    return Err(e);
                }

                println!(" Connection lost: {}", e);

                if args.retry_forever {
                    println!(
                        " Reconnecting in {} seconds... (attempt {})",
                        RETRY_DELAY_SECS, retry_count
                    );
                } else {
                    println!(
                        " Reconnecting in {} seconds... (attempt {}/{})",
                        RETRY_DELAY_SECS, retry_count, MAX_RETRIES
                    );
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
                println!(" Reconnecting to {}...", ws_url);
            }
        }
    }

    let duration = start_time.elapsed();
    stats.print_summary(duration);

    Ok(())
}

async fn connect_and_watch(
    ws_url: &str,
    args: &WatchEventsArgs,
    stats: &mut EventStats,
) -> Result<()> {
    let (ws_stream, _) = connect_async(ws_url).await.map_err(|e| {
        NodeCliError::network_connection_failed(&format!("WebSocket connection failed: {}", e))
    })?;

    println!(" Connected to node WebSocket");
    println!(" Watching for block events... (Press Ctrl+C to stop)\n");

    let (mut _write, mut read) = ws_stream.split();

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
        _ = &mut ctrl_c => {
        println!("\n Shutting down gracefully...");
        return Ok(());
        }
        msg = read.next() => {
        match msg {
        Some(Ok(Message::Text(text))) => {
        if let Err(e) = handle_event(&text, args, stats) {
        eprintln!(" Error processing event: {}", e);
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

fn handle_event(text: &str, args: &WatchEventsArgs, stats: &mut EventStats) -> Result<()> {
    let event: NodeEvent = serde_json::from_str(text)
        .map_err(|e| NodeCliError::from(format!("Failed to parse event: {}", e)))?;

    if let Some(filter) = &args.filter {
        let matches = match (&event, filter.as_str()) {
            (NodeEvent::BlockCreated { .. }, "created") => true,
            (NodeEvent::BlockAdded { .. }, "added") => true,
            (NodeEvent::BlockFinalised { .. }, "finalized" | "finalised") => true,
            (NodeEvent::SentUnapprovedBlock { .. }, "genesis") => true,
            (NodeEvent::SentApprovedBlock { .. }, "genesis") => true,
            (NodeEvent::BlockApprovalReceived { .. }, "genesis") => true,
            (NodeEvent::ApprovedBlockReceived { .. }, "genesis") => true,
            (NodeEvent::EnteredRunningState { .. }, "lifecycle") => true,
            (NodeEvent::NodeStarted { .. }, "lifecycle") => true,
            _ => false,
        };

        if !matches {
            return Ok(());
        }
    }

    stats.increment(&event);
    display_pretty(&event);
    Ok(())
}

fn display_pretty(event: &NodeEvent) {
    match event {
        NodeEvent::Started { .. } => {
            println!(" WebSocket connection started\n");
        }
        NodeEvent::BlockCreated { payload, .. } => {
            println!(" Block Created");
            display_block_payload(payload);
        }
        NodeEvent::BlockAdded { payload, .. } => {
            println!(" Block Added");
            display_block_payload(payload);
        }
        NodeEvent::BlockFinalised { payload, .. } => {
            println!(" Block Finalized");
            println!(" Hash:     {}", payload.block_hash);
            println!(" Creator:  {}", payload.creator);
            println!(" Seq Num:  {}", payload.seq_num);
            println!(" Parents:  {}", payload.parent_hashes.len());
            if !payload.deploys.is_empty() {
                println!(
                    " Deploys:  {} [{}]",
                    payload.deploys.len(),
                    payload.deploys.iter().map(|d| d.id.clone()).collect::<Vec<_>>().join(", ")
                );
            } else {
                println!(" Deploys:  {}", payload.deploys.len());
            }
            println!();
        }
        NodeEvent::SentUnapprovedBlock { payload, .. } => {
            println!(" Sent Unapproved Block");
            println!(" Hash: {}", payload.block_hash);
            println!();
        }
        NodeEvent::SentApprovedBlock { payload, .. } => {
            println!(" Sent Approved Block");
            println!(" Hash: {}", payload.block_hash);
            println!();
        }
        NodeEvent::BlockApprovalReceived { payload, .. } => {
            println!(" Block Approval Received");
            println!(" Hash:   {}", payload.block_hash);
            println!(" Sender: {}", payload.sender);
            println!();
        }
        NodeEvent::ApprovedBlockReceived { payload, .. } => {
            println!(" Approved Block Received");
            println!(" Hash: {}", payload.block_hash);
            println!();
        }
        NodeEvent::EnteredRunningState { payload, .. } => {
            println!(" Entered Running State");
            println!(" Block: {}", payload.block_hash);
            println!();
        }
        NodeEvent::NodeStarted { payload, .. } => {
            println!(" Node Started");
            println!(" Address: {}", payload.address);
            println!();
        }
    }
}

fn display_block_payload(payload: &BlockEventPayload) {
    println!(" Hash:     {}", payload.block_hash);
    println!(" Creator:  {}", payload.creator);
    println!(" Seq Num:  {}", payload.seq_num);
    println!(" Parents:  {}", payload.parent_hashes.len());
    if !payload.deploys.is_empty() {
        println!(
            " Deploys:  {} [{}]",
            payload.deploys.len(),
            payload.deploys.iter().map(|d| d.id.clone()).collect::<Vec<_>>().join(", ")
        );
    } else {
        println!(" Deploys:  {}", payload.deploys.len());
    }
    println!();
}
