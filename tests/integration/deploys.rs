//! Group B - what a deploy reports on its way to a terminal verdict.

use std::time::Duration;

use libtest_mimic::Trial;
use node_cli::connection_manager::{ConnectionConfig, F1r3flyConnectionManager};
use node_cli::{vault, DeployOptions, Endpoint, F1r3flyApi};

use crate::common::cli::Cli;
use crate::common::contracts;
use crate::common::keys::{self, Key};
use crate::common::node::Node;
use crate::common::rt;

pub fn trials() -> Vec<Trial> {
    vec![
        Trial::test(
            "deploys::a_deploy_writing_to_deploy_id_returns_its_data",
            || {
                a_deploy_writing_to_deploy_id_returns_its_data();
                Ok(())
            },
        ),
        Trial::test("deploys::a_deploy_writing_nothing_returns_no_data", || {
            a_deploy_writing_nothing_returns_no_data();
            Ok(())
        }),
        Trial::test(
            "deploys::an_errored_deploy_reports_its_execution_failure",
            || {
                an_errored_deploy_reports_its_execution_failure();
                Ok(())
            },
        ),
        Trial::test("deploys::an_expiration_in_the_past_is_refused", || {
            an_expiration_in_the_past_is_refused();
            Ok(())
        }),
        Trial::test("deploys::deploy_status_reports_a_finalized_deploy", || {
            deploy_status_reports_a_finalized_deploy();
            Ok(())
        }),
        Trial::test("deploys::a_signed_deploy_sent_twice_is_one_deploy", || {
            a_signed_deploy_sent_twice_is_one_deploy();
            Ok(())
        }),
        Trial::test(
            "deploys::a_landed_deploy_resent_after_it_expired_is_the_same_deploy",
            || {
                a_landed_deploy_resent_after_it_expired_is_the_same_deploy();
                Ok(())
            },
        ),
        Trial::test("deploys::a_deploy_for_another_shard_is_refused", || {
            a_deploy_for_another_shard_is_refused();
            Ok(())
        }),
    ]
}

/// The library returns the deploy's own output, and the CLI prints it.
fn a_deploy_writing_to_deploy_id_returns_its_data() {
    let result = rt()
        .block_on(
            manager(&keys::DEPLOY_A, 120)
                .deploy_and_wait(contracts::STDOUT_TERM, &DeployOptions::default()),
        )
        .expect("deploy-and-wait");

    let data = result.data.expect("deployId data should be readable");
    assert!(
        !data.is_empty(),
        "a deploy writing to deployId returned no data"
    );

    Cli::new("deploy-and-wait")
        .args(["-f", contracts::path("stdout.rho")])
        .key(&keys::DEPLOY_B)
        .at(Node::Validator1)
        .observing(Node::ReadOnly)
        .args(["--max-wait", "120"])
        .run()
        .expect_success("deploy-and-wait")
        .expect_contains("Data[0]:");
}

/// A deploy that writes nothing reads as empty, not as a failed read.
fn a_deploy_writing_nothing_returns_no_data() {
    let result = rt()
        .block_on(
            manager(&keys::SPARE, 120)
                .deploy_and_wait(contracts::SILENT_TERM, &DeployOptions::default()),
        )
        .expect("deploy-and-wait");

    let data = result.data.expect("deployId data should be readable");
    assert!(
        data.is_empty(),
        "a deploy writing nothing returned {} values",
        data.len()
    );
}

/// A vault transfer at the 50k default phlo limit runs out of phlo. The node
/// writes its `Failed` verdict only past the contestability bound, so the
/// command must name the execution failure rather than time out silently.
fn an_errored_deploy_reports_its_execution_failure() {
    let term = vault::build_transfer_rholang(
        keys::OVERDRAFT.address,
        keys::TRANSFER_RECIPIENT.address,
        &1u32.into(),
    )
    .unwrap();
    let error = rt()
        .block_on(manager(&keys::OVERDRAFT, 90).deploy_and_wait(&term, &DeployOptions::default()))
        .expect_err("a 50k-phlo vault transfer should not succeed");

    assert!(
        matches!(
            error,
            node_cli::Error::DeployFailed {
                execution: Some(_),
                ..
            } | node_cli::Error::FinalizationTimeout {
                execution: Some(_),
                ..
            }
        ),
        "the failure does not name the errored execution: {error}"
    );
}

/// The node refuses a deploy whose expiration has already passed, at
/// submission rather than by leaving it pending.
fn an_expiration_in_the_past_is_refused() {
    let expired = (std::time::SystemTime::now() - Duration::from_secs(3600))
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_millis() as i64;
    let options = DeployOptions {
        expiration: Some(expired),
        ..DeployOptions::default()
    };

    let error = rt()
        .block_on(manager(&keys::LOAD_TEST, 30).deploy_and_wait(contracts::STDOUT_TERM, &options))
        .expect_err("an expired deploy should be refused");
    assert!(
        error.to_string().to_lowercase().contains("expire"),
        "the refusal does not mention expiry: {error}"
    );
}

/// `deploy-status` reaches a terminal state for a deploy that finalized.
fn deploy_status_reports_a_finalized_deploy() {
    let deploy_id = rt()
        .block_on(
            manager(&keys::DEPLOY_C, 120)
                .deploy_and_wait(contracts::STDOUT_TERM, &DeployOptions::default()),
        )
        .expect("deploy-and-wait")
        .deploy_id;

    Cli::new("deploy-status")
        .args(["--sig", &deploy_id])
        .http_at(Node::ReadOnly)
        .run()
        .expect_success("deploy-status")
        .expect_contains("Finalized");
}

/// A deploy's ID is known once it is signed, and sending the same signed
/// deploy again, as a caller recovering from a timeout would, is not a second
/// deploy: the node answers with the same ID and the deploy finalizes once.
fn a_signed_deploy_sent_twice_is_one_deploy() {
    let key = node_cli::utils::CryptoUtils::decode_private_key(keys::RESUBMIT.private).unwrap();
    let api = F1r3flyApi::new(endpoint(Node::Validator1.grpc_endpoint()));
    let signed = rt()
        .block_on(api.prepare(contracts::STDOUT_TERM, &DeployOptions::default()))
        .expect("prepare")
        .sign(&key);
    let stored = node_cli::SignedDeploy::from_bytes(&signed.to_bytes()).expect("stored deploy");

    let first = rt().block_on(api.submit(&signed)).expect("first submit");
    let second = rt().block_on(api.submit(&stored)).expect("second submit");
    assert_eq!(first, signed.id());
    assert_eq!(second, signed.id());

    let result = rt()
        .block_on(manager(&keys::RESUBMIT, 120).submit_and_wait(&stored))
        .expect("submit-and-wait");
    assert_eq!(result.deploy_id, signed.id());
    assert!(
        !result.data.expect("deployId data").is_empty(),
        "the resent deploy returned no data"
    );
}

/// Once a deploy's validity window closes the node refuses it as expired,
/// even when it already landed. Resending it must still resolve to the landed
/// deploy rather than report it refused.
fn a_landed_deploy_resent_after_it_expired_is_the_same_deploy() {
    let key =
        node_cli::utils::CryptoUtils::decode_private_key(keys::RESEND_EXPIRED.private).unwrap();
    let api = F1r3flyApi::new(endpoint(Node::Validator1.grpc_endpoint()));
    let expiration = unix_millis() + 45_000;
    let options = DeployOptions {
        expiration: Some(expiration),
        ..DeployOptions::default()
    };
    let signed = rt()
        .block_on(api.prepare(contracts::STDOUT_TERM, &options))
        .expect("prepare")
        .sign(&key);
    let manager = manager(&keys::RESEND_EXPIRED, 120);

    let landed = rt()
        .block_on(manager.submit_and_wait(&signed))
        .expect("the first submit-and-wait");
    assert_eq!(landed.deploy_id, signed.id());

    let wait = expiration + 2_000 - unix_millis();
    if wait > 0 {
        std::thread::sleep(Duration::from_millis(wait as u64));
    }
    let refusal = rt()
        .block_on(api.submit(&signed))
        .expect_err("the node should refuse an expired deploy");
    assert!(
        matches!(refusal, node_cli::Error::NodeRejected { ref message, .. }
            if message.contains("expired")),
        "the refusal does not name the expiry: {refusal}"
    );

    let resent = rt()
        .block_on(manager.submit_and_wait(&signed))
        .expect("resending the landed deploy");
    assert_eq!(resent.deploy_id, signed.id());
    assert_eq!(resent.block_hash, landed.block_hash);
}

fn unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// A deploy signs its shard ID, so a node of another shard refuses it at
/// submission.
fn a_deploy_for_another_shard_is_refused() {
    let api = F1r3flyApi::new(endpoint(Node::Validator1.grpc_endpoint()))
        .with_signing_key(keys::SPARE.private)
        .unwrap()
        .with_shard_id("not-this-shard");
    let error = rt()
        .block_on(api.deploy(contracts::STDOUT_TERM, &DeployOptions::default()))
        .expect_err("a deploy for another shard should be refused");
    assert!(
        matches!(error, node_cli::Error::NodeRejected { ref message, .. }
            if message.contains("not-this-shard")),
        "the refusal does not name the shard: {error}"
    );
}

/// A manager deploying through validator1 and reading finalization from the
/// read-only node, which is how every supported deployment is wired.
fn manager(key: &Key, finalization_timeout_secs: u32) -> F1r3flyConnectionManager {
    let mut config = ConnectionConfig::new(
        endpoint(Node::Validator1.grpc_endpoint()),
        endpoint(Node::Validator1.http_endpoint()),
        key.private.to_string(),
    )
    .with_observer(endpoint(Node::ReadOnly.http_endpoint()));
    config.finalization_timeout_secs = finalization_timeout_secs;
    F1r3flyConnectionManager::new(config).expect("connection manager")
}

fn endpoint(url: String) -> Endpoint {
    Endpoint::parse(&url).expect("a shard endpoint")
}
