//! Client for a node's HTTP API.

use serde::{Deserialize, Serialize};

use crate::endpoint::Endpoint;
use crate::error::{Error, Result};
use crate::f1r3fly_api::{DeployDetail, DeployFinalizationStatus};
use crate::utils::http::{http_error, is_deploy_not_found, json, send, EXPLORATORY_RETRY_BACKOFF};

/// A node's HTTP API at one endpoint. Cloning shares the connection pool.
#[derive(Clone, Debug)]
pub struct NodeHttp {
    endpoint: Endpoint,
    client: reqwest::Client,
}

impl NodeHttp {
    pub fn new(endpoint: Endpoint) -> Result<Self> {
        let client = endpoint.http_client()?;
        Ok(NodeHttp { endpoint, client })
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// GET `path` as JSON.
    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        let url = self.endpoint.join(path);
        let response = send(self.client.get(&url), &url).await?;
        if !response.status().is_success() {
            return Err(http_error(response, &url).await);
        }
        json(response, &url, "JSON response").await
    }

    /// GET `path` as text.
    pub async fn get_text(&self, path: &str) -> Result<String> {
        let url = self.endpoint.join(path);
        let response = send(self.client.get(&url), &url).await?;
        if !response.status().is_success() {
            return Err(http_error(response, &url).await);
        }
        response.text().await.map_err(|e| Error::transport(url, e))
    }

    /// GET `path`, answering `None` when the node has no such deploy.
    async fn get_deploy_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &'static str,
    ) -> Result<Option<T>> {
        let url = self.endpoint.join(path);
        let response = send(self.client.get(&url), &url).await?;
        if !response.status().is_success() {
            let error = http_error(response, &url).await;
            return if is_deploy_not_found(&error) {
                Ok(None)
            } else {
                Err(error)
            };
        }
        json(response, &url, what).await.map(Some)
    }

    /// The deploy as `/api/deploy/{id}` describes it.
    pub async fn get_deploy(&self, deploy_id: &str) -> Result<Option<serde_json::Value>> {
        self.get_deploy_json(&format!("/api/deploy/{deploy_id}"), "deploy")
            .await
    }

    pub async fn get_deploy_detail(&self, deploy_id: &str) -> Result<Option<DeployDetail>> {
        self.get_deploy_json(&format!("/api/deploy/{deploy_id}"), "deploy detail")
            .await
    }

    pub async fn get_deploy_block_hash(&self, deploy_id: &str) -> Result<Option<String>> {
        let Some(deploy) = self.get_deploy(deploy_id).await? else {
            return Ok(None);
        };
        deploy
            .get("blockHash")
            .and_then(|v| v.as_str())
            .map(|block_hash| Some(block_hash.to_string()))
            .ok_or_else(|| Error::decode("deploy", "a found deploy carries no blockHash"))
    }

    /// Canonical finalization state of a deploy, from
    /// `/api/deploy-finalization-status/{deploy_sig_hex}`.
    ///
    /// Returns `Ok(None)` on 404: the node does not serve the endpoint, since an
    /// unknown deploy answers 200 `Pending`.
    pub async fn deploy_finalization_status(
        &self,
        deploy_sig_hex: &str,
    ) -> Result<Option<DeployFinalizationStatus>> {
        let url = self
            .endpoint
            .join(&format!("/api/deploy-finalization-status/{deploy_sig_hex}"));
        let response = send(self.client.get(&url), &url).await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(http_error(response, &url).await);
        }
        json(response, &url, "deploy finalization status")
            .await
            .map(Some)
    }

    /// The error for an endpoint `feature` that answered 404. A node that
    /// answers `/api/status` is too old to serve `feature`; one that does not
    /// is the wrong endpoint, such as a mistyped base path, and that failure is
    /// returned instead.
    pub(crate) async fn unserved(&self, feature: &'static str, requirement: &'static str) -> Error {
        match self.get_json("/api/status").await {
            Ok(_) => Error::NodeTooOld {
                endpoint: self.endpoint.to_string(),
                feature,
                requirement,
            },
            Err(error) => error,
        }
    }

    /// [`unserved`](Self::unserved) for `/api/deploy-finalization-status`.
    pub(crate) async fn finalization_status_unserved(&self) -> Error {
        self.unserved(
            "/api/deploy-finalization-status",
            "f1r3node-rust v0.4.15 or later",
        )
        .await
    }

    /// Estimate phlogiston cost via `POST /api/estimate-cost`.
    ///
    /// When `deployer` is `Some`, the node executes the term under that identity,
    /// producing an accurate cost for identity-dependent contracts (e.g. vault
    /// transfers). When `None`, the estimate may be significantly lower than the
    /// real deploy cost. With `block_hash`, the estimate runs against that
    /// block's state.
    pub async fn estimate_cost(
        &self,
        term: &str,
        deployer: Option<&str>,
        block_hash: Option<&str>,
    ) -> Result<u64> {
        let mut url = self.endpoint.join("/api/estimate-cost");
        if let Some(block_hash) = block_hash {
            url.push_str(&format!("?block_hash={block_hash}"));
        }
        let body = EstimateCostRequest { term, deployer };
        let response = send(self.client.post(&url).json(&body), &url).await?;
        if !response.status().is_success() {
            return Err(http_error(response, &url).await);
        }
        let result: EstimateCostResponse = json(response, &url, "cost estimate").await?;
        Ok(result.cost)
    }

    /// Run `term` read-only through `POST /api/explore-deploy`, which only a
    /// read-only node serves.
    ///
    /// A node admits only `api-server.exploratory-deploy-max-concurrent`
    /// exploratory queries and answers the excess with 503 instead of queueing
    /// it, so an occupied slot is retried on [`EXPLORATORY_RETRY_BACKOFF`].
    pub async fn explore_deploy(&self, term: &str) -> Result<serde_json::Value> {
        let url = self.endpoint.join("/api/explore-deploy");
        let body = serde_json::json!({ "term": term });
        let mut backoff = EXPLORATORY_RETRY_BACKOFF.iter();
        loop {
            let response = send(self.client.post(&url).json(&body), &url).await?;
            let status = response.status();
            if status.is_success() {
                return json(response, &url, "exploratory deploy").await;
            }
            match backoff.next() {
                Some(delay) if status == reqwest::StatusCode::SERVICE_UNAVAILABLE => {
                    tokio::time::sleep(*delay).await;
                }
                _ => return Err(http_error(response, &url).await),
            }
        }
    }
}

#[derive(Serialize)]
struct EstimateCostRequest<'a> {
    term: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployer: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct EstimateCostResponse {
    cost: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// An HTTP server answering `/api/status` with `status_code` and every
    /// other path with 404.
    async fn server(status_code: u16) -> NodeHttp {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = [0u8; 1024];
                let read = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..read]);
                let (code, body) = if request.starts_with("GET /api/status ") {
                    (status_code, "{}")
                } else {
                    (404, "")
                };
                let response = format!(
                    "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        NodeHttp::new(Endpoint::parse(&format!("http://127.0.0.1:{port}")).unwrap()).unwrap()
    }

    #[tokio::test]
    async fn a_404_from_a_live_node_means_it_is_too_old() {
        let node = server(200).await;
        assert!(matches!(
            node.unserved("/api/x", "v1").await,
            Error::NodeTooOld {
                feature: "/api/x",
                ..
            }
        ));
    }

    #[tokio::test]
    async fn a_404_from_something_that_is_not_a_node_names_the_endpoint() {
        let node = server(404).await;
        assert!(matches!(
            node.unserved("/api/x", "v1").await,
            Error::Http { status: 404, ref url, .. } if url.ends_with("/api/status")
        ));
    }
}
