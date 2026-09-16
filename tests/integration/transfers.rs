//! Group C - moving tokens between vaults.

use libtest_mimic::Trial;

use crate::common::cli::Cli;
use crate::common::keys::{self, Key};
use crate::common::node::Node;

pub fn trials() -> Vec<Trial> {
    vec![
        Trial::test("transfers::a_transfer_moves_exactly_the_amount", || {
            a_transfer_moves_exactly_the_amount();
            Ok(())
        }),
        Trial::test("transfers::an_overdraft_fails_and_moves_nothing", || {
            an_overdraft_fails_and_moves_nothing();
            Ok(())
        }),
        Trial::test("transfers::the_block_report_describes_the_transfer", || {
            the_block_report_describes_the_transfer();
            Ok(())
        }),
    ]
}

const AMOUNT: u64 = 1_000_000;

fn a_transfer_moves_exactly_the_amount() {
    let before = balance(keys::TRANSFER_RECIPIENT.address);

    transfer(&keys::TRANSFER_SENDER, keys::TRANSFER_RECIPIENT.address, AMOUNT)
        .expect_success("transfer")
        .expect_contains("Transfer complete");

    let after = balance(keys::TRANSFER_RECIPIENT.address);
    assert_eq!(
        after - before,
        AMOUNT as i128,
        "recipient balance moved by the wrong amount ({before} then {after})"
    );
}

/// A vault rejection is a successful deploy carrying a failure, so the client
/// has to read the result rather than trust the exit of the deploy itself.
fn an_overdraft_fails_and_moves_nothing() {
    let sender = balance(keys::OVERDRAFT_TRANSFER.address);
    let recipient = balance(keys::IDLE_RECIPIENT.address);

    transfer(
        &keys::OVERDRAFT_TRANSFER,
        keys::IDLE_RECIPIENT.address,
        u64::from(u32::MAX) * 1_000_000,
    )
    .expect_failure("an overdrawing transfer")
    .expect_contains("Insufficient funds");

    assert_eq!(
        balance(keys::IDLE_RECIPIENT.address),
        recipient,
        "a rejected transfer moved tokens"
    );
    assert!(
        balance(keys::OVERDRAFT_TRANSFER.address) <= sender,
        "a rejected transfer credited the sender"
    );
}

/// The read-only node reports who paid whom, which is what a client watching
/// the chain reads back.
fn the_block_report_describes_the_transfer() {
    let sent = transfer(&keys::REPORT_SENDER, keys::SPARE.address, AMOUNT)
        .expect_success("transfer");
    let block = sent.value_after("Block hash:").to_string();

    Cli::new("block-transfers")
        .arg(&block)
        .http_at(Node::ReadOnly)
        .run()
        .expect_success("block-transfers")
        .expect_contains(keys::REPORT_SENDER.address)
        .expect_contains(keys::SPARE.address)
        .expect_contains(&AMOUNT.to_string());
}

fn transfer(from: &Key, to: &str, amount: u64) -> crate::common::cli::Output {
    Cli::new("transfer")
        .args(["--to-address", to])
        .args(["--amount", &amount.to_string()])
        .key(from)
        .at(Node::Validator1)
        .observing(Node::ReadOnly)
        .args(["--max-wait", "120"])
        .run()
}

/// The vault balance of an address, read through the read-only node.
fn balance(address: &str) -> i128 {
    let output = Cli::new("wallet-balance")
        .args(["--address", address])
        .grpc_at(Node::ReadOnly)
        .run()
        .expect_success("wallet-balance");

    let label = format!("Balance for {address}:");
    output
        .value_after(&label)
        .parse()
        .unwrap_or_else(|e| panic!("balance is not a number ({e}):\n{}", output.text))
}
