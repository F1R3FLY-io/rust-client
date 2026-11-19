use axum::{
    extract::WebSocketUpgrade,
    routing::{get, post},
    Json, Router,
};
use node_cli::{
    DeployAdmissionReason, DeployOptions, Endpoint, Error, F1r3flyApi, SigningKey, UnsignedDeploy,
};
use serde_json::{json, Value};
use std::time::Duration;

// scripts/test-deploy-queue.sh starts the production queue. A fixed tip and
// an open event stream make its window and per-deployer budget deterministic.
#[tokio::test]
async fn real_queue_refuses_a_short_window_and_a_full_deployer_share() {
    let validator = Router::new()
        .route(
            "/api/status",
            get(|| async {
                Json(json!({
                    "shardId": "root", "minPhloPrice": "1", "latestBlockNumber": 101,
                    "lastFinalizedBlockNumber": 99, "deployLifespan": 50,
                    "maxUserDeploysPerBlock": 4, "maxParentDepth": 25,
                    "isValidator": true, "isReadOnly": false
                }))
            }),
        )
        .route(
            "/ws/events",
            get(|ws: WebSocketUpgrade| async {
                ws.on_upgrade(|mut socket| async move { while socket.recv().await.is_some() {} })
            }),
        )
        .route("/api/deploy", post(|| async { Json("Success!") }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:40583")
        .await
        .unwrap();
    tokio::spawn(async move { axum::serve(listener, validator).await.unwrap() });
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let Ok(response) = reqwest::get("http://127.0.0.1:40573/queue/status").await {
                let status: Value = response.json().await.unwrap();
                if status["connected"] == true {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("queue did not connect to the validator fixture");

    let api = F1r3flyApi::new(Endpoint::parse("http://127.0.0.1:40571").unwrap()).unwrap();
    let key = SigningKey::from_hex(&"42".repeat(32)).unwrap();
    let sign = |height| {
        UnsignedDeploy::new("Nil", &DeployOptions::default(), height, "root")
            .unwrap()
            .sign(key.secret_key())
    };
    let short_window = sign(60);
    let refusal = api.submit(&short_window).await.unwrap_err();
    assert!(matches!(refusal, Error::DeployAdmissionRefused {
        reason: DeployAdmissionReason::QueueFull, deploy_id, retry_after: Some(_), congestion: Some(_), ..
    } if deploy_id == short_window.id()));

    // Half of the 24 available slots belong to a single deployer.
    for _ in 0..12 {
        let deploy = sign(100);
        assert_eq!(api.submit(&deploy).await.unwrap(), deploy.id());
    }
    let excess = sign(100);
    let refusal = api.submit(&excess).await.unwrap_err();
    assert!(matches!(refusal, Error::DeployAdmissionRefused {
        reason: DeployAdmissionReason::DeployerShareFull, deploy_id, retry_after: Some(_), congestion: None, ..
    } if deploy_id == excess.id()));
}
