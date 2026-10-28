//! HTTP-based methods on F1r3flyApi (deploy lookup, deploy detail, cost estimation)

use super::F1r3flyApi;
use crate::error::{Error, Result};
use crate::f1r3fly_api::{DeployDetail, DeployFinalizationStatus};
use crate::utils::http::{http_error, is_deploy_not_found, json, send};
use serde::{Deserialize, Serialize};

impl<'a> F1r3flyApi<'a> {
    /// GET `url`, answering `None` when the node has no such deploy.
    async fn get_deploy_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        what: &'static str,
    ) -> Result<Option<T>> {
        let response = send(reqwest::Client::new().get(url), url).await?;
        if !response.status().is_success() {
            let error = http_error(response, url).await;
            return if is_deploy_not_found(&error) {
                Ok(None)
            } else {
                Err(error)
            };
        }
        json(response, url, what).await.map(Some)
    }

    fn deploy_url(&self, deploy_id: &str, http_port: u16) -> String {
        format!(
            "http://{}:{}/api/deploy/{}",
            self.node_host, http_port, deploy_id
        )
    }

    pub async fn get_deploy_block_hash(
        &self,
        deploy_id: &str,
        http_port: u16,
    ) -> Result<Option<String>> {
        let url = self.deploy_url(deploy_id, http_port);
        let Some(deploy) = self
            .get_deploy_json::<serde_json::Value>(&url, "deploy")
            .await?
        else {
            return Ok(None);
        };
        deploy
            .get("blockHash")
            .and_then(|v| v.as_str())
            .map(|block_hash| Some(block_hash.to_string()))
            .ok_or_else(|| Error::decode("deploy", "a found deploy carries no blockHash"))
    }

    pub async fn get_deploy_detail(
        &self,
        deploy_id: &str,
        http_port: u16,
    ) -> Result<Option<DeployDetail>> {
        let url = self.deploy_url(deploy_id, http_port);
        self.get_deploy_json(&url, "deploy detail").await
    }

    /// Query `/api/deploy-finalization-status/{deploy_sig_hex}` for canonical
    /// finalization state of a deploy.
    ///
    /// Returns `Ok(Some(status))` on 200 and `Ok(None)` on 404: the node does
    /// not serve the endpoint, since an unknown deploy answers 200 `Pending`.
    pub async fn deploy_finalization_status(
        &self,
        deploy_sig_hex: &str,
        http_port: u16,
    ) -> Result<Option<DeployFinalizationStatus>> {
        let url = format!(
            "http://{}:{}/api/deploy-finalization-status/{}",
            self.node_host, http_port, deploy_sig_hex
        );
        let response = send(reqwest::Client::new().get(&url), &url).await?;
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

    /// Get deploy info using the default view (works on all nodes).
    /// Returns raw JSON with block metadata (blockHash, seqNum, blockNumber, etc.)
    pub async fn get_deploy_default(
        &self,
        deploy_id: &str,
        http_port: u16,
    ) -> Result<Option<serde_json::Value>> {
        let url = self.deploy_url(deploy_id, http_port);
        self.get_deploy_json(&url, "deploy").await
    }

    /// Estimate phlogiston cost via `POST /api/estimate-cost`.
    ///
    /// When `deployer` is `Some`, the node executes the term under that identity,
    /// producing an accurate cost for identity-dependent contracts (e.g. vault
    /// transfers). When `None`, the estimate may be significantly lower than the
    /// real deploy cost.
    ///
    /// `block_hash` is optional; when supplied the estimate runs against that
    /// block's state.
    pub async fn estimate_cost(
        &self,
        term: &str,
        deployer: Option<&str>,
        block_hash: Option<&str>,
        http_port: u16,
    ) -> Result<u64> {
        let mut url = format!("http://{}:{}/api/estimate-cost", self.node_host, http_port);
        if let Some(bh) = block_hash {
            url.push_str(&format!("?block_hash={bh}"));
        }

        let body = EstimateCostRequest { term, deployer };
        let response = send(reqwest::Client::new().post(&url).json(&body), &url).await?;
        if !response.status().is_success() {
            return Err(http_error(response, &url).await);
        }
        let result: EstimateCostResponse = json(response, &url, "cost estimate").await?;
        Ok(result.cost)
    }
}

/// Request body for `POST /api/estimate-cost`.
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
