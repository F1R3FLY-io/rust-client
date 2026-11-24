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
    use crate::endpoint::Endpoint;
    use f1r3fly_models::casper::v1::{block_info_response, BlockInfoResponse};
    use f1r3fly_models::casper::BlocksQueryByHeight;
    use std::collections::VecDeque;
    use std::convert::Infallible;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};
    use std::time::{Duration, Instant};
    use tonic::body::Body;
    use tonic::codegen::{http, BoxFuture, BoxStream, Service};

    const TIP: i64 = 100;
    const SHORT_TIMEOUT: Duration = Duration::from_millis(300);

    /// How the mock node answers one block-stream call.
    #[derive(Clone, Copy)]
    enum Answer {
        /// Response headers after `headers`, the block at [`TIP`] after a
        /// further `block`, then the end of the stream.
        Tip { headers: Duration, block: Duration },
        /// Response headers at once, then nothing more.
        StalledStream,
        /// An `UNAVAILABLE` status.
        Refused,
        /// The block at [`TIP`], then [`PRUNED`] at height 3 reported with
        /// `pruned_floor`, then [`UNAVAILABLE`] at height 9 reported with
        /// `unavailable_floor`.
        Missing {
            pruned_floor: i64,
            unavailable_floor: i64,
        },
    }

    const PROMPT_TIP: Answer = Answer::Tip {
        headers: Duration::ZERO,
        block: Duration::ZERO,
    };
    const PRUNED: [u8; 2] = [0xab, 0xcd];
    const UNAVAILABLE: [u8; 2] = [0xef, 0x01];

    /// A node serving only `showMainChain` and `getBlocksByHeights`, giving
    /// each call the next scripted answer and recording each by-heights
    /// request's `include_missing`.
    #[derive(Clone)]
    struct BlockStreams {
        answers: Arc<Mutex<VecDeque<Answer>>>,
        include_missing: Arc<Mutex<Vec<bool>>>,
    }

    impl tonic::server::NamedService for BlockStreams {
        const NAME: &'static str = "casper.v1.DeployService";
    }

    impl Service<http::Request<Body>> for BlockStreams {
        type Response = http::Response<Body>;
        type Error = Infallible;
        type Future = BoxFuture<Self::Response, Infallible>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: http::Request<Body>) -> Self::Future {
            let answer = self
                .answers
                .lock()
                .unwrap()
                .pop_front()
                .expect("every call has a scripted answer");
            let blocks = Blocks {
                answer,
                include_missing: self.include_missing.clone(),
            };
            Box::pin(async move {
                Ok(match request.uri().path() {
                    "/casper.v1.DeployService/showMainChain" => {
                        tonic::server::Grpc::new(tonic_prost::ProstCodec::<
                            BlockInfoResponse,
                            BlocksQuery,
                        >::default())
                        .server_streaming(blocks, request)
                        .await
                    }
                    "/casper.v1.DeployService/getBlocksByHeights" => {
                        tonic::server::Grpc::new(tonic_prost::ProstCodec::<
                            BlockInfoResponse,
                            BlocksQueryByHeight,
                        >::default())
                        .server_streaming(blocks, request)
                        .await
                    }
                    _ => tonic::Status::unimplemented("").into_http(),
                })
            })
        }
    }

    struct Blocks {
        answer: Answer,
        include_missing: Arc<Mutex<Vec<bool>>>,
    }

    /// The `include_missing` of a block query, where it has one.
    trait IncludeMissing {
        fn include_missing(&self) -> Option<bool>;
    }

    impl IncludeMissing for BlocksQuery {
        fn include_missing(&self) -> Option<bool> {
            None
        }
    }

    impl IncludeMissing for BlocksQueryByHeight {
        fn include_missing(&self) -> Option<bool> {
            Some(self.include_missing)
        }
    }

    fn block(message: block_info_response::Message) -> Result<BlockInfoResponse, tonic::Status> {
        Ok(BlockInfoResponse {
            message: Some(message),
        })
    }

    fn tip_block() -> block_info_response::Message {
        block_info_response::Message::BlockInfo(LightBlockInfo {
            block_number: TIP,
            ..Default::default()
        })
    }

    impl<Q: IncludeMissing + Send + 'static> tonic::server::ServerStreamingService<Q> for Blocks {
        type Response = BlockInfoResponse;
        type ResponseStream = BoxStream<BlockInfoResponse>;
        type Future = BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;

        fn call(&mut self, request: tonic::Request<Q>) -> Self::Future {
            if let Some(include_missing) = request.get_ref().include_missing() {
                self.include_missing.lock().unwrap().push(include_missing);
            }
            let answer = self.answer;
            Box::pin(async move {
                let stream: Self::ResponseStream = match answer {
                    Answer::Refused => return Err(tonic::Status::unavailable("refused")),
                    Answer::StalledStream => Box::pin(futures_util::stream::pending()),
                    Answer::Tip {
                        headers,
                        block: delay,
                    } => {
                        tokio::time::sleep(headers).await;
                        Box::pin(futures_util::stream::once(async move {
                            tokio::time::sleep(delay).await;
                            block(tip_block())
                        }))
                    }
                    Answer::Missing {
                        pruned_floor,
                        unavailable_floor,
                    } => {
                        use f1r3fly_models::casper::v1::{BlockPrunedInfo, BlockUnavailableInfo};
                        Box::pin(futures_util::stream::iter([
                            block(tip_block()),
                            block(block_info_response::Message::Pruned(BlockPrunedInfo {
                                hash: PRUNED.to_vec().into(),
                                block_height: 3,
                                prune_floor: pruned_floor,
                            })),
                            block(block_info_response::Message::Unavailable(
                                BlockUnavailableInfo {
                                    hash: UNAVAILABLE.to_vec().into(),
                                    block_height: 9,
                                    prune_floor: unavailable_floor,
                                },
                            )),
                        ]))
                    }
                };
                Ok(tonic::Response::new(stream))
            })
        }
    }

    async fn node(
        answers: impl IntoIterator<Item = Answer>,
        request_timeout: Duration,
    ) -> F1r3flyApi {
        recording_node(answers, request_timeout).await.0
    }

    /// [`node`], with the `include_missing` of each by-heights request it
    /// receives.
    async fn recording_node(
        answers: impl IntoIterator<Item = Answer>,
        request_timeout: Duration,
    ) -> (F1r3flyApi, Arc<Mutex<Vec<bool>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let include_missing = Arc::new(Mutex::new(Vec::new()));
        let service = BlockStreams {
            answers: Arc::new(Mutex::new(answers.into_iter().collect())),
            include_missing: include_missing.clone(),
        };
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(service)
                .serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener)),
        );
        let endpoint = Endpoint::parse(&format!("http://127.0.0.1:{port}"))
            .unwrap()
            .with_request_timeout(request_timeout);
        (F1r3flyApi::new(endpoint).unwrap(), include_missing)
    }

    /// `call`'s output, failing the test instead of hanging it if the call
    /// outlives every timeout it should have.
    async fn ends<T>(call: impl std::future::Future<Output = T>) -> T {
        tokio::time::timeout(Duration::from_secs(5), call)
            .await
            .expect("the call ends within its request timeout")
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
        let slow = Answer::Tip {
            headers: Duration::from_millis(2500),
            block: Duration::ZERO,
        };
        let api = node([slow, slow], Duration::from_secs(5)).await;
        api.prepare("Nil", &DeployOptions::default()).await.unwrap();
        assert_eq!(api.tip_floor.load(Ordering::Relaxed), TIP);
    }

    #[tokio::test]
    async fn a_tip_stream_slower_than_the_request_timeout_fails_preparation() {
        let slow_body = Answer::Tip {
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
            [Answer::StalledStream, Answer::StalledStream],
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
            [Answer::Refused, PROMPT_TIP],
            [Answer::StalledStream, PROMPT_TIP],
            [PROMPT_TIP, Answer::StalledStream],
        ] {
            let api = node(answers, SHORT_TIMEOUT).await;
            api.prepare("Nil", &DeployOptions::default()).await.unwrap();
            assert_eq!(api.tip_floor.load(Ordering::Relaxed), TIP);
        }
    }

    #[tokio::test]
    async fn a_block_range_stream_that_never_ends_fails_within_the_request_timeout() {
        let api = node([Answer::StalledStream], SHORT_TIMEOUT).await;
        timed_out_after(ends(api.get_blocks_by_height(0, 10)).await, SHORT_TIMEOUT);
    }

    #[tokio::test]
    async fn a_block_range_reports_the_blocks_the_node_does_not_hold() {
        let (api, include_missing) = recording_node(
            [Answer::Missing {
                pruned_floor: 5,
                unavailable_floor: 5,
            }],
            SHORT_TIMEOUT,
        )
        .await;
        let answer = api.get_blocks_by_height(0, 10).await.unwrap();
        assert_eq!(*include_missing.lock().unwrap(), [true]);
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
            [Answer::Missing {
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
            [Answer::Missing {
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
