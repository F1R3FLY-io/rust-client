use crate::args::WatchEventsArgs;
use crate::error::{NodeCliError, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// F1R3FLY node event from WebSocket /ws/events endpoint.
///
/// The node defines 10 event types in F1r3flyEvent:
///   Block lifecycle:  block-created, block-added, block-finalised
///   Transfer:         transfers-available (readonly only, after block report)
///   Genesis ceremony: sent-unapproved-block, sent-approved-block,
///                     block-approval-received, approved-block-received
///   Node lifecycle:   entered-running-state, node-started
///
/// The "started" variant is a WebSocket handshake (not an F1r3flyEvent).
/// `Unknown` absorbs event types added to the node after this client.
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
        payload: BlockEventPayload,
    },
    // Transfer extraction (readonly only)
    TransfersAvailable {
        #[serde(rename = "schema-version")]
        schema_version: i32,
        payload: TransfersAvailablePayload,
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
        payload: BlockApprovalPayload,
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
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BlockEventPayload {
    pub block_hash: String,
    #[serde(default)]
    pub block_number: Option<i64>,
    #[serde(default)]
    pub timestamp: Option<i64>,
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
pub struct TransfersAvailablePayload {
    pub block_hash: String,
    pub block_number: i64,
    pub deploys: Vec<DeployTransfers>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DeployTransfers {
    pub deploy_id: String,
    pub transfers: Vec<TransferEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TransferEvent {
    pub from_addr: String,
    pub to_addr: String,
    /// Attempted amount; rejected transfers can be negative.
    #[serde(deserialize_with = "crate::vault::deserialize_reported_amount")]
    pub amount: num_bigint::BigInt,
    pub success: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BlockHashPayload {
    pub block_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BlockApprovalPayload {
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
    transfers: u32,
    genesis: u32,
    lifecycle: u32,
    unknown: u32,
    total: u32,
}

impl EventStats {
    fn new() -> Self {
        Self {
            created: 0,
            added: 0,
            finalized: 0,
            transfers: 0,
            genesis: 0,
            lifecycle: 0,
            unknown: 0,
            total: 0,
        }
    }

    fn increment(&mut self, event: &NodeEvent) {
        self.total += 1;
        match event {
            NodeEvent::BlockCreated { .. } => self.created += 1,
            NodeEvent::BlockAdded { .. } => self.added += 1,
            NodeEvent::BlockFinalised { .. } => self.finalized += 1,
            NodeEvent::TransfersAvailable { .. } => self.transfers += 1,
            NodeEvent::SentUnapprovedBlock { .. }
            | NodeEvent::SentApprovedBlock { .. }
            | NodeEvent::BlockApprovalReceived { .. }
            | NodeEvent::ApprovedBlockReceived { .. } => self.genesis += 1,
            NodeEvent::EnteredRunningState { .. } | NodeEvent::NodeStarted { .. } => {
                self.lifecycle += 1
            }
            NodeEvent::Unknown => self.unknown += 1,
            NodeEvent::Started { .. } => {}
        }
    }

    fn print_summary(&self, duration: std::time::Duration) {
        println!("\n Event Statistics:");
        println!(" Total Events: {}", self.total);
        println!(" - Created:    {}", self.created);
        println!(" - Added:      {}", self.added);
        println!(" - Finalized:  {}", self.finalized);
        if self.transfers > 0 {
            println!(" - Transfers:  {}", self.transfers);
        }
        if self.genesis > 0 {
            println!(" - Genesis:    {}", self.genesis);
        }
        if self.lifecycle > 0 {
            println!(" - Lifecycle:  {}", self.lifecycle);
        }
        if self.unknown > 0 {
            println!(" - Unknown:    {}", self.unknown);
        }
        println!(" Duration:     {:.1}s", duration.as_secs_f64());
        if duration.as_secs() > 0 {
            let rate = self.total as f64 / duration.as_secs_f64();
            println!(" Rate:         {rate:.2} events/sec");
        }
    }
}

/// Watch blocks command - connects to WebSocket and streams block events
pub async fn watch_events_command(args: &WatchEventsArgs) -> Result<()> {
    const VALID_FILTERS: &[&str] = &[
        "created",
        "added",
        "finalized",
        "finalised",
        "transfers",
        "genesis",
        "lifecycle",
    ];
    if let Some(filter) = &args.filter {
        if !VALID_FILTERS.contains(&filter.as_str()) {
            return Err(NodeCliError::from(format!(
                "Invalid --filter value '{}'. Valid values: {}",
                filter,
                VALID_FILTERS.join(", ")
            )));
        }
    }

    let ws_url = format!("ws://{}:{}/ws/events", args.host, args.http_port);

    println!(" Connecting to F1r3fly node WebSocket...");
    println!(" URL: {ws_url}");

    if let Some(filter) = &args.filter {
        println!(" Filter: {filter}");
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
                    println!(" Max reconnection attempts ({MAX_RETRIES}) reached");
                    return Err(e);
                }

                println!(" Connection lost: {e}");

                if args.retry_forever {
                    println!(
                        " Reconnecting in {RETRY_DELAY_SECS} seconds... (attempt {retry_count})"
                    );
                } else {
                    println!(
                        " Reconnecting in {RETRY_DELAY_SECS} seconds... (attempt {retry_count}/{MAX_RETRIES})"
                    );
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
                println!(" Reconnecting to {ws_url}...");
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
        NodeCliError::network_connection_failed(&format!("WebSocket connection failed: {e}"))
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
        eprintln!(" Error processing event: {e}");
        continue;
        }
        }
        Some(Ok(Message::Close(_))) => {
        return Err(NodeCliError::network_connection_failed("WebSocket closed by server"));
        }
        Some(Err(e)) => {
        return Err(NodeCliError::network_connection_failed(&format!("WebSocket error: {e}")));
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
    let frame: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| NodeCliError::from(format!("Failed to parse event: {e}")))?;
    let name = frame
        .get("event")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| NodeCliError::from(format!("Event has no \"event\" tag: {text}")))?
        .to_owned();
    let event: NodeEvent = serde_json::from_value(frame)
        .map_err(|e| NodeCliError::from(format!("Failed to parse {name} event: {e}")))?;

    if let Some(filter) = &args.filter {
        let matches = matches!(
            (&event, filter.as_str()),
            (NodeEvent::BlockCreated { .. }, "created")
                | (NodeEvent::BlockAdded { .. }, "added")
                | (NodeEvent::BlockFinalised { .. }, "finalized" | "finalised")
                | (NodeEvent::TransfersAvailable { .. }, "transfers")
                | (NodeEvent::SentUnapprovedBlock { .. }, "genesis")
                | (NodeEvent::SentApprovedBlock { .. }, "genesis")
                | (NodeEvent::BlockApprovalReceived { .. }, "genesis")
                | (NodeEvent::ApprovedBlockReceived { .. }, "genesis")
                | (NodeEvent::EnteredRunningState { .. }, "lifecycle")
                | (NodeEvent::NodeStarted { .. }, "lifecycle")
        );

        if !matches {
            return Ok(());
        }
    }

    stats.increment(&event);
    display_pretty(&event, &name);
    Ok(())
}

fn display_pretty(event: &NodeEvent, name: &str) {
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
            display_block_payload(payload);
        }
        NodeEvent::TransfersAvailable { payload, .. } => {
            println!(" Transfers Available");
            println!(
                " Block:    {} (#{}))",
                payload.block_hash, payload.block_number
            );
            println!(" Deploys:  {}", payload.deploys.len());
            for dt in &payload.deploys {
                println!(
                    "   Deploy: {}  ({} transfers)",
                    &dt.deploy_id[..24.min(dt.deploy_id.len())],
                    dt.transfers.len()
                );
                for t in &dt.transfers {
                    println!(
                        "     {} -> {} : {} ({})",
                        &t.from_addr[..16.min(t.from_addr.len())],
                        &t.to_addr[..16.min(t.to_addr.len())],
                        t.amount,
                        if t.success { "ok" } else { "failed" },
                    );
                }
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
        NodeEvent::Unknown => {
            println!(" Unknown event type: {name}");
            println!();
        }
    }
}

fn display_block_payload(payload: &BlockEventPayload) {
    println!(" Hash:     {}", payload.block_hash);
    println!(
        " Block #:  {}",
        payload
            .block_number
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        " Time:     {}",
        payload
            .timestamp
            .map(|t| t.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(" Creator:  {}", payload.creator);
    println!(" Seq Num:  {}", payload.seq_num);
    println!(" Parents:  {}", payload.parent_hashes.len());
    if !payload.deploys.is_empty() {
        println!(
            " Deploys:  {} [{}]",
            payload.deploys.len(),
            payload
                .deploys
                .iter()
                .map(|d| d.id.clone())
                .collect::<Vec<_>>()
                .join(", ")
        );
    } else {
        println!(" Deploys:  {}", payload.deploys.len());
    }
    println!();
}

#[cfg(test)]
mod bigint_tests {
    use super::*;

    #[test]
    fn failed_transfer_keeps_the_whole_notification_readable() {
        let text = serde_json::json!({
            "event": "transfers-available",
            "schema-version": 1,
            "payload": {
                "block-hash": "test-block",
                "block-number": 42,
                "deploys": [{
                    "deploy-id": "test-deploy",
                    "transfers": [
                        {"from-addr": "sender", "to-addr": "recipient", "amount": 1000, "success": true},
                        {"from-addr": "sender", "to-addr": "recipient", "amount": -1, "success": false},
                        {"from-addr": "sender", "to-addr": "recipient", "amount": "18446744073709551617", "success": true}
                    ]
                }]
            }
        })
        .to_string();
        let event: NodeEvent = serde_json::from_str(&text).unwrap();
        let NodeEvent::TransfersAvailable { payload, .. } = event else {
            panic!("expected a transfer notification");
        };
        let transfers = &payload.deploys[0].transfers;
        let amounts: Vec<_> = transfers.iter().map(|t| t.amount.to_string()).collect();
        assert_eq!(amounts, ["1000", "-1", "18446744073709551617"]);
        assert_eq!(
            transfers.iter().map(|t| t.success).collect::<Vec<_>>(),
            [true, false, true]
        );

        let args = WatchEventsArgs {
            host: "localhost".into(),
            http_port: 40403,
            filter: Some("transfers".into()),
            retry_forever: false,
        };
        let mut stats = EventStats::new();
        handle_event(&text, &args, &mut stats).unwrap();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.transfers, 1);
    }

    #[test]
    fn reads_legacy_and_bigint_transfer_events() {
        for (amount, success) in [
            (serde_json::json!(1000), true),
            (serde_json::json!(-1), false),
            (
                serde_json::json!("1606938044258990275541962092341162602522202993782792835301377"),
                true,
            ),
            (
                serde_json::json!("-1606938044258990275541962092341162602522202993782792835301377"),
                false,
            ),
        ] {
            let event: TransferEvent = serde_json::from_value(serde_json::json!({
                "from-addr": "sender", "to-addr": "recipient", "amount": amount, "success": success
            }))
            .unwrap();
            let expected = amount
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| amount.to_string());
            assert_eq!(event.amount.to_string(), expected);
            assert_eq!(event.success, success);
        }
    }
}

#[cfg(test)]
mod event_type_tests {
    use super::*;

    fn watch(filter: Option<&str>) -> WatchEventsArgs {
        WatchEventsArgs {
            host: "localhost".into(),
            http_port: 40403,
            filter: filter.map(Into::into),
            retry_forever: false,
        }
    }

    #[test]
    fn block_approval_received_counts_toward_the_genesis_filter() {
        let text = serde_json::json!({
            "event": "block-approval-received",
            "schema-version": 1,
            "payload": {"block-hash": "test-block", "sender": "test-sender"}
        })
        .to_string();
        let mut stats = EventStats::new();
        handle_event(&text, &watch(Some("genesis")), &mut stats).unwrap();
        assert_eq!(stats.genesis, 1);
        assert_eq!(stats.total, 1);
    }

    #[test]
    fn an_unknown_event_type_is_counted_not_rejected() {
        let text = serde_json::json!({
            "event": "some-future-event",
            "schema-version": 1,
            "payload": {"anything": true}
        })
        .to_string();
        let mut stats = EventStats::new();
        handle_event(&text, &watch(None), &mut stats).unwrap();
        assert_eq!(stats.unknown, 1);
        assert_eq!(stats.total, 1);

        let mut filtered = EventStats::new();
        handle_event(&text, &watch(Some("genesis")), &mut filtered).unwrap();
        assert_eq!(filtered.total, 0);
    }

    #[test]
    fn a_malformed_known_event_is_still_an_error() {
        let text = serde_json::json!({
            "event": "block-approval-received",
            "schema-version": 1,
            "payload": {"block-hash": "test-block"}
        })
        .to_string();
        let error = handle_event(&text, &watch(None), &mut EventStats::new()).unwrap_err();
        assert!(
            error.to_string().contains("block-approval-received"),
            "{error}"
        );
    }
}
