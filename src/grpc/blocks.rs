//! Block queries, finalization checks, and tip sampling

use super::{rejected, F1r3flyApi};
use crate::error::{Error, Result, TransportError};
use crate::f1r3fly_api::{BlocksByHeight, MissingBlock};
use f1r3fly_models::casper::v1::is_finalized_response::Message as IsFinalizedResponseMessage;
use f1r3fly_models::casper::{BlocksQuery, BlocksQueryByHeight, IsFinalizedQuery, LightBlockInfo};
use std::sync::atomic::Ordering;

const BLOCK_SAMPLE_DEPTH: u32 = 8;
const TIP_SAMPLE_ATTEMPTS: usize = 2;
const TIP_SAMPLE_DELAY_MS: u64 = 50;

impl F1r3flyApi {
    pub async fn is_finalized(
        &self,
        block_hash: &str,
        max_attempts: u32,
        retry_delay_sec: u64,
    ) -> Result<bool> {
        let mut attempts = 0;

        loop {
            attempts += 1;

            let mut client = self.deploy_client().await?;

            let query = IsFinalizedQuery {
                hash: block_hash.to_string(),
            };

            match client.is_finalized(query).await {
                Ok(response) => match &response.get_ref().message {
                    Some(IsFinalizedResponseMessage::Error(error)) => {
                        return Err(rejected("isFinalized", error));
                    }
                    Some(IsFinalizedResponseMessage::IsFinalized(true)) => return Ok(true),
                    Some(IsFinalizedResponseMessage::IsFinalized(false)) => {}
                    None => return Err(Error::decode("isFinalized response", "empty message")),
                },
                Err(status) => {
                    if attempts >= max_attempts {
                        return Err(self.grpc_error(status));
                    }
                }
            }

            if attempts >= max_attempts {
                return Ok(false);
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_sec)).await;
        }
    }

    pub async fn show_main_chain(&self, depth: u32) -> Result<Vec<LightBlockInfo>> {
        let mut client = self.deploy_client().await?;
        let query = BlocksQuery {
            depth: depth as i32,
        };
        let answer = self
            .within_request_timeout(async {
                let stream = client
                    .show_main_chain(query)
                    .await
                    .map_err(|status| self.grpc_error(status))?
                    .into_inner();
                self.collect_blocks("showMainChain", stream).await
            })
            .await?;
        if !answer.pruned.is_empty() || !answer.unavailable.is_empty() {
            return Err(Error::decode(
                "showMainChain stream",
                "a missing-block item, which this call never carries",
            ));
        }
        Ok(answer.blocks)
    }

    /// The blocks at heights `start_block_number..=end_block_number`, with the
    /// blocks the node names there but does not hold, which it would
    /// otherwise leave out without saying so.
    pub async fn get_blocks_by_height(
        &self,
        start_block_number: i64,
        end_block_number: i64,
    ) -> Result<BlocksByHeight> {
        let mut client = self.deploy_client().await?;
        let query = BlocksQueryByHeight {
            start_block_number,
            end_block_number,
            include_missing: true,
        };
        self.within_request_timeout(async {
            let stream = client
                .get_blocks_by_heights(query)
                .await
                .map_err(|status| self.grpc_error(status))?
                .into_inner();
            self.collect_blocks("getBlocksByHeights", stream).await
        })
        .await
    }

    /// `call`, response stream included, bounded by the request timeout. The
    /// channel's own timeout ends when the response headers arrive.
    async fn within_request_timeout<T>(
        &self,
        call: impl std::future::Future<Output = Result<T>>,
    ) -> Result<T> {
        let timeout = self.endpoint.request_timeout();
        tokio::time::timeout(timeout, call).await.map_err(|_| {
            Error::transport(self.endpoint.to_string(), TransportError::TimedOut(timeout))
        })?
    }

    async fn collect_blocks(
        &self,
        operation: &'static str,
        mut stream: tonic::Streaming<f1r3fly_models::casper::v1::BlockInfoResponse>,
    ) -> Result<BlocksByHeight> {
        use f1r3fly_models::casper::v1::block_info_response::Message;

        let mut answer = BlocksByHeight::default();
        while let Some(response) = stream
            .message()
            .await
            .map_err(|status| self.grpc_error(status))?
        {
            let (missing, hash, height, prune_floor) = match response.message {
                Some(Message::Error(error)) => return Err(rejected(operation, &error)),
                Some(Message::BlockInfo(block_info)) => {
                    answer.blocks.push(block_info);
                    continue;
                }
                Some(Message::Pruned(info)) => (
                    &mut answer.pruned,
                    info.hash,
                    info.block_height,
                    info.prune_floor,
                ),
                Some(Message::Unavailable(info)) => (
                    &mut answer.unavailable,
                    info.hash,
                    info.block_height,
                    info.prune_floor,
                ),
                None => return Err(Error::decode("block stream", "empty message")),
            };
            missing.push(MissingBlock {
                hash: hex::encode(hash),
                height,
            });
            match answer.prune_floor {
                Some(floor) if floor != prune_floor => {
                    return Err(Error::decode(
                        "block stream",
                        format!("prune floors {floor} and {prune_floor} in one answer"),
                    ))
                }
                _ => answer.prune_floor = Some(prune_floor),
            }
        }
        Ok(answer)
    }

    pub(crate) async fn get_current_block_number_monotonic(&self) -> Result<i64> {
        let sampled_tip = self.get_current_block_number_sampled().await?;
        let cached_tip = self.tip_floor.load(Ordering::Relaxed);
        let selected_tip = if cached_tip != super::TIP_FLOOR_UNSET && sampled_tip < cached_tip {
            tracing::warn!(
                cached_tip,
                sampled_tip,
                "Tip sample regressed from in-memory floor. Using cached floor."
            );
            cached_tip
        } else {
            sampled_tip
        };
        self.tip_floor.store(selected_tip, Ordering::Relaxed);
        Ok(selected_tip)
    }

    async fn get_current_block_number_sampled(&self) -> Result<i64> {
        let mut best_tip: Option<i64> = None;
        let mut min_tip: Option<i64> = None;
        let mut max_tip: Option<i64> = None;
        let mut successful_samples: usize = 0;
        let mut last_error: Option<Error> = None;

        for attempt in 1..=TIP_SAMPLE_ATTEMPTS {
            let sampled_tip = match self.show_main_chain(BLOCK_SAMPLE_DEPTH).await {
                Ok(blocks) => blocks.iter().map(|b| b.block_number).max(),
                Err(err) => {
                    tracing::warn!(attempt, total = TIP_SAMPLE_ATTEMPTS, %err, "Tip sample failed");
                    last_error = Some(err);
                    None
                }
            };

            if let Some(tip) = sampled_tip {
                successful_samples += 1;
                best_tip = Some(best_tip.map_or(tip, |prev| prev.max(tip)));
                min_tip = Some(min_tip.map_or(tip, |prev| prev.min(tip)));
                max_tip = Some(max_tip.map_or(tip, |prev| prev.max(tip)));
            }

            if attempt < TIP_SAMPLE_ATTEMPTS {
                tokio::time::sleep(tokio::time::Duration::from_millis(TIP_SAMPLE_DELAY_MS)).await;
            }
        }

        if let Some(best_tip) = best_tip {
            let min_tip = min_tip.unwrap_or(best_tip);
            let max_tip = max_tip.unwrap_or(best_tip);
            if successful_samples > 1 {
                tracing::debug!(
                    successful_samples,
                    min_tip,
                    max_tip,
                    selected = best_tip,
                    "Tip sampling complete"
                );
            }
            Ok(best_tip)
        } else {
            Err(last_error
                .unwrap_or_else(|| Error::decode("main chain", "no blocks in any tip sample")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::DeployOptions;
    use crate::grpc::mock_node::{ends, BlockAnswer, MockNode, PROMPT_TIP, TIP};
    use std::time::{Duration, Instant};

    const SHORT_TIMEOUT: Duration = Duration::from_millis(300);

    async fn node(
        answers: impl IntoIterator<Item = BlockAnswer>,
        request_timeout: Duration,
    ) -> F1r3flyApi {
        MockNode::blocks(answers).serve(request_timeout).await
    }

    fn timed_out_after(result: Result<impl std::fmt::Debug>, timeout: Duration) {
        match result {
            Err(Error::Transport {
                cause: TransportError::TimedOut(after),
                ..
            }) => assert_eq!(after, timeout),
            other => panic!("expected a timeout after {timeout:?}, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_tip_slower_than_two_seconds_prepares_within_a_longer_request_timeout() {
        let slow = BlockAnswer::Tip {
            headers: Duration::from_millis(2500),
            block: Duration::ZERO,
        };
        let api = node([slow, slow], Duration::from_secs(5)).await;
        api.prepare("Nil", &DeployOptions::default()).await.unwrap();
        assert_eq!(api.tip_floor.load(Ordering::Relaxed), TIP);
    }

    #[tokio::test]
    async fn a_tip_stream_slower_than_the_request_timeout_fails_preparation() {
        let slow_body = BlockAnswer::Tip {
            headers: Duration::ZERO,
            block: Duration::from_millis(1500),
        };
        let api = node([slow_body, slow_body], SHORT_TIMEOUT).await;
        let started = Instant::now();
        timed_out_after(
            api.prepare("Nil", &DeployOptions::default()).await,
            SHORT_TIMEOUT,
        );
        assert!(started.elapsed() < Duration::from_millis(1500));
    }

    #[tokio::test]
    async fn a_tip_stream_that_never_ends_fails_preparation() {
        let api = node(
            [BlockAnswer::StalledStream, BlockAnswer::StalledStream],
            SHORT_TIMEOUT,
        )
        .await;
        timed_out_after(
            ends(api.prepare("Nil", &DeployOptions::default())).await,
            SHORT_TIMEOUT,
        );
    }

    #[tokio::test]
    async fn one_good_tip_sample_is_enough_to_prepare() {
        for answers in [
            [BlockAnswer::Refused, PROMPT_TIP],
            [BlockAnswer::StalledStream, PROMPT_TIP],
            [PROMPT_TIP, BlockAnswer::StalledStream],
        ] {
            let api = node(answers, SHORT_TIMEOUT).await;
            api.prepare("Nil", &DeployOptions::default()).await.unwrap();
            assert_eq!(api.tip_floor.load(Ordering::Relaxed), TIP);
        }
    }

    #[tokio::test]
    async fn a_block_range_stream_that_never_ends_fails_within_the_request_timeout() {
        let api = node([BlockAnswer::StalledStream], SHORT_TIMEOUT).await;
        timed_out_after(ends(api.get_blocks_by_height(0, 10)).await, SHORT_TIMEOUT);
    }

    #[tokio::test]
    async fn a_block_range_reports_the_blocks_the_node_does_not_hold() {
        let node = MockNode::blocks([BlockAnswer::Missing {
            pruned_floor: 5,
            unavailable_floor: 5,
        }]);
        let api = node.serve(SHORT_TIMEOUT).await;
        let answer = api.get_blocks_by_height(0, 10).await.unwrap();
        assert_eq!(node.include_missing(), [true]);
        assert_eq!(answer.blocks.len(), 1);
        assert_eq!(
            answer.pruned,
            [MissingBlock {
                hash: "abcd".into(),
                height: 3
            }]
        );
        assert_eq!(
            answer.unavailable,
            [MissingBlock {
                hash: "ef01".into(),
                height: 9
            }]
        );
        assert_eq!(answer.prune_floor, Some(5));
    }

    #[tokio::test]
    async fn a_block_range_missing_nothing_has_no_prune_floor() {
        let api = node([PROMPT_TIP], SHORT_TIMEOUT).await;
        let answer = api.get_blocks_by_height(0, 10).await.unwrap();
        assert_eq!(answer.blocks.len(), 1);
        assert!(answer.pruned.is_empty() && answer.unavailable.is_empty());
        assert_eq!(answer.prune_floor, None);
    }

    #[tokio::test]
    async fn two_prune_floors_in_one_answer_are_malformed() {
        let api = node(
            [BlockAnswer::Missing {
                pruned_floor: 5,
                unavailable_floor: 6,
            }],
            SHORT_TIMEOUT,
        )
        .await;
        assert!(matches!(
            api.get_blocks_by_height(0, 10).await,
            Err(Error::Decode { .. })
        ));
    }

    #[tokio::test]
    async fn a_main_chain_carrying_a_missing_block_is_malformed() {
        let api = node(
            [BlockAnswer::Missing {
                pruned_floor: 5,
                unavailable_floor: 5,
            }],
            SHORT_TIMEOUT,
        )
        .await;
        assert!(matches!(
            api.show_main_chain(8).await,
            Err(Error::Decode { .. })
        ));
    }
}
