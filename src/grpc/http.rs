//! HTTP-based methods on F1r3flyApi (deploy lookup, deploy detail, cost estimation)

use super::F1r3flyApi;
use crate::f1r3fly_api::{DeployDetail, DeployFinalizationStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ApiErrorResponse {
    pub error: String,
    pub message: String,
}

impl<'a> F1r3flyApi<'a> {
    pub async fn get_deploy_block_hash(
        &self,
        deploy_id: &str,
        http_port: u16,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let url = format!(
            "http://{}:{}/api/deploy/{}",
            self.node_host, http_port, deploy_id
        );
        let client = reqwest::Client::new();

        match client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let deploy_info: serde_json::Value = response.json().await?;
                    if let Some(block_hash) = deploy_info.get("blockHash").and_then(|v| v.as_str())
                    {
                        Ok(Some(block_hash.to_string()))
                    } else {
                        Ok(None)
                    }
                } else {
                    let status = response.status();
                    let error_body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unable to read response body".to_string());

                    if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&error_body) {
                        match api_err.error.as_str() {
                            "deploy_not_found" => Ok(None),
                            _ => Err(format!(
                                "get_deploy_block_hash failed: {} ({})",
                                api_err.message, api_err.error
                            )
                            .into()),
                        }
                    } else {
                        Err(format!(
                            "HTTP error {}: {} - Response: {}",
                            status,
                            status.canonical_reason().unwrap_or("Unknown"),
                            error_body
                        )
                        .into())
                    }
                }
            }
            Err(e) => Err(format!("Network error: {e}").into()),
        }
    }

    pub async fn get_deploy_detail(
        &self,
        deploy_id: &str,
        http_port: u16,
    ) -> Result<Option<DeployDetail>, Box<dyn std::error::Error>> {
        let url = format!(
            "http://{}:{}/api/deploy/{}",
            self.node_host, http_port, deploy_id
        );
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&body) {
                if api_err.error == "deploy_not_found" {
                    return Ok(None);
                }
                return Err(format!(
                    "get_deploy_detail failed: {} ({})",
                    api_err.message, api_err.error
                )
                .into());
            }
            return Err(format!("HTTP {status} from {url}: {body}").into());
        }

        // None is reserved for 404 (handled above). A JSON parse error is a real
        // problem — schema mismatch, malformed response, etc. — and must surface.
        let detail = response.json::<DeployDetail>().await?;
        Ok(Some(detail))
    }

    /// Query `/api/deploy-finalization-status/{deploy_sig_hex}` for canonical
    /// finalization state of a deploy.
    ///
    /// Returns `Ok(Some(status))` on 200, `Ok(None)` on 404 (endpoint not
    /// available on this node — caller should fall back to block-hash polling),
    /// and `Err` on other failures (network, JSON parse, 5xx).
    pub async fn deploy_finalization_status(
        &self,
        deploy_sig_hex: &str,
        http_port: u16,
    ) -> Result<Option<DeployFinalizationStatus>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "http://{}:{}/api/deploy-finalization-status/{}",
            self.node_host, http_port, deploy_sig_hex
        );
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;

        if response.status().as_u16() == 404 {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&body) {
                return Err(format!(
                    "deploy_finalization_status failed: {} ({})",
                    api_err.message, api_err.error
                )
                .into());
            }
            return Err(format!("HTTP {status} from {url}: {body}").into());
        }

        let status = response.json::<DeployFinalizationStatus>().await?;
        Ok(Some(status))
    }

    /// Get deploy info using the default view (works on all nodes).
    /// Returns raw JSON with block metadata (blockHash, seqNum, blockNumber, etc.)
    pub async fn get_deploy_default(
        &self,
        deploy_id: &str,
        http_port: u16,
    ) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!(
            "http://{}:{}/api/deploy/{}",
            self.node_host, http_port, deploy_id
        );
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&body) {
                if api_err.error == "deploy_not_found" {
                    return Ok(None);
                }
                return Err(format!(
                    "get_deploy_default failed: {} ({})",
                    api_err.message, api_err.error
                )
                .into());
            }
            return Err(format!("HTTP {status} from {url}: {body}").into());
        }

        let json: serde_json::Value = response.json().await?;
        Ok(Some(json))
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
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let mut url = format!("http://{}:{}/api/estimate-cost", self.node_host, http_port);
        if let Some(bh) = block_hash {
            url.push_str(&format!("?block_hash={bh}"));
        }

        let body = EstimateCostRequest { term, deployer };
        let client = reqwest::Client::new();
        let response = client.post(&url).json(&body).send().await?;

        if response.status().is_success() {
            let result: EstimateCostResponse = response.json().await?;
            return Ok(result.cost);
        }

        let status = response.status();
        let raw_body = response.text().await.unwrap_or_default();

        if let Ok(api_err) = serde_json::from_str::<ApiErrorResponse>(&raw_body) {
            return Err(format!(
                "estimate-cost failed: {} ({})",
                api_err.message, api_err.error
            )
            .into());
        }

        Err(format!("HTTP {status} from {url}: {raw_body}").into())
    }
}

/// Request body for `POST /api/estimate-cost`.
#[derive(Serialize)]
struct EstimateCostRequest<'a> {
    term: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployer: Option<&'a str>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct EstimateCostResponse {
    pub cost: u64,
    #[serde(rename = "blockNumber")]
    pub block_number: i64,
    #[serde(rename = "blockHash")]
    pub block_hash: String,
}
