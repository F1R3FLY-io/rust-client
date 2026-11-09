//! Group C - moving tokens between vaults.

use libtest_mimic::Trial;
use num_bigint::BigUint;

use crate::common::chain;
use crate::common::cli::Cli;
use crate::common::keys::{self, Key};
use crate::common::node::Node;

pub fn trials() -> Vec<Trial> {
    vec![
        Trial::test("transfers::wide_amount_and_fee_are_exact", || {
            wide_amount_and_fee_are_exact();
            Ok(())
        }),
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
        Trial::test("transfers::a_balance_is_the_exact_genesis_amount", || {
            assert_eq!(
                balance(&keys::IDLE_RECIPIENT.address),
                keys::FUNDED.parse::<BigUint>().expect("a genesis amount")
            );
            Ok(())
        }),
        Trial::test(
            "transfers::a_vault_the_node_cannot_look_up_is_an_error",
            || {
                a_vault_the_node_cannot_look_up_is_an_error();
                Ok(())
            },
        ),
    ]
}

/// An address with a broken checksum passes the client's format check and
/// fails the vault's lookup, which must surface as an error, never a number.
fn a_vault_the_node_cannot_look_up_is_an_error() {
    let address = &keys::IDLE_RECIPIENT.address;
    let (body, last) = address.split_at(address.len() - 1);
    let corrupted = format!("{body}{}", if last == "2" { "3" } else { "2" });

    Cli::new("wallet-balance")
        .args(["--address", &corrupted])
        .http_at(Node::ReadOnly)
        .run()
        .expect_failure("wallet-balance of an address the vault rejects")
        .expect_not_contains("Balance for");
}

const AMOUNT: u64 = 1_000_000;

fn a_transfer_moves_exactly_the_amount() {
    let before = balance(&keys::TRANSFER_RECIPIENT.address);

    let sent = transfer(
        &keys::TRANSFER_SENDER,
        &keys::TRANSFER_RECIPIENT.address,
        AMOUNT,
    )
    .expect_success("transfer")
    .expect_contains("Transfer complete");
    chain::finalized_on_read_only(&sent);

    let after = balance(&keys::TRANSFER_RECIPIENT.address);
    assert_eq!(
        &after - &before,
        BigUint::from(AMOUNT),
        "recipient balance moved by the wrong amount ({before} then {after})"
    );
}

/// A vault rejection is a successful deploy carrying a failure, so the client
/// has to read the result rather than trust the exit of the deploy itself.
fn an_overdraft_fails_and_moves_nothing() {
    let sender = balance(&keys::OVERDRAFT_TRANSFER.address);
    let recipient = balance(&keys::IDLE_RECIPIENT.address);

    let refused = transfer(
        &keys::OVERDRAFT_TRANSFER,
        &keys::IDLE_RECIPIENT.address,
        u64::from(u32::MAX) * 1_000_000,
    )
    .expect_failure("an overdrawing transfer")
    .expect_contains("Insufficient funds");
    // Without this, a balance read before the deploy settles would pass even
    // if the transfer had moved tokens.
    chain::finalized_on_read_only(&refused);

    assert_eq!(
        balance(&keys::IDLE_RECIPIENT.address),
        recipient,
        "a rejected transfer moved tokens"
    );
    assert!(
        balance(&keys::OVERDRAFT_TRANSFER.address) <= sender,
        "a rejected transfer credited the sender"
    );
}

/// The read-only node reports who paid whom, which is what a client watching
/// the chain reads back.
fn the_block_report_describes_the_transfer() {
    let sent =
        transfer(&keys::REPORT_SENDER, &keys::SPARE.address, AMOUNT).expect_success("transfer");
    chain::finalized_on_read_only(&sent);
    let block = sent.value_after("Block hash:").to_string();

    Cli::new("block-transfers")
        .arg(&block)
        .http_at(Node::ReadOnly)
        .run()
        .expect_success("block-transfers")
        .expect_contains(&keys::REPORT_SENDER.address)
        .expect_contains(&keys::SPARE.address)
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
fn balance(address: &str) -> BigUint {
    let output = Cli::new("wallet-balance")
        .args(["--address", address])
        .http_at(Node::ReadOnly)
        .run()
        .expect_success("wallet-balance");

    let label = format!("Balance for {address}:");
    output
        .value_after(&label)
        .parse()
        .unwrap_or_else(|e| panic!("balance is not a number ({e}):\n{}", output.text))
}

/// Exercise values beyond u128 through signing, execution, refunding and reads.
fn wide_amount_and_fee_are_exact() {
    use crate::common::node::get_json;
    use node_cli::f1r3fly_api::{DeployDetail, NodeStatus};

    let status: NodeStatus = serde_json::from_value(
        get_json(&Node::ReadOnly.http_url("/api/status")).expect("status response"),
    )
    .unwrap();
    assert_eq!(status.native_token_decimals, Some(18));
    assert_eq!(status.min_phlo_price, BigUint::from(1u8));
    let amount = BigUint::from(1u8) << 150usize;
    let price = BigUint::from(1u8) << 128usize;
    let sender_before = balance(&keys::WIDE_SENDER.address);
    let recipient_before = balance(&keys::WIDE_RECIPIENT.address);
    let sent = Cli::new("transfer")
        .args(["--to-address", &keys::WIDE_RECIPIENT.address])
        .arg("--whole-tokens")
        .args([
            "--amount",
            &node_cli::vault::dust_to_tokens(&amount, 18).unwrap(),
        ])
        .args(["--phlo-price", &price.to_string()])
        .key(&keys::WIDE_SENDER)
        .at(Node::Validator1)
        .observing(Node::ReadOnly)
        .args(["--max-wait", "120"])
        .run()
        .expect_success("wide transfer")
        .expect_contains("Transfer complete");
    chain::finalized_on_read_only(&sent);
    let detail: DeployDetail = serde_json::from_value(
        get_json(
            &Node::ReadOnly.http_url(&format!("/api/deploy/{}", sent.value_after("Deploy ID:"))),
        )
        .expect("deploy detail"),
    )
    .unwrap();
    assert_eq!(detail.phlo_price, Some(price.clone()));
    let limit = u64::try_from(detail.phlo_limit.expect("signed limit")).unwrap();
    assert!(
        detail.cost < limit,
        "the deploy must have unused phlo to test refunding"
    );
    let charge = &price * detail.cost;
    assert_eq!(
        balance(&keys::WIDE_RECIPIENT.address) - recipient_before,
        amount
    );
    // Unused phlo is not charged: the finalized balance delta includes only actual cost.
    assert_eq!(
        sender_before - balance(&keys::WIDE_SENDER.address),
        &amount + &charge
    );
    Cli::new("block-transfers")
        .arg(&detail.block_hash)
        .http_at(Node::ReadOnly)
        .run()
        .expect_success("wide transfer report")
        .expect_contains(&amount.to_string());
}
