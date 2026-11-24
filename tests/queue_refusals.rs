use std::time::Duration;

use axum::{
    body::Body,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use node_cli::{
    ConnectionConfig, DeployAdmissionReason as Reason, DeployOptions, Endpoint, Error,
    F1r3flyConnectionManager, QueueCongestion, SignedDeploy, SigningKey, UnsignedDeploy,
};
use tokio::net::TcpListener;
use tonic::{Code, Status};

// Exercise the public client over HTTP/2 with the statuses emitted by the queue.
// Storage and shutdown faults are injected at the RPC boundary.
async fn fixture(status: Status) -> (F1r3flyConnectionManager, SignedDeploy) {
    let router = Router::new()
        .route(
            "/casper.v1.DeployService/doDeploy",
            post(move || async move { status.into_http::<Body>() }),
        )
        .route(
            "/api/deploy/{sig}",
            get(|| async { StatusCode::BAD_GATEWAY }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let key = SigningKey::from_hex(&"42".repeat(32)).unwrap();
    let signed = UnsignedDeploy::new("Nil", &DeployOptions::default(), 1, "root")
        .unwrap()
        .sign(key.secret_key());
    let config = ConnectionConfig::new(endpoint.clone(), endpoint, key);
    (F1r3flyConnectionManager::new(config).unwrap(), signed)
}

#[tokio::test]
async fn queue_capacity_and_shutdown_survive_grpc_transport() {
    const SHARD_FULL: &str =
        "deploy queue is full: the shard's finality is holding the validator back";

    for (code, message, wait, cause, reason, expected_congestion) in [
        (
            Code::ResourceExhausted,
            SHARD_FULL,
            Some(12),
            None,
            Reason::QueueFull,
            Some(QueueCongestion::Shard),
        ),
        (
            Code::ResourceExhausted,
            "deploy queue is full for this deployer",
            Some(3),
            None,
            Reason::DeployerShareFull,
            None,
        ),
        (
            Code::ResourceExhausted,
            SHARD_FULL,
            Some(9),
            Some("validator_idle"),
            Reason::QueueFull,
            Some(QueueCongestion::ValidatorIdle),
        ),
        (
            Code::ResourceExhausted,
            SHARD_FULL,
            Some(3),
            Some("deployer_share"),
            Reason::DeployerShareFull,
            None,
        ),
        (
            Code::ResourceExhausted,
            SHARD_FULL,
            Some(12),
            Some("future"),
            Reason::QueueFull,
            None,
        ),
        (
            Code::ResourceExhausted,
            SHARD_FULL,
            Some(12),
            Some(""),
            Reason::QueueFull,
            Some(QueueCongestion::Shard),
        ),
        (
            Code::Unavailable,
            "deploy queue is shutting down",
            None,
            None,
            Reason::QueueStopped,
            None,
        ),
    ] {
        let mut status = Status::new(code, message);
        if let Some(seconds) = wait {
            status
                .metadata_mut()
                .insert("retry-after", seconds.to_string().parse().unwrap());
        }
        if let Some(value) = cause {
            status
                .metadata_mut()
                .insert("cause", value.parse().unwrap());
        }
        let (manager, signed) = fixture(status).await;
        let error = manager.api().submit(&signed).await.unwrap_err();
        assert!(
            matches!(&error, Error::DeployAdmissionRefused {
            deploy_id, reason: actual, retry_after, congestion, message: detail,
        } if *deploy_id == signed.id() && *actual == reason && detail == message
            && *congestion == expected_congestion && *retry_after == wait.map(Duration::from_secs)),
            "cause {cause:?}: {error:?}"
        );
    }
}

#[tokio::test]
async fn queue_storage_failure_preserves_the_grpc_status() {
    let (manager, signed) = fixture(Status::internal("queue store failed")).await;
    let error = manager.submit_and_wait(&signed).await.unwrap_err();
    assert!(matches!(error, Error::Grpc { status, .. }
        if status.code() == Code::Internal && status.message() == "queue store failed"));
}

#[tokio::test]
async fn unavailable_queue_and_failed_lookup_preserve_both_errors_without_an_observer() {
    let mut status = Status::unavailable("deploy queue is not connected to its node");
    status
        .metadata_mut()
        .insert("retry-after", "5".parse().unwrap());
    let (manager, signed) = fixture(status).await;
    let error = manager.submit_and_wait(&signed).await.unwrap_err();
    let Error::DeployOutcomeUnknown {
        deploy_id,
        refusal,
        lookup,
    } = error
    else {
        panic!("expected an unknown outcome: {error}");
    };
    assert_eq!(deploy_id, signed.id());
    assert!(matches!(*refusal, Error::DeployAdmissionRefused {
        reason: Reason::QueueNodeUnavailable, retry_after: Some(wait), ..
    } if wait == Duration::from_secs(5)));
    assert!(matches!(*lookup, Error::Http { status: 502, url, .. }
        if url.ends_with(&format!("/api/deploy/{deploy_id}"))));
}
