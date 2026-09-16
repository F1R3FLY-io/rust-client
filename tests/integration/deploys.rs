//! Group B - what a deploy reports on its way to a terminal verdict.

use std::time::Duration;

use libtest_mimic::Trial;
use node_cli::connection_manager::{ConnectionConfig, F1r3flyConnectionManager};
use node_cli::vault;

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
    ]
}

/// The library returns the deploy's own output, and the CLI prints it.
fn a_deploy_writing_to_deploy_id_returns_its_data() {
    let result = rt()
        .block_on(manager(&keys::DEPLOY_A, 120).deploy_and_wait(contracts::STDOUT_TERM, false, 0))
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
        .block_on(manager(&keys::SPARE, 120).deploy_and_wait(contracts::SILENT_TERM, false, 0))
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
    let term =
        vault::build_transfer_rholang(keys::OVERDRAFT.address, keys::TRANSFER_RECIPIENT.address, 1);
    let error = rt()
        .block_on(manager(&keys::OVERDRAFT, 90).deploy_and_wait(&term, false, 0))
        .expect_err("a 50k-phlo vault transfer should not succeed");

    let message = error.to_string();
    assert!(
        message.contains("executed with an error in block"),
        "the failure does not name the errored execution: {message}"
    );
}

/// The node refuses a deploy whose expiration has already passed, at
/// submission rather than by leaving it pending.
fn an_expiration_in_the_past_is_refused() {
    let expired = (std::time::SystemTime::now() - Duration::from_secs(3600))
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_millis() as i64;

    let error = rt()
        .block_on(manager(&keys::LOAD_TEST, 30).deploy_and_wait(
            contracts::STDOUT_TERM,
            false,
            expired,
        ))
        .expect_err("an expired deploy should be refused");
    assert!(
        error.to_string().to_lowercase().contains("expire"),
        "the refusal does not mention expiry: {error}"
    );
}

/// `deploy-status` reaches a terminal state for a deploy that finalized.
fn deploy_status_reports_a_finalized_deploy() {
    let deploy_id = rt()
        .block_on(manager(&keys::DEPLOY_C, 120).deploy_and_wait(contracts::STDOUT_TERM, false, 0))
        .expect("deploy-and-wait")
        .deploy_id;

    Cli::new("deploy-status")
        .args(["--sig", &deploy_id])
        .http_port_at(Node::ReadOnly)
        .run()
        .expect_success("deploy-status")
        .expect_contains("Finalized");
}

/// A manager deploying through validator1 and reading finalization from the
/// read-only node, which is how every supported deployment is wired.
fn manager(key: &Key, finalization_timeout_secs: u32) -> F1r3flyConnectionManager {
    let mut config = ConnectionConfig::new(
        "localhost".to_string(),
        Node::Validator1.grpc_port(),
        Node::Validator1.http_port(),
        key.private.to_string(),
    )
    .with_observer(
        "localhost".to_string(),
        Node::ReadOnly.grpc_port(),
        Node::ReadOnly.http_port(),
    );
    config.finalization_timeout_secs = finalization_timeout_secs;
    F1r3flyConnectionManager::new(config)
}
