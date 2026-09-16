//! Group E - the rest of the CLI, against the running shard.
//!
//! Each command's port flags differ: `-p` is the HTTP port for some and the
//! gRPC port for others, which is why the helper differs per test.

use libtest_mimic::Trial;

use crate::common::cli::Cli;
use crate::common::contracts;
use crate::common::keys;
use crate::common::node::Node;

pub fn trials() -> Vec<Trial> {
    vec![
        Trial::test("surface::status_describes_the_shard", || {
            Cli::new("status")
                .http_at(Node::ReadOnly)
                .run()
                .expect_success("status")
                .expect_contains("root");
            Ok(())
        }),
        Trial::test("surface::the_last_finalized_block_is_reported", || {
            Cli::new("last-finalized-block")
                .http_at(Node::ReadOnly)
                .run()
                .expect_success("last-finalized-block")
                .expect_contains("Block");
            Ok(())
        }),
        Trial::test("surface::blocks_lists_recent_blocks", || {
            Cli::new("blocks")
                .http_at(Node::Validator1)
                .run()
                .expect_success("blocks")
                .expect_contains("Block");
            Ok(())
        }),
        Trial::test("surface::the_main_chain_is_walkable", || {
            Cli::new("show-main-chain")
                .grpc_at(Node::Validator1)
                .run()
                .expect_success("show-main-chain")
                .expect_contains("Block");
            Ok(())
        }),
        Trial::test("surface::blocks_can_be_fetched_by_height", || {
            Cli::new("get-blocks-by-height")
                .args(["--start-block-number", "1"])
                .args(["--end-block-number", "3"])
                .grpc_at(Node::Validator1)
                .run()
                .expect_success("get-blocks-by-height")
                .expect_contains("Block");
            Ok(())
        }),
        Trial::test("surface::epoch_info_reports_the_configured_epoch", || {
            Cli::new("epoch-info")
                .at(Node::ReadOnly)
                .run()
                .expect_success("epoch-info")
                .expect_contains("4");
            Ok(())
        }),
        Trial::test("surface::epoch_rewards_are_readable", || {
            Cli::new("epoch-rewards")
                .at(Node::ReadOnly)
                .run()
                .expect_success("epoch-rewards");
            Ok(())
        }),
        Trial::test(
            "surface::a_term_is_costed_and_a_broken_one_is_refused",
            || {
                // Costing runs an exploratory deploy, so it needs the read-only
                // node, and an identity-dependent term needs the deployer.
                let estimate = Cli::new("estimate-cost")
                    .args(["-f", contracts::path("stdout.rho")])
                    .args(["--deployer", keys::ESTIMATE_COST.public])
                    .http_port_at(Node::ReadOnly)
                    .run()
                    .expect_success("estimate-cost");
                assert!(
                    estimate.text.trim().parse::<u64>().is_ok(),
                    "estimate-cost printed something other than a cost: {}",
                    estimate.text
                );

                Cli::new("estimate-cost")
                    .args(["-f", contracts::path("broken.rho")])
                    .args(["--deployer", keys::ESTIMATE_COST.public])
                    .http_port_at(Node::ReadOnly)
                    .run()
                    .expect_failure("estimate-cost on a term that does not parse")
                    .expect_contains("rholang_bad_term");
                Ok(())
            },
        ),
        Trial::test("surface::network_health_reports_the_shard", || {
            Cli::new("network-health")
                .args(["-H", "localhost"])
                .run()
                .expect_success("network-health");
            Ok(())
        }),
        Trial::test("surface::metrics_are_exposed", || {
            Cli::new("metrics")
                .http_at(Node::ReadOnly)
                .run()
                .expect_success("metrics");
            Ok(())
        }),
    ]
}
