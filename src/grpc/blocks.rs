//! Block queries, finalization checks, and tip sampling

use super::{rejected, F1r3flyApi};
use crate::error::{Error, Result, TransportError};
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
        self.within_request_timeout(async {
            let stream = client
                .show_main_chain(query)
                .await
                .map_err(|status| self.grpc_error(status))?
                .into_inner();
            self.collect_blocks("showMainChain", stream).await
        })
        .await
    }

    pub async fn get_blocks_by_height(
        &self,
        start_block_number: i64,
        end_block_number: i64,
    ) -> Result<Vec<LightBlockInfo>> {
        let mut client = self.deploy_client().await?;
        let query = BlocksQueryByHeight {
            start_block_number,
            end_block_number,
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
    ) -> Result<Vec<LightBlockInfo>> {
        use f1r3fly_models::casper::v1::block_info_response::Message;

        let mut blocks = Vec::new();
        while let Some(response) = stream
            .message()
            .await
            .map_err(|status| self.grpc_error(status))?
        {
            match response.message {
                Some(Message::Error(error)) => return Err(rejected(operation, &error)),
                Some(Message::BlockInfo(block_info)) => blocks.push(block_info),
                None => return Err(Error::decode("block stream", "empty message")),
            }
        }
        Ok(blocks)
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
    }

    const PROMPT_TIP: Answer = Answer::Tip {
        headers: Duration::ZERO,
        block: Duration::ZERO,
    };

    /// A node serving only `showMainChain` and `getBlocksByHeights`, giving
    /// each call the next scripted answer.
    #[derive(Clone)]
    struct BlockStreams(Arc<Mutex<VecDeque<Answer>>>);

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
                .0
                .lock()
                .unwrap()
                .pop_front()
                .expect("every call has a scripted answer");
            Box::pin(async move {
                Ok(match request.uri().path() {
                    "/casper.v1.DeployService/showMainChain" => {
                        tonic::server::Grpc::new(tonic_prost::ProstCodec::<
                            BlockInfoResponse,
                            BlocksQuery,
                        >::default())
                        .server_streaming(Blocks(answer), request)
                        .await
                    }
                    "/casper.v1.DeployService/getBlocksByHeights" => {
                        tonic::server::Grpc::new(tonic_prost::ProstCodec::<
                            BlockInfoResponse,
                            BlocksQueryByHeight,
                        >::default())
                        .server_streaming(Blocks(answer), request)
                        .await
                    }
                    _ => tonic::Status::unimplemented("").into_http(),
                })
            })
        }
    }

    struct Blocks(Answer);

    impl<Q: Send + 'static> tonic::server::ServerStreamingService<Q> for Blocks {
        type Response = BlockInfoResponse;
        type ResponseStream = BoxStream<BlockInfoResponse>;
        type Future = BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;

        fn call(&mut self, _: tonic::Request<Q>) -> Self::Future {
            let answer = self.0;
            Box::pin(async move {
                let stream: Self::ResponseStream = match answer {
                    Answer::Refused => return Err(tonic::Status::unavailable("refused")),
                    Answer::StalledStream => Box::pin(futures_util::stream::pending()),
                    Answer::Tip { headers, block } => {
                        tokio::time::sleep(headers).await;
                        Box::pin(futures_util::stream::once(async move {
                            tokio::time::sleep(block).await;
                            Ok(BlockInfoResponse {
                                message: Some(block_info_response::Message::BlockInfo(
                                    LightBlockInfo {
                                        block_number: TIP,
                                        ..Default::default()
                                    },
                                )),
                            })
                        }))
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let service = BlockStreams(Arc::new(Mutex::new(answers.into_iter().collect())));
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(service)
                .serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener)),
        );
        let endpoint = Endpoint::parse(&format!("http://127.0.0.1:{port}"))
            .unwrap()
            .with_request_timeout(request_timeout);
        F1r3flyApi::new(endpoint).unwrap()
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
}
