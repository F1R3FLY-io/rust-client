//! The shard's nodes, and the host ports each one answers on.

use std::collections::BTreeMap;
use std::sync::RwLock;

use super::rt;

/// The node's internal gRPC API (deploys, proposals) inside its container.
pub const GRPC_PORT: u16 = 40402;
/// The node's HTTP API inside its container.
pub const HTTP_PORT: u16 = 40403;

/// Host ports Docker assigned to published container ports, by compose service
/// and container port. `topology/compose.yml` publishes no fixed host port, so
/// none can be taken by another process before the shard binds it.
static PUBLISHED: RwLock<BTreeMap<(&str, u16), u16>> = RwLock::new(BTreeMap::new());

/// Record that `service`'s `container_port` is published on `host_port`.
pub fn record_published(service: &'static str, container_port: u16, host_port: u16) {
    PUBLISHED
        .write()
        .expect("the published-port map is never poisoned")
        .insert((service, container_port), host_port);
}

/// The host port `service`'s `container_port` is published on.
pub fn published(service: &str, container_port: u16) -> u16 {
    *PUBLISHED
        .read()
        .expect("the published-port map is never poisoned")
        .get(&(service, container_port))
        .unwrap_or_else(|| {
            panic!("{service}:{container_port} is not published: is the service running?")
        })
}

/// The shard's nodes, by compose service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    /// Ceremony master, not a validator. Deploys sent here are never included.
    Boot,
    Validator1,
    Validator2,
    Validator3,
    /// The only node serving exploratory deploys.
    ReadOnly,
    /// Unbonded at genesis, started by the bonding tests.
    Validator4,
}

impl Node {
    pub fn all() -> &'static [Node] {
        &[
            Node::Boot,
            Node::Validator1,
            Node::Validator2,
            Node::Validator3,
            Node::ReadOnly,
        ]
    }

    pub fn container(self) -> &'static str {
        match self {
            Node::Boot => "it-boot",
            Node::Validator1 => "it-validator1",
            Node::Validator2 => "it-validator2",
            Node::Validator3 => "it-validator3",
            Node::ReadOnly => "it-readonly",
            Node::Validator4 => "it-validator4",
        }
    }

    /// The node's service in `topology/compose.yml`.
    pub fn service(self) -> &'static str {
        match self {
            Node::Boot => "boot",
            Node::Validator1 => "validator1",
            Node::Validator2 => "validator2",
            Node::Validator3 => "validator3",
            Node::ReadOnly => "readonly",
            Node::Validator4 => "validator4",
        }
    }

    pub fn grpc_port(self) -> u16 {
        published(self.service(), GRPC_PORT)
    }

    pub fn http_port(self) -> u16 {
        published(self.service(), HTTP_PORT)
    }

    pub fn grpc_endpoint(self) -> String {
        format!("http://localhost:{}", self.grpc_port())
    }

    pub fn http_endpoint(self) -> String {
        format!("http://localhost:{}", self.http_port())
    }

    pub fn http_url(self, path: &str) -> String {
        format!("{}{path}", self.http_endpoint())
    }

    /// The node's last finalized block number, or `None` while it is not
    /// answering yet.
    pub fn finalized_height(self) -> Option<i64> {
        self.status()?.get("lastFinalizedBlockNumber")?.as_i64()
    }

    pub fn status(self) -> Option<serde_json::Value> {
        get_json(&self.http_url("/api/status"))
    }
}

/// A GET returning parsed JSON, or `None` for any failure. Callers poll, so a
/// transient failure is not worth distinguishing from "not yet".
pub fn get_json(url: &str) -> Option<serde_json::Value> {
    rt().block_on(async {
        reqwest::Client::new()
            .get(url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()
    })
}
