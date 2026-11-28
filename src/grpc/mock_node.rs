//! A node for the gRPC client's tests: it answers block streams from a script
//! and `status` with a fixed response, on a local port.

use super::F1r3flyApi;
use crate::endpoint::Endpoint;
use f1r3fly_models::casper::v1::{
    block_info_response, BlockInfoResponse, BlockPrunedInfo, BlockUnavailableInfo, StatusResponse,
};
use f1r3fly_models::casper::{BlocksQuery, BlocksQueryByHeight, LightBlockInfo};
use std::collections::VecDeque;
use std::convert::Infallible;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tonic::body::Body;
use tonic::codegen::{http, BoxFuture, BoxStream, Service};

/// The height of the block a [`BlockAnswer::Tip`] streams.
pub(crate) const TIP: i64 = 100;
/// The hash a [`BlockAnswer::Missing`] reports pruned, at height 3.
pub(crate) const PRUNED: [u8; 2] = [0xab, 0xcd];
/// The hash a [`BlockAnswer::Missing`] reports unavailable, at height 9.
pub(crate) const UNAVAILABLE: [u8; 2] = [0xef, 0x01];

/// How the node answers one block-stream call.
#[derive(Clone, Copy)]
pub(crate) enum BlockAnswer {
    /// Response headers after `headers`, the block at [`TIP`] after a further
    /// `block`, then the end of the stream.
    Tip { headers: Duration, block: Duration },
    /// Response headers at once, then nothing more.
    StalledStream,
    /// An `UNAVAILABLE` status.
    Refused,
    /// The block at [`TIP`], then [`PRUNED`] reported with `pruned_floor`, then
    /// [`UNAVAILABLE`] reported with `unavailable_floor`.
    Missing {
        pruned_floor: i64,
        unavailable_floor: i64,
    },
}

pub(crate) const PROMPT_TIP: BlockAnswer = BlockAnswer::Tip {
    headers: Duration::ZERO,
    block: Duration::ZERO,
};

/// The node's script. Each block-stream call takes the next block answer;
/// `status` answers with the status response, or `UNIMPLEMENTED` without one.
#[derive(Clone, Default)]
pub(crate) struct MockNode {
    blocks: Arc<Mutex<VecDeque<BlockAnswer>>>,
    status: Option<StatusResponse>,
    include_missing: Arc<Mutex<Vec<bool>>>,
}

impl MockNode {
    pub(crate) fn blocks(answers: impl IntoIterator<Item = BlockAnswer>) -> Self {
        MockNode {
            blocks: Arc::new(Mutex::new(answers.into_iter().collect())),
            ..MockNode::default()
        }
    }

    pub(crate) fn status(response: StatusResponse) -> Self {
        MockNode {
            status: Some(response),
            ..MockNode::default()
        }
    }

    /// The `include_missing` of each by-heights request received so far.
    pub(crate) fn include_missing(&self) -> Vec<bool> {
        self.include_missing.lock().unwrap().clone()
    }

    /// Serve the script on a local port, returning a client for it.
    pub(crate) async fn serve(&self, request_timeout: Duration) -> F1r3flyApi {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(self.clone())
                .serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener)),
        );
        let endpoint = Endpoint::parse(&format!("http://127.0.0.1:{port}"))
            .unwrap()
            .with_request_timeout(request_timeout);
        F1r3flyApi::new(endpoint).unwrap()
    }
}

/// `call`'s output, failing the test instead of hanging it if the call
/// outlives every timeout it should have.
pub(crate) async fn ends<T>(call: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), call)
        .await
        .expect("the call ends within its request timeout")
}

impl tonic::server::NamedService for MockNode {
    const NAME: &'static str = "casper.v1.DeployService";
}

impl Service<http::Request<Body>> for MockNode {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Infallible>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: http::Request<Body>) -> Self::Future {
        let node = self.clone();
        Box::pin(async move {
            Ok(match request.uri().path() {
                "/casper.v1.DeployService/showMainChain" => {
                    tonic::server::Grpc::new(tonic_prost::ProstCodec::<
                        BlockInfoResponse,
                        BlocksQuery,
                    >::default())
                    .server_streaming(node.next_blocks(), request)
                    .await
                }
                "/casper.v1.DeployService/getBlocksByHeights" => {
                    tonic::server::Grpc::new(tonic_prost::ProstCodec::<
                        BlockInfoResponse,
                        BlocksQueryByHeight,
                    >::default())
                    .server_streaming(node.next_blocks(), request)
                    .await
                }
                "/casper.v1.DeployService/status" => match node.status {
                    Some(response) => {
                        tonic::server::Grpc::new(
                            tonic_prost::ProstCodec::<StatusResponse, ()>::default(),
                        )
                        .unary(StatusCall { response }, request)
                        .await
                    }
                    None => tonic::Status::unimplemented("").into_http(),
                },
                _ => tonic::Status::unimplemented("").into_http(),
            })
        })
    }
}

impl MockNode {
    fn next_blocks(&self) -> Blocks {
        Blocks {
            answer: self
                .blocks
                .lock()
                .unwrap()
                .pop_front()
                .expect("every block-stream call has a scripted answer"),
            include_missing: self.include_missing.clone(),
        }
    }
}

struct StatusCall {
    response: StatusResponse,
}

impl tonic::server::UnaryService<()> for StatusCall {
    type Response = StatusResponse;
    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

    fn call(&mut self, _: tonic::Request<()>) -> Self::Future {
        let response = self.response.clone();
        Box::pin(async move { Ok(tonic::Response::new(response)) })
    }
}

struct Blocks {
    answer: BlockAnswer,
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
                BlockAnswer::Refused => return Err(tonic::Status::unavailable("refused")),
                BlockAnswer::StalledStream => Box::pin(futures_util::stream::pending()),
                BlockAnswer::Tip {
                    headers,
                    block: delay,
                } => {
                    tokio::time::sleep(headers).await;
                    Box::pin(futures_util::stream::once(async move {
                        tokio::time::sleep(delay).await;
                        block(tip_block())
                    }))
                }
                BlockAnswer::Missing {
                    pruned_floor,
                    unavailable_floor,
                } => Box::pin(futures_util::stream::iter([
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
                ])),
            };
            Ok(tonic::Response::new(stream))
        })
    }
}
