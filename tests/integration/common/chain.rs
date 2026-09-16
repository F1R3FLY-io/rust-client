//! Waiting on chain state. Nothing in the suite sleeps for a fixed duration:
//! every wait names the condition it is waiting for and fails with it.

use std::time::{Duration, Instant};

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

pub fn finalized_height(node: Node) -> i64 {
    node.finalized_height().unwrap_or(-1)
}
