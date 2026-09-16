//! Group D - what the client reports about the bond set, through a real
//! joiner's whole lifecycle.
//!
//! Serial: every test here moves shard-wide state. The PoS contract's own
//! semantics belong to the node's suites; these assert what the client says
//! about them.

use std::time::Duration;

use libtest_mimic::Trial;

use crate::common::chain;
use crate::common::cli::{Cli, Output};
use crate::common::keys;
use crate::common::node::Node;

/// Matches `epoch-length` in `topology/node.conf`. The PoS contract applies
/// bonds, withdrawals and payouts only at a boundary.
const EPOCH_LENGTH: i64 = 4;

const STAKE: &str = "100";

pub fn trials() -> Vec<Trial> {
    vec![
        Trial::test("pos::genesis_reports_three_bonded_validators", || {
            genesis_reports_three_bonded_validators();
            Ok(())
        }),
        Trial::test("pos::a_joiner_bonds_and_becomes_active", || {
            a_joiner_bonds_and_becomes_active();
            Ok(())
        }),
        Trial::test("pos::unbonding_quarantines_the_stake", || {
            unbonding_quarantines_the_stake();
            Ok(())
        }),
    ]
}

/// The genesis bond set, as the client reports it from a validator and from
/// the observer.
fn genesis_reports_three_bonded_validators() {
    // The client abbreviates keys as <first 8>...<last 8>.
    let validator1 = format!(
        "{}...{}",
        &keys::VALIDATOR1_PUBLIC[..8],
        &keys::VALIDATOR1_PUBLIC[keys::VALIDATOR1_PUBLIC.len() - 8..]
    );

    bonds()
        .expect_success("bonds")
        .expect_contains("3 total")
        .expect_contains(&validator1);

    Cli::new("active-validators")
        .http_at(Node::ReadOnly)
        .run()
        .expect_success("active-validators")
        .expect_contains("3 total")
        .expect_contains(&validator1);

    validator_status(keys::VALIDATOR1_PUBLIC)
        .expect_success("validator-status for a genesis validator")
        .expect_contains("BONDED");

    validator_status(keys::NEVER_BONDED_PUBLIC)
        .expect_success("validator-status for an unbonded key")
        .expect_contains("NOT BONDED");
}

/// Start the joiner, bond it, and watch it enter the active set at the next
/// epoch boundary. A second bond of the same key must be refused.
fn a_joiner_bonds_and_becomes_active() {
    bond(&keys::VALIDATOR4)
        .expect_success("bond-validator")
        .expect_contains("Bond");

    bonds()
        .expect_success("bonds")
        .expect_contains("4 total");

    bond(&keys::VALIDATOR4)
        .expect_failure("a second bond of the same key")
        .expect_contains("already bonded");

    chain::next_epoch_boundary(Node::ReadOnly, EPOCH_LENGTH, Duration::from_secs(180));

    validator_status(keys::VALIDATOR4.public)
        .expect_success("validator-status for the joiner")
        .expect_contains("ACTIVE");
}

/// Unbonding takes the joiner out of the bond set and puts its stake into
/// quarantine; the chain must keep finalizing without it.
fn unbonding_quarantines_the_stake() {
    unbond(&keys::VALIDATOR4).expect_success("unbond-validator");

    validator_status(keys::VALIDATOR4.public)
        .expect_success("validator-status after unbonding")
        .expect_contains("WITHDRAW");

    chain::next_epoch_boundary(Node::ReadOnly, EPOCH_LENGTH, Duration::from_secs(180));

    bonds()
        .expect_success("bonds")
        .expect_contains("3 total");

    // The shard must still make progress with the joiner gone.
    chain::advance(Node::ReadOnly, 2, Duration::from_secs(180));

    unbond(&keys::VALIDATOR4).expect_failure("a second unbond of the same key");
}

fn bonds() -> Output {
    Cli::new("bonds").http_at(Node::ReadOnly).run()
}

fn validator_status(public_key: &str) -> Output {
    Cli::new("validator-status")
        .args(["--public-key", public_key])
        .http_port_at(Node::ReadOnly)
        .run()
}

fn bond(key: &keys::Key) -> Output {
    Cli::new("bond-validator")
        .args(["--stake", STAKE])
        .key(key)
        .at(Node::Validator1)
        .observing(Node::ReadOnly)
        .args(["--max-wait", "180"])
        .run()
}

fn unbond(key: &keys::Key) -> Output {
    Cli::new("unbond-validator")
        .key(key)
        .at(Node::Validator1)
        .observing(Node::ReadOnly)
        .args(["--max-wait", "180"])
        .run()
}
