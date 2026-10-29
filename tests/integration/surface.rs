//! Group E - the rest of the CLI, against the running shard.

use libtest_mimic::Trial;
use node_cli::{Endpoint, F1r3flyApi, NodeHttp};

use crate::common::cli::Cli;
use crate::common::contracts;
use crate::common::keys;
use crate::common::node::Node;
use crate::common::rt;

pub fn trials() -> Vec<Trial> {
    vec![
        Trial::test("surface::bigint_query_preserves_values_above_u64", || {
            Cli::new("exploratory-deploy")
                .args(["-f", contracts::path("bigint.rho")])
                .grpc_at(Node::ReadOnly)
                .run()
                .expect_success("BigInt exploratory-deploy")
                .expect_contains("(18446744073709551617n, -18446744073709551617n)");
            Ok(())
        }),
        Trial::test("surface::grpc_and_http_return_the_same_values", || {
            grpc_and_http_return_the_same_values();
            Ok(())
        }),
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
                .grpc_at(Node::ReadOnly)
                .run()
                .expect_success("epoch-info")
                .expect_contains("4");
            Ok(())
        }),
        Trial::test("surface::epoch_rewards_are_readable", || {
            Cli::new("epoch-rewards")
                .http_at(Node::ReadOnly)
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
                    .args(["--deployer", &keys::ESTIMATE_COST.public])
                    .http_at(Node::ReadOnly)
                    .run()
                    .expect_success("estimate-cost");
                assert!(
                    estimate.text.trim().parse::<u64>().is_ok(),
                    "estimate-cost printed something other than a cost: {}",
                    estimate.text
                );

                Cli::new("estimate-cost")
                    .args(["-f", contracts::path("broken.rho")])
                    .args(["--deployer", &keys::ESTIMATE_COST.public])
                    .http_at(Node::ReadOnly)
                    .run()
                    .expect_failure("estimate-cost on a term that does not parse")
                    .expect_contains("rholang_bad_term");
                Ok(())
            },
        ),
        Trial::test("surface::network_health_reports_the_shard", || {
            Cli::new("network-health")
                .run()
                .expect_success("network-health")
                .expect_contains("Healthy nodes: 5/5");
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

/// The node encodes a query's result differently on each transport, and the
/// client decodes each separately; both must give the value the term returned.
fn grpc_and_http_return_the_same_values() {
    let endpoint = |url: String| Endpoint::parse(&url).expect("a shard endpoint");
    let grpc = F1r3flyApi::new(endpoint(Node::ReadOnly.grpc_endpoint())).expect("gRPC client");
    let http = NodeHttp::new(endpoint(Node::ReadOnly.http_endpoint())).expect("HTTP client");

    let over_grpc = rt()
        .block_on(grpc.exploratory_deploy(contracts::VALUES_TERM, None, false))
        .expect("exploratory deploy over gRPC")
        .value()
        .expect("one value over gRPC");
    let over_http = rt()
        .block_on(http.explore_deploy(contracts::VALUES_TERM))
        .expect("exploratory deploy over HTTP")
        .value()
        .expect("one value over HTTP")
        .clone();

    assert_eq!(over_grpc, over_http);
    assert_eq!(
        over_grpc.to_string(),
        r#"(-1, 2n, 3u64, -4i32, 5u8, 1.5f64, 7/1r, 12.50p2, "s", `rho:id:x`, [true, false], Set(1), {"k": 7u16})"#
    );
}
