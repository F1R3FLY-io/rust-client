//! Waiting on chain state. Nothing in the suite sleeps for a fixed duration:
//! every wait names the condition it is waiting for and fails with it.

use std::time::{Duration, Instant};

use super::cli::Output;
use super::node::Node;

/// Poll `condition` until it holds, failing after `budget` with `what`.
pub fn until(what: &str, budget: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + budget;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Block until the node finalizes `blocks` further blocks.
pub fn advance(node: Node, blocks: i64, budget: Duration) {
    let start = finalized_height(node);
    until(
        &format!("{} to finalize {blocks} more blocks", node.container()),
        budget,
        || finalized_height(node) >= start + blocks,
    );
}

/// Block until the node passes the next epoch boundary, which is when the PoS
/// contract applies bonds, withdrawals and payouts.
pub fn next_epoch_boundary(node: Node, epoch_length: i64, budget: Duration) {
    let height = finalized_height(node);
    let boundary = epoch_length * (1 + height / epoch_length);
    until(
        &format!("{} to finalize past block {boundary}", node.container()),
        budget,
        || finalized_height(node) > boundary,
    );
}

/// Block until `node` itself gives the deploy a terminal state, and return it.
///
/// A client's verdict comes from whichever node answers first, so another
/// node's finalized state need not include the deploy yet. A node settles a
/// verdict from its own last finalized block, so once it answers, reads of
/// its finalized state include the deploy.
pub fn settled_on(node: Node, deploy_id: &str, budget: Duration) -> String {
    let url = node.http_url(&format!("/api/deploy-finalization-status/{deploy_id}"));
    let mut state = String::new();
    until(
        &format!("{} to settle deploy {deploy_id}", node.container()),
        budget,
        || {
            state = super::node::get_json(&url)
                .and_then(|status| Some(status.get("state")?.as_str()?.to_owned()))
                .unwrap_or_default();
            matches!(state.as_str(), "Finalized" | "Failed" | "Expired")
        },
    );
    state
}

/// Block until the read-only node, which the suite's assertions read, has
/// finalized the deploy a command reported.
pub fn finalized_on_read_only(output: &Output) {
    let state = settled_on(
        Node::ReadOnly,
        output.value_after("Deploy ID:"),
        Duration::from_secs(60),
    );
    assert_eq!(state, "Finalized", "the read-only node's verdict");
}

pub fn finalized_height(node: Node) -> i64 {
    node.finalized_height().unwrap_or(-1)
}
