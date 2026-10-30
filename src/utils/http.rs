use crate::error::{Error, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::time::Duration;

/// The node's error body: `{"error": <code>, "message": <text>}`.
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    error: String,
    message: String,
}

pub(crate) async fn send(request: reqwest::RequestBuilder, url: &str) -> Result<reqwest::Response> {
    request.send().await.map_err(|e| Error::transport(url, e))
}

/// An [`Error::Http`] for a non-success response, carrying the node's error
/// code when the body is the node's error JSON.
pub(crate) async fn http_error(response: reqwest::Response, url: &str) -> Error {
    let status = response.status().as_u16();
    let body = match response.text().await {
        Ok(body) => body,
        Err(e) => return Error::transport(url, e),
    };
    match serde_json::from_str::<ApiErrorResponse>(&body) {
        Ok(api) => Error::Http {
            url: url.to_string(),
            status,
            code: Some(api.error),
            message: api.message,
        },
        Err(_) => Error::Http {
            url: url.to_string(),
            status,
            code: None,
            message: body,
        },
    }
}

pub(crate) async fn json<T: DeserializeOwned>(
    response: reqwest::Response,
    url: &str,
    what: &'static str,
) -> Result<T> {
    let body = response
        .bytes()
        .await
        .map_err(|e| Error::transport(url, e))?;
    serde_json::from_slice(&body).map_err(|e| Error::decode(what, e))
}

/// Whether `error` is the node's answer that it has no such deploy.
pub(crate) fn is_deploy_not_found(error: &Error) -> bool {
    matches!(error, Error::Http { code: Some(code), .. } if code == "deploy_not_found")
}

/// Backoff schedule for an exploratory query that the node rejected because its
/// exploratory-query capacity was occupied.
///
/// A node admits only `api-server.exploratory-deploy-max-concurrent` exploratory
/// queries (1 by default) and rejects the excess immediately instead of queueing
/// it, so a slot held by another client is a transient condition. The node
/// advertises a retry delay of `api-server.exploratory-deploy-execution-timeout`
/// (15 s by default), which is the worst case rather than the expected one: a
/// slot usually frees within about a second. These delays probe early and then
/// back off, for a total wait of about 30 s.
pub const EXPLORATORY_RETRY_BACKOFF: [Duration; 7] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(15),
];
