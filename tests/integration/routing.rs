//! Group A - which node a command talks to, and over which endpoint.

use libtest_mimic::Trial;

use crate::common::cli::Cli;
use crate::common::contracts;
use crate::common::keys;
use crate::common::node::{published, Node};
use crate::common::shard;

/// validator1's gRPC API behind the TLS proxy.
fn tls_grpc() -> String {
    tls_endpoint(shard::TLS_PROXY_PORTS[0])
}

/// validator1's HTTP API behind the TLS proxy.
fn tls_http() -> String {
    tls_endpoint(shard::TLS_PROXY_PORTS[1])
}

/// validator1's HTTP API behind the TLS proxy, under a base path.
fn tls_http_under_path() -> String {
    format!("{}/node1", tls_endpoint(shard::TLS_PROXY_PORTS[2]))
}

/// The proxy's certificate names `localhost`, so the endpoint must too.
fn tls_endpoint(proxy_port: u16) -> String {
    format!(
        "https://localhost:{}",
        published(shard::TLS_PROXY, proxy_port)
    )
}

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
        .args([
            "--grpc".to_string(),
            tls_grpc(),
            "--http".to_string(),
            tls_http_under_path(),
        ])
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
        .args(["--grpc".to_string(), tls_grpc()])
        .run()
        .expect_failure("show-main-chain against an untrusted certificate");
    Cli::new("status")
        .args(["--http".to_string(), tls_http()])
        .run()
        .expect_failure("status against an untrusted certificate");

    Cli::new("show-main-chain")
        .args(["--grpc".to_string(), tls_grpc()])
        .args(["--ca-cert".to_string(), ca_certificate()])
        .run()
        .expect_success("show-main-chain with the proxy's CA")
        .expect_contains("Main chain blocks retrieved");
    Cli::new("status")
        .args(["--http".to_string(), tls_http()])
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
