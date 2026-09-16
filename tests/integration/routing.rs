//! Group A - which node a command talks to, and on which port.

use libtest_mimic::Trial;

use crate::common::cli::Cli;
use crate::common::keys;
use crate::common::node::Node;

pub fn trials() -> Vec<Trial> {
    vec![
        Trial::test("routing::finalization_through_the_observer_only", || {
            finalization_through_the_observer_only();
            Ok(())
        }),
        Trial::test("routing::finalization_fails_without_an_observer", || {
            finalization_fails_without_an_observer();
            Ok(())
        }),
        Trial::test("routing::a_validator_refuses_exploratory_queries", || {
            a_validator_refuses_exploratory_queries();
            Ok(())
        }),
        Trial::test("routing::the_read_only_node_refuses_deploys", || {
            the_read_only_node_refuses_deploys();
            Ok(())
        }),
    ]
}

/// The deploy node's own HTTP port is wrong on purpose: finalization must be
/// read through `--observer-http-port`, which is the port a shard actually
/// serves the finalization endpoints on.
fn finalization_through_the_observer_only() {
    Cli::new("deploy-and-wait")
        .args(["-f", contract("stdout")])
        .key(&keys::DEPLOY_A)
        .args(["-H", "localhost"])
        .args(["-p", &Node::Validator1.grpc_port().to_string()])
        .args(["--http-port", "1"])
        .observing(Node::ReadOnly)
        .args(["--max-wait", "120"])
        .run()
        .expect_success("deploy-and-wait through the observer")
        .expect_contains("Block hash:");
}

/// The negative control for the test above: with no reachable finalization
/// endpoint the command must fail rather than report success.
fn finalization_fails_without_an_observer() {
    Cli::new("deploy-and-wait")
        .args(["-f", contract("stdout")])
        .key(&keys::DEPLOY_B)
        .args(["-H", "localhost"])
        .args(["-p", &Node::Validator1.grpc_port().to_string()])
        .args(["--http-port", "1"])
        .args(["--observer-host", "localhost"])
        .args(["--observer-port", "1"])
        .args(["--observer-http-port", "1"])
        .args(["--max-wait", "20"])
        .run()
        .expect_failure("deploy-and-wait with no reachable observer");
}

/// Exploratory deploys are served only by a read-only node.
fn a_validator_refuses_exploratory_queries() {
    Cli::new("bonds")
        .http_at(Node::Validator1)
        .run()
        .expect_failure("bonds against a validator");

    Cli::new("bonds")
        .http_at(Node::ReadOnly)
        .run()
        .expect_success("bonds against the read-only node")
        .expect_contains("Bonded Validators");
}

fn the_read_only_node_refuses_deploys() {
    Cli::new("deploy")
        .args(["-f", contract("stdout")])
        .key(&keys::SPARE)
        .grpc_at(Node::ReadOnly)
        .run()
        .expect_failure("deploy against the read-only node")
        .expect_contains("read-only mode");
}

/// Path to one of the suite's Rholang fixtures.
fn contract(name: &str) -> &'static str {
    match name {
        "stdout" => concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/integration/contracts/stdout.rho"
        ),
        other => panic!("no contract named {other}"),
    }
}
