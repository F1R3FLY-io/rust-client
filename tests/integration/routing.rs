//! Group A - which node a command talks to, and over which endpoint.

use libtest_mimic::Trial;

use crate::common::cli::Cli;
use crate::common::contracts;
use crate::common::keys;
use crate::common::node::Node;
use crate::common::shard;

/// validator1's gRPC and HTTP APIs behind the TLS proxy, the HTTP API also
/// under a base path.
const TLS_GRPC: &str = "https://localhost:40460";
const TLS_HTTP: &str = "https://localhost:40461";
const TLS_HTTP_UNDER_PATH: &str = "https://localhost:40462/node1";

/// An endpoint nothing listens on.
const UNREACHABLE: &str = "http://localhost:1";

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
        Trial::test(
            "routing::a_deploy_through_tls_finalizes_on_the_deploy_node_alone",
            || {
                a_deploy_through_tls_finalizes_on_the_deploy_node_alone();
                Ok(())
            },
        ),
        Trial::test("routing::an_untrusted_certificate_is_refused", || {
            an_untrusted_certificate_is_refused();
            Ok(())
        }),
        Trial::test(
            "routing::a_profile_alone_supplies_the_endpoints_and_the_key_file",
            || {
                a_profile_alone_supplies_the_endpoints_and_the_key_file();
                Ok(())
            },
        ),
    ]
}

/// The deploy node's HTTP endpoint is unreachable on purpose: the verdict and
/// the deploy's details must come through the observer.
fn finalization_through_the_observer_only() {
    Cli::new("deploy-and-wait")
        .args(["-f", contracts::path("stdout.rho")])
        .key(&keys::DEPLOY_A)
        .grpc_at(Node::Validator1)
        .args(["--http", UNREACHABLE])
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
        .args(["-f", contracts::path("stdout.rho")])
        .key(&keys::DEPLOY_B)
        .grpc_at(Node::Validator1)
        .args(["--http", UNREACHABLE])
        .args(["--observer", UNREACHABLE])
        .args(["--max-wait", "20"])
        .run()
        .expect_failure("deploy-and-wait with no reachable observer");
}

/// Exploratory deploys are served only by a read-only node.
fn a_validator_refuses_exploratory_queries() {
    Cli::new("bonds")
        .http_at(Node::Validator1)
        .run()
        .expect_failure("bonds against a validator")
        .expect_contains("readonly_node_required");

    Cli::new("bonds")
        .http_at(Node::ReadOnly)
        .run()
        .expect_success("bonds against the read-only node")
        .expect_contains("Bonded Validators");
}

fn the_read_only_node_refuses_deploys() {
    Cli::new("deploy")
        .args(["-f", contracts::path("stdout.rho")])
        .key(&keys::SPARE)
        .grpc_at(Node::ReadOnly)
        .run()
        .expect_failure("deploy against the read-only node")
        .expect_contains("read-only mode");
}

/// Deploy, verdict and result all go over TLS, the HTTP calls under a base
/// path, and with no observer the wait polls only the deploy node.
fn a_deploy_through_tls_finalizes_on_the_deploy_node_alone() {
    Cli::new("deploy-and-wait")
        .args(["-f", contracts::path("stdout.rho")])
        .key(&keys::TLS_DEPLOY)
        .args(["--grpc", TLS_GRPC, "--http", TLS_HTTP_UNDER_PATH])
        .args(["--ca-cert".to_string(), ca_certificate()])
        .args(["--max-wait", "120"])
        .run()
        .expect_success("deploy-and-wait over TLS")
        .expect_contains("Block hash:")
        .expect_contains("Data[0]:");
}

/// A certificate from an authority the client was not told to trust is
/// refused, over gRPC and HTTP alike, and the same calls succeed once it is.
fn an_untrusted_certificate_is_refused() {
    Cli::new("show-main-chain")
        .args(["--grpc", TLS_GRPC])
        .run()
        .expect_failure("show-main-chain against an untrusted certificate");
    Cli::new("status")
        .args(["--http", TLS_HTTP])
        .run()
        .expect_failure("status against an untrusted certificate");

    Cli::new("show-main-chain")
        .args(["--grpc", TLS_GRPC])
        .args(["--ca-cert".to_string(), ca_certificate()])
        .run()
        .expect_success("show-main-chain with the proxy's CA")
        .expect_contains("Main chain blocks retrieved");
    Cli::new("status")
        .args(["--http", TLS_HTTP])
        .args(["--ca-cert".to_string(), ca_certificate()])
        .run()
        .expect_success("status with the proxy's CA")
        .expect_contains("Shard:");
}

/// With only `--config`, a profile supplies the deploy node, the observer and
/// the signing key file, and `config show` names that key's vault address.
fn a_profile_alone_supplies_the_endpoints_and_the_key_file() {
    let config = shard::shard_dir().join("profile.toml");
    std::fs::write(
        &config,
        format!(
            "default_profile = \"it\"\n\n[profiles.it]\ngrpc = \"{}\"\nhttp = \"{}\"\nobserver = \"{}\"\nprivate_key_file = \"keys/profile-deploy.hex\"\n",
            Node::Validator1.grpc_endpoint(),
            Node::Validator1.http_endpoint(),
            Node::ReadOnly.http_endpoint(),
        ),
    )
    .expect("profile config");
    let config = config.display().to_string();

    Cli::new("deploy-and-wait")
        .args(["-f", contracts::path("stdout.rho")])
        .args(["--config", &config, "--max-wait", "120"])
        .run()
        .expect_success("deploy-and-wait from a profile")
        .expect_contains("Block hash:");

    Cli::new("config")
        .args(["show", "--config", &config])
        .run()
        .expect_success("config show")
        .expect_contains(&keys::PROFILE_DEPLOY.address)
        .expect_contains("(profile it)");
}

fn ca_certificate() -> String {
    shard::ca_certificate().display().to_string()
}
