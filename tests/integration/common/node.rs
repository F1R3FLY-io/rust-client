//! The shard's nodes, and the host ports each one answers on.

use super::rt;

/// Ports as published by `topology/compose.yml`.
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

    /// First published port of the node's block; the node's own offsets give
    /// the rest, so the whole map is one number per node.
    fn base_port(self) -> u16 {
        match self {
            Node::Boot => 40400,
            Node::Validator1 => 40410,
            Node::Validator2 => 40420,
            Node::Validator3 => 40430,
            Node::Validator4 => 40440,
            Node::ReadOnly => 40450,
        }
    }

    pub fn grpc_port(self) -> u16 {
        self.base_port() + 2
    }

    pub fn http_port(self) -> u16 {
        self.base_port() + 3
    }

    pub fn http_url(self, path: &str) -> String {
        format!("http://localhost:{}{path}", self.http_port())
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
