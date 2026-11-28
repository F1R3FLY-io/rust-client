//! The deploy parameters the node advertises, from `DeployServiceV1.Status`.

use super::{rejected, F1r3flyApi};
use crate::error::{Error, Result};
use crate::f1r3fly_api::DeployParameters;
use f1r3fly_models::casper::v1::status_response;

impl F1r3flyApi {
    /// The price and phlo cap this node enforces, read over the same gRPC
    /// connection deploys use.
    ///
    /// The values are optional because a node older than the fields, or one
    /// that has not adopted the shard's parameters yet, does not advertise
    /// them.
    pub async fn deploy_parameters(&self) -> Result<DeployParameters> {
        let mut client = self.deploy_client().await?;
        let response = client
            .status(())
            .await
            .map_err(|status| self.grpc_error(status))?
            .into_inner();

        let status = match response.message {
            Some(status_response::Message::Status(status)) => status,
            Some(status_response::Message::Error(error)) => {
                return Err(rejected("the status request", &error))
            }
            None => return Err(Error::decode("status response", "empty message")),
        };

        let min_phlo_price = if status.min_phlo_price.is_empty() {
            None
        } else {
            Some(
                crate::amount::uint256(&status.min_phlo_price)
                    .map_err(|error| Error::decode("minPhloPrice", error))?,
            )
        };
        let max_phlo_limit = match status.max_phlo_limit {
            None => None,
            Some(cap) if cap <= 0 => return Err(Error::decode("maxPhloLimit", cap)),
            Some(cap) => Some(cap as u64),
        };

        Ok(DeployParameters {
            endpoint: self.endpoint().to_string(),
            min_phlo_price,
            max_phlo_limit,
            native_token_decimals: status.native_token_decimals,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint::Endpoint;
    use f1r3fly_models::casper::v1::StatusResponse;
    use f1r3fly_models::casper::Status;
    use f1r3fly_models::servicemodelapi::ServiceError;
    use std::convert::Infallible;
    use std::future::Future;
    use std::task::{Context, Poll};
    use tonic::body::Body;
    use tonic::codegen::{http, BoxFuture, Service};

    /// A node serving only `status`.
    #[derive(Clone)]
    struct StatusOnly {
        response: StatusResponse,
    }

    impl tonic::server::NamedService for StatusOnly {
        const NAME: &'static str = "casper.v1.DeployService";
    }

    impl Service<http::Request<Body>> for StatusOnly {
        type Response = http::Response<Body>;
        type Error = Infallible;
        type Future = BoxFuture<Self::Response, Infallible>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: http::Request<Body>) -> Self::Future {
            let response = self.response.clone();
            Box::pin(async move {
                Ok(match request.uri().path() {
                    "/casper.v1.DeployService/status" => {
                        tonic::server::Grpc::new(
                            tonic_prost::ProstCodec::<StatusResponse, ()>::default(),
                        )
                        .unary(StatusCall { response }, request)
                        .await
                    }
                    _ => tonic::Status::unimplemented("").into_http(),
                })
            })
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

    async fn node(response: StatusResponse) -> F1r3flyApi {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(
            tonic::transport::Server::builder()
                .add_service(StatusOnly { response })
                .serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener)),
        );
        let endpoint = Endpoint::parse(&format!("http://127.0.0.1:{port}")).unwrap();
        F1r3flyApi::new(endpoint).unwrap()
    }

    fn answering(status: Status) -> StatusResponse {
        StatusResponse {
            message: Some(status_response::Message::Status(status)),
        }
    }

    /// `call`'s output, failing the test instead of hanging it if the call
    /// outlives every timeout it should have.
    async fn ends<T>(call: impl Future<Output = T>) -> T {
        tokio::time::timeout(std::time::Duration::from_secs(5), call)
            .await
            .expect("the call ends")
    }

    #[tokio::test]
    async fn deploy_parameters_come_from_the_grpc_status() {
        let api = node(answering(Status {
            min_phlo_price: "18446744073709551617".to_string(),
            max_phlo_limit: Some(20_000_000),
            native_token_decimals: 18,
            ..Default::default()
        }))
        .await;

        let parameters = ends(api.deploy_parameters()).await.unwrap();
        assert_eq!(
            parameters.min_phlo_price,
            Some(18_446_744_073_709_551_617u128.into())
        );
        assert_eq!(parameters.max_phlo_limit, Some(20_000_000));
        assert_eq!(parameters.phlo_limit_cap().unwrap(), 20_000_000);
        assert_eq!(parameters.native_token_decimals, 18);
        assert_eq!(parameters.endpoint, api.endpoint().to_string());
    }

    #[tokio::test]
    async fn a_status_without_the_phlo_fields_leaves_them_unset() {
        let api = node(answering(Status::default())).await;

        let parameters = ends(api.deploy_parameters()).await.unwrap();
        assert_eq!(parameters.min_phlo_price, None);
        assert_eq!(parameters.max_phlo_limit, None);
        assert!(matches!(
            parameters.phlo_limit_cap(),
            Err(Error::NodeNotReady {
                parameter: "maxPhloLimit",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn a_malformed_status_is_refused() {
        for status in [
            Status {
                min_phlo_price: "-1".to_string(),
                ..Default::default()
            },
            Status {
                max_phlo_limit: Some(-1),
                ..Default::default()
            },
        ] {
            let api = node(answering(status)).await;
            assert!(matches!(
                ends(api.deploy_parameters()).await,
                Err(Error::Decode { .. })
            ));
        }
    }

    #[tokio::test]
    async fn a_node_refusal_is_reported_as_such() {
        let api = node(StatusResponse {
            message: Some(status_response::Message::Error(ServiceError {
                messages: vec!["not ready".to_string()],
            })),
        })
        .await;

        assert!(matches!(
            ends(api.deploy_parameters()).await,
            Err(Error::NodeRejected {
                operation: "the status request",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn a_silent_node_fails_the_status_call_within_its_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let endpoint = Endpoint::parse(&format!("http://127.0.0.1:{port}"))
            .unwrap()
            .with_request_timeout(std::time::Duration::from_millis(300));
        let api = F1r3flyApi::new(endpoint).unwrap();

        let started = std::time::Instant::now();
        assert!(ends(api.deploy_parameters()).await.is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
