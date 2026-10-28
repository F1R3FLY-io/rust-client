//! Deploy and propose operations

use super::F1r3flyApi;
use crate::deploy::{DeployOptions, SignedDeploy, UnsignedDeploy};
use crate::error::{Error, Result};
use f1r3fly_models::casper::v1::deploy_response::Message as DeployResponseMessage;
use f1r3fly_models::casper::v1::propose_response::Message as ProposeResponseMessage;
use f1r3fly_models::casper::ProposeQuery;
use std::time::Instant;

use crate::f1r3fly_api::ProposeResult;

/// The node's answer to a deploy it already holds.
const ALREADY_KNOWN: &str = "Deploy already known";

impl<'a> F1r3flyApi<'a> {
    /// Fix a deploy's fields for signing, anchoring its validity window at the
    /// node's current tip.
    pub async fn prepare(&self, term: &str, options: &DeployOptions) -> Result<UnsignedDeploy> {
        let tip_lookup_start = Instant::now();
        let valid_after = self.get_current_block_number_monotonic().await?;
        tracing::debug!(elapsed = ?tip_lookup_start.elapsed(), valid_after, "Tip selection");
        UnsignedDeploy::new(term, options, valid_after, &self.shard_id)
    }

    /// Send a signed deploy and return its ID. Sending a deploy the node
    /// already holds also returns its ID, so a deploy whose fate is unknown,
    /// after a timeout or a crash, can be sent again safely.
    pub async fn submit(&self, deploy: &SignedDeploy) -> Result<String> {
        let id = deploy.id();
        let connect_start = Instant::now();
        let mut client = self.deploy_client().await?;
        tracing::debug!(elapsed = ?connect_start.elapsed(), "gRPC connect");

        let do_deploy_start = Instant::now();
        let response = client
            .do_deploy(deploy.proto().clone())
            .await
            .map_err(|status| self.grpc_error(status))?
            .into_inner();
        tracing::debug!(elapsed = ?do_deploy_start.elapsed(), "do_deploy RPC");

        match response.message {
            Some(DeployResponseMessage::Error(error)) => {
                let message = error.messages.join("; ");
                if message.contains(ALREADY_KNOWN) && message.contains(&id) {
                    tracing::debug!(deploy_id = %id, "The node already holds this deploy");
                    Ok(id)
                } else {
                    Err(Error::NodeRejected {
                        operation: "the deploy",
                        message,
                    })
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
