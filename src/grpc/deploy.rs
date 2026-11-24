//! Deploy and propose operations

use super::F1r3flyApi;
use crate::deploy::{DeployOptions, SignedDeploy, UnsignedDeploy};
use crate::error::{DeployAdmissionReason, Error, QueueCongestion, Result};
use f1r3fly_models::casper::v1::deploy_response::Message as DeployResponseMessage;
use f1r3fly_models::casper::v1::propose_response::Message as ProposeResponseMessage;
use f1r3fly_models::casper::ProposeQuery;
use std::time::{Duration, Instant};

use crate::f1r3fly_api::ProposeResult;

/// The node's answer to a deploy it already holds.
const ALREADY_KNOWN: &str = "Deploy already known";

impl F1r3flyApi {
    /// Fix a deploy's fields for signing, anchoring its validity window at the
    /// node's current tip.
    pub async fn prepare(&self, term: &str, options: &DeployOptions) -> Result<UnsignedDeploy> {
        let tip_lookup_start = Instant::now();
        let valid_after = self.get_current_block_number_monotonic().await?;
        tracing::debug!(elapsed = ?tip_lookup_start.elapsed(), valid_after, "Tip selection");
        UnsignedDeploy::new(term, options, valid_after, &self.shard_id)
    }

    /// Send a signed deploy and return its ID. While the deploy's validity
    /// window is open, sending a deploy the node already holds also returns
    /// its ID. After the window closes the node refuses it as expired whether
    /// or not it landed; `F1r3flyConnectionManager::submit_and_wait` tells
    /// those cases apart.
    pub async fn submit(&self, deploy: &SignedDeploy) -> Result<String> {
        let id = deploy.id();
        let mut client = self.deploy_client().await?;

        let do_deploy_start = Instant::now();
        let response = client
            .do_deploy(deploy.proto().clone())
            .await
            .map_err(|status| {
                queue_refusal(&id, &status).unwrap_or_else(|| self.grpc_error(status))
            })?
            .into_inner();
        tracing::debug!(elapsed = ?do_deploy_start.elapsed(), "do_deploy RPC");

        match response.message {
            Some(DeployResponseMessage::Error(error)) => {
                let message = error.messages.join("; ");
                if message.contains(ALREADY_KNOWN) && message.contains(&id) {
                    tracing::debug!(deploy_id = %id, "The node already holds this deploy");
                    Ok(id)
                } else {
                    Err(deploy_refusal(&id, &error.messages))
                }
            }
            Some(DeployResponseMessage::Result(result)) => {
                let answered = Self::extract_deploy_id(&result)?;
                if answered == id {
                    Ok(id)
                } else {
                    Err(Error::decode(
                        "deploy response",
                        format!("the node accepted deploy {answered}, but {id} was sent"),
                    ))
                }
            }
            None => Err(Error::decode("deploy response", "empty message")),
        }
    }

    /// Prepare, sign with this client's key, and submit a new deploy. Every
    /// call is a new deploy; to resend one, [`submit`](Self::submit) it again.
    pub async fn deploy(&self, term: &str, options: &DeployOptions) -> Result<String> {
        let key = self
            .signing_key
            .as_ref()
            .ok_or_else(|| Error::invalid("signing key", "this client was built without one"))?;
        let deploy = self.prepare(term, options).await?.sign(key);
        self.submit(&deploy).await
    }

    pub async fn propose(&self) -> Result<ProposeResult> {
        let mut propose_client = self.propose_client().await?;

        let propose_response = propose_client
            .propose(ProposeQuery { is_async: false })
            .await
            .map_err(|status| self.grpc_error(status))?
            .into_inner();

        let message = propose_response
            .message
            .ok_or_else(|| Error::decode("propose response", "empty message"))?;

        match message {
            ProposeResponseMessage::Result(block_hash) => {
                if let Some(hash) = block_hash
                    .strip_prefix("Success! Block ")
                    .and_then(|s| s.strip_suffix(" created and added."))
                {
                    Ok(ProposeResult::Proposed(hash.to_string()))
                } else if Self::is_recoverable_propose_error(&block_hash) {
                    Ok(ProposeResult::Skipped(block_hash))
                } else {
                    Ok(ProposeResult::Proposed(block_hash))
                }
            }
            ProposeResponseMessage::Error(error) => {
                let error_message = error.messages.join("; ");
                if Self::is_recoverable_propose_error(&error_message) {
                    Ok(ProposeResult::Skipped(error_message))
                } else {
                    Err(Error::NodeRejected {
                        operation: "the propose",
                        message: error_message,
                    })
                }
            }
        }
    }

    fn extract_deploy_id(result: &str) -> Result<String> {
        let cleaned = result.trim();
        if let Some(id) = cleaned.strip_prefix("Success! DeployId is: ") {
            Ok(id.trim().to_string())
        } else if let Some(id) = cleaned.strip_prefix("Success!\nDeployId is: ") {
            Ok(id.trim().to_string())
        } else if cleaned.starts_with("Success!") {
            for line in cleaned.lines() {
                let trimmed = line.trim();
                if trimmed.len() > 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Ok(trimmed.to_string());
                }
            }
            Err(Error::decode(
                "deploy response",
                format!("no deploy ID in {result:?}"),
            ))
        } else {
            Ok(cleaned.to_string())
        }
    }

    fn is_recoverable_propose_error(error_message: &str) -> bool {
        let normalized = error_message.to_ascii_lowercase();
        const RECOVERABLE_PATTERNS: [&str; 6] = [
            "must wait for more blocks from other validators",
            "no new blocks from peers yet; synchronize with network first",
            "no new deploys to propose",
            "propose skipped due to transient proposal race",
            "must wait for more blocks",
            "not enough new blocks",
        ];
        RECOVERABLE_PATTERNS.iter().any(|p| normalized.contains(p))
    }
}

fn deploy_refusal(deploy_id: &str, messages: &[String]) -> Error {
    let reason = messages.iter().find_map(|message| {
        let (code, _) = message.trim().split_once(':')?;
        DeployAdmissionReason::from_code(code)
    });
    let message = messages.join("; ");
    match reason {
        Some(reason) => Error::DeployAdmissionRefused {
            deploy_id: deploy_id.to_string(),
            reason,
            message,
            retry_after: None,
            congestion: None,
        },
        None => Error::NodeRejected {
            operation: "the deploy",
            message,
        },
    }
}

/// A deploy queue's capacity or availability refusal, which it answers as a
/// gRPC status rather than a service error. The queue did not hold the
/// deploy. Other statuses, such as a plain `UNAVAILABLE` from a dropped
/// connection, leave the outcome unknown and are not classified here.
fn queue_refusal(deploy_id: &str, status: &tonic::Status) -> Option<Error> {
    const SHARE_FULL: &str = "deploy queue is full for this deployer";
    const QUEUE_FULL: &str = "deploy queue is full:";
    const NODE_UNAVAILABLE: &str = "deploy queue is not connected to its node";
    const STOPPED: &str = "deploy queue is shutting down";

    let message = status.message();
    let reason = match status.code() {
        tonic::Code::ResourceExhausted if message.starts_with(SHARE_FULL) => {
            DeployAdmissionReason::DeployerShareFull
        }
        tonic::Code::ResourceExhausted if message.starts_with(QUEUE_FULL) => {
            DeployAdmissionReason::QueueFull
        }
        tonic::Code::Unavailable if message.starts_with(NODE_UNAVAILABLE) => {
            DeployAdmissionReason::QueueNodeUnavailable
        }
        tonic::Code::Unavailable if message.starts_with(STOPPED) => {
            DeployAdmissionReason::QueueStopped
        }
        _ => return None,
    };
    let congestion = match reason {
        DeployAdmissionReason::QueueFull => queue_congestion(&message[QUEUE_FULL.len()..]),
        _ => None,
    };
    let retry_after = status
        .metadata()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|secs| secs.trim().parse::<u64>().ok())
        .map(Duration::from_secs);
    Some(Error::DeployAdmissionRefused {
        deploy_id: deploy_id.to_string(),
        reason,
        message: message.to_string(),
        retry_after,
        congestion,
    })
}

/// The cause a full queue names right after its `deploy queue is full:` prefix.
fn queue_congestion(cause: &str) -> Option<QueueCongestion> {
    let cause = cause.trim_start();
    [
        (
            "the validator has created no block",
            QueueCongestion::ValidatorIdle,
        ),
        (
            "the validator is taking fewer deploys",
            QueueCongestion::Validator,
        ),
        ("the shard's finality", QueueCongestion::Shard),
        (
            "the signed deploys the queue holds",
            QueueCongestion::Storage,
        ),
    ]
    .into_iter()
    .find_map(|(prefix, congestion)| cause.starts_with(prefix).then_some(congestion))
}

#[cfg(test)]
mod admission_tests {
    use super::*;

    fn with_retry_after(mut status: tonic::Status, secs: &str) -> tonic::Status {
        status
            .metadata_mut()
            .insert("retry-after", secs.parse().unwrap());
        status
    }

    #[test]
    fn deploy_queue_statuses_become_refusals_with_their_wait_and_cause() {
        for (message, congestion) in [
            (
                "deploy queue is full: the validator is taking fewer deploys than arrive, so the \
                 deploys ahead of this one would not land before its validity window closes; \
                 sign it again with a current validAfterBlockNumber and retry in 12s",
                QueueCongestion::Validator,
            ),
            (
                "deploy queue is full: the validator has created no block while the shard moved \
                 on, so ...",
                QueueCongestion::ValidatorIdle,
            ),
            (
                "deploy queue is full: the shard's finality is holding the validator back, so ...",
                QueueCongestion::Shard,
            ),
            (
                "deploy queue is full: the signed deploys the queue holds fill its storage \
                 budget, so ...",
                QueueCongestion::Storage,
            ),
        ] {
            let status = with_retry_after(tonic::Status::resource_exhausted(message), "12");
            let refusal = queue_refusal("signed-id", &status).expect("a queue refusal");
            assert!(
                matches!(&refusal, Error::DeployAdmissionRefused {
                    deploy_id, reason: DeployAdmissionReason::QueueFull, retry_after, congestion: found, ..
                } if deploy_id == "signed-id"
                    && *retry_after == Some(Duration::from_secs(12))
                    && *found == Some(congestion)),
                "{refusal:?}"
            );
        }
        assert!(!DeployAdmissionReason::QueueFull.is_retryable());
    }

    #[test]
    fn deploy_queue_share_and_availability_refusals_allow_the_same_deploy() {
        for (status, expected) in [
            (
                with_retry_after(
                    tonic::Status::resource_exhausted(
                        "deploy queue is full for this deployer: it holds 4 of the 4 deploys one \
                         deployer may hold, so that other deployers keep room; retry in 3s",
                    ),
                    "3",
                ),
                DeployAdmissionReason::DeployerShareFull,
            ),
            (
                with_retry_after(
                    tonic::Status::unavailable(
                        "deploy queue is not connected to its node; retry in 5s",
                    ),
                    "5",
                ),
                DeployAdmissionReason::QueueNodeUnavailable,
            ),
            (
                tonic::Status::unavailable("deploy queue is shutting down"),
                DeployAdmissionReason::QueueStopped,
            ),
        ] {
            let refusal = queue_refusal("id", &status).expect("a queue refusal");
            assert!(
                matches!(refusal, Error::DeployAdmissionRefused { reason, congestion: None, .. }
                    if reason == expected),
                "{refusal:?}"
            );
            assert!(expected.is_retryable());
        }
        assert_eq!(
            DeployAdmissionReason::DeployerShareFull.code(),
            "deploy_queue_full"
        );
    }

    #[test]
    fn other_grpc_statuses_keep_an_unknown_outcome() {
        for status in [
            tonic::Status::unavailable("connection refused"),
            tonic::Status::resource_exhausted("message length too large"),
            tonic::Status::internal("queue store failed"),
            tonic::Status::unknown("deploy queue is full: the shard's finality"),
        ] {
            assert!(queue_refusal("id", &status).is_none(), "{status:?}");
        }
    }

    #[test]
    fn deploy_service_envelope_preserves_the_admission_reason_and_signed_id() {
        for (code, expected, retryable) in [
            ("deploy_pool_busy", DeployAdmissionReason::PoolBusy, true),
            (
                "deploy_admission_busy",
                DeployAdmissionReason::AdmissionBusy,
                true,
            ),
            ("deploy_too_large", DeployAdmissionReason::TooLarge, false),
            ("deploy_expired", DeployAdmissionReason::Expired, false),
            (
                "invalid_deploy_window",
                DeployAdmissionReason::OutsideWindow,
                true,
            ),
        ] {
            let messages = vec![format!("{code}: deploy was not accepted")];
            assert!(matches!(deploy_refusal("signed-id", &messages),
                Error::DeployAdmissionRefused { deploy_id, reason, message, .. }
                if deploy_id == "signed-id" && reason == expected && message == messages[0]));
            assert_eq!(expected.code(), code);
            assert_eq!(expected.is_retryable(), retryable, "{code}");
        }
    }

    #[test]
    fn unrelated_refusals_and_code_mentions_do_not_become_admission_refusals() {
        for message in [
            "unknown error",
            "deploy_pool_busy_extra: failed",
            "unknown_error: deploy_pool_busy",
            "deploy_expired",
            "Deploy has expired: expirationTimestamp=Some(1) is in the past.",
            "timeout",
        ] {
            assert!(matches!(
                deploy_refusal("id", &[message.into()]),
                Error::NodeRejected { .. }
            ));
        }
    }
}
