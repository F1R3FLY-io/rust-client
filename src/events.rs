//! WebSocket event streaming for real-time deploy finalization
//!
//! Connects to the node's `/ws/events` endpoint and provides
//! deploy finalization notifications without polling.
//!
//! Uses `f1r3fly_shared::F1r3flyEvent` for type-safe event deserialization,
//! matching the node's event format exactly.

use f1r3fly_shared::rust::shared::f1r3fly_event::{DeployEvent as NodeDeployEvent, F1r3flyEvent};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use tokio_tungstenite::tungstenite::Message;

/// A deploy finalization event from the node
#[derive(Debug, Clone)]
pub struct DeployEvent {
    pub deploy_id: String,
    pub cost: u64,
    pub errored: bool,
    pub deployer: String,
}

impl From<NodeDeployEvent> for DeployEvent {
    fn from(e: NodeDeployEvent) -> Self {
        Self {
            deploy_id: e.id,
            cost: e.cost as u64,
            errored: e.errored,
            deployer: e.deployer,
        }
    }
}

/// WebSocket event listener for deploy finalization
///
/// Connects to a node's `/ws/events` endpoint and tracks
/// `block-finalised` events. Callers use `wait_for_deploy`
/// to wait for a specific deploy to be finalized.
#[derive(Clone)]
pub struct NodeEvents {
    pending: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    results: Arc<Mutex<HashMap<String, DeployEvent>>>,
}

impl NodeEvents {
    /// Connect to a node's WebSocket event stream
    ///
    /// `ws_url` should be the base WebSocket URL, e.g. `ws://localhost:40403`
    pub fn connect(ws_url: &str) -> Self {
        let url = format!("{}/ws/events", ws_url);
        let pending: Arc<Mutex<HashMap<String, Arc<Notify>>>> = Arc::default();
        let results: Arc<Mutex<HashMap<String, DeployEvent>>> = Arc::default();

        tokio::spawn({
            let pending = pending.clone();
            let results = results.clone();
            async move {
                loop {
                    match tokio_tungstenite::connect_async(&url).await {
                        Ok((mut stream, _)) => {
                            tracing::info!("WebSocket connected to {}", url);
                            while let Some(msg) = stream.next().await {
                                let text = match msg {
                                    Ok(Message::Text(t)) => t,
                                    Ok(_) => continue,
                                    Err(e) => {
                                        tracing::debug!("WebSocket error: {}", e);
                                        break;
                                    }
                                };

                                // The node sends events in an envelope:
                                // {"event": "block-finalised", "schema-version": 1, "payload": {...}}
                                // F1r3flyEvent expects internally-tagged format:
                                // {"event": "block-finalised", "block-hash": "...", "deploys": [...]}
                                // Unwrap: merge payload fields into top-level, remove envelope keys.
                                let mut envelope: serde_json::Value =
                                    match serde_json::from_str(&text) {
                                        Ok(v) => v,
                                        Err(_) => continue,
                                    };

                                if let Some(payload) = envelope.get("payload").cloned() {
                                    if let (Some(top), Some(inner)) =
                                        (envelope.as_object_mut(), payload.as_object())
                                    {
                                        for (k, v) in inner {
                                            top.insert(k.clone(), v.clone());
                                        }
                                        top.remove("payload");
                                        top.remove("schema-version");
                                    }
                                }

                                let event: F1r3flyEvent = match serde_json::from_value(envelope) {
                                    Ok(e) => e,
                                    Err(e) => {
                                        tracing::debug!("Failed to deserialize event: {}", e);
                                        continue;
                                    }
                                };

                                // Only process block-finalised events
                                let deploys = match event {
                                    F1r3flyEvent::BlockFinalised(b) => b.deploys,
                                    _ => continue,
                                };

                                let mut pending_guard = pending.lock().await;
                                let mut results_guard = results.lock().await;

                                for deploy in deploys {
                                    let id = deploy.id.clone();
                                    let event = DeployEvent::from(deploy);
                                    results_guard.insert(id.clone(), event);

                                    if let Some(notify) = pending_guard.remove(&id) {
                                        notify.notify_waiters();
                                    }
                                }
                            }
                            tracing::info!("WebSocket disconnected, reconnecting in 5s...");
                        }
                        Err(e) => {
                            tracing::warn!("WebSocket connect failed: {}, retrying in 5s", e);
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        });

        Self { pending, results }
    }

    /// Wait for a deploy to be finalized
    ///
    /// Returns `Some(DeployEvent)` if finalized within the timeout,
    /// `None` if the timeout expires.
    pub async fn wait_for_deploy(&self, deploy_id: &str, timeout: Duration) -> Option<DeployEvent> {
        // Check if already finalized
        {
            let results = self.results.lock().await;
            if let Some(event) = results.get(deploy_id) {
                return Some(event.clone());
            }
        }

        // Register for notification
        let notify = Arc::new(Notify::new());
        {
            let mut pending = self.pending.lock().await;
            pending.insert(deploy_id.to_string(), notify.clone());
        }

        // Wait for notification or timeout
        let result = tokio::select! {
        _ = notify.notified() => {
        let results = self.results.lock().await;
        results.get(deploy_id).cloned()
        }
        _ = tokio::time::sleep(timeout) => None,
        };

        // Clean up pending entry on timeout
        if result.is_none() {
            let mut pending = self.pending.lock().await;
            pending.remove(deploy_id);
        }

        result
    }

    /// Check if a deploy has been finalized (non-blocking)
    pub async fn is_finalized(&self, deploy_id: &str) -> Option<DeployEvent> {
        let results = self.results.lock().await;
        results.get(deploy_id).cloned()
    }
}
