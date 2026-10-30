//! Transfer reporting against a deterministic HTTP fixture; no node required.

use std::process::Output;
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::Command;

/// Serve `body` to one request and run `block-transfers` against it, returning
/// the command's output and the request line the fixture received.
async fn block_transfers_against(body: String, extra_args: &[&str]) -> (Output, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut request = String::new();
        stream.read_line(&mut request).await.unwrap();
        loop {
            let mut header = String::new();
            assert_ne!(stream.read_line(&mut header).await.unwrap(), 0);
            if header == "\r\n" {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .get_mut()
            .write_all(response.as_bytes())
            .await
            .unwrap();
        request
    });

    tokio::time::timeout(Duration::from_secs(10), async {
        let output = Command::new(env!("CARGO_BIN_EXE_node_cli"))
            .args(["block-transfers", "test-block", "--http"])
            .arg(format!("http://127.0.0.1:{port}"))
            .args(extra_args)
            .env("NO_PROXY", "127.0.0.1")
            .env("no_proxy", "127.0.0.1")
            .kill_on_drop(true)
            .output()
            .await
            .unwrap();
        (output, server.await.unwrap())
    })
    .await
    .expect("block-transfers fixture timed out")
}

#[tokio::test]
async fn failed_transfer_does_not_abort_the_block_report() {
    let body = json!({
        "blockInfo": {"blockNumber": 42},
        "deploys": [{
            "sig": "test-deploy",
            "transfers": [
                {
                    "fromAddr": "sender",
                    "toAddr": "recipient",
                    "amount": -1,
                    "success": false,
                    "failReason": "Amount must be positive value"
                },
                {
                    "fromAddr": "sender",
                    "toAddr": "recipient",
                    "amount": "18446744073709551617",
                    "success": true,
                    "failReason": ""
                }
            ]
        }]
    })
    .to_string();
    let (output, request) = block_transfers_against(body, &[]).await;

    assert_eq!(request, "GET /api/block/test-block HTTP/1.1\r\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    for expected in [
        "Amount: -1",
        "Status: Failed: Amount must be positive value",
        "Amount: 18446744073709551617",
        "Status: Success",
        "Total deploys in block: 1",
        "Deploys with transfers: 1",
        "Total transfers: 2",
        "Successful: 1",
        "Failed: 1",
    ] {
        assert!(stdout.contains(expected), "missing {expected:?}:\n{stdout}");
    }
}

#[tokio::test]
async fn a_pending_transfer_report_is_an_error_not_zero_transfers() {
    let body = json!({
        "blockInfo": {"blockNumber": 42},
        "deploys": [{"sig": "test-deploy"}]
    })
    .to_string();
    let (output, _) = block_transfers_against(body, &["--max-wait", "0"]).await;

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success(), "{stdout}\n{stderr}");
    assert!(
        format!("{stdout}{stderr}").contains("has not produced the transfer report"),
        "{stdout}\n{stderr}"
    );
    assert!(
        format!("{stdout}{stderr}").contains("only a read-only node attaches transfers"),
        "{stdout}\n{stderr}"
    );
    assert!(!stdout.contains("Total transfers: 0"), "{stdout}");
}
