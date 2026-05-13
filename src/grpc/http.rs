//! HTTP-based methods on F1r3flyApi (deploy lookup, deploy detail)

use super::F1r3flyApi;
use crate::f1r3fly_api::{DeployDetail, DeployFinalizationStatus};

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
                } else if response.status().as_u16() == 404 {
                    Ok(None)
                } else {
                    let status = response.status();
                    let error_body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unable to read response body".to_string());

                    if error_body.contains("Couldn't find block containing deploy with id:") {
                        Ok(None)
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
            Err(e) => Err(format!("Network error: {}", e).into()),
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
            return Ok(None);
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
    ) -> Result<Option<DeployFinalizationStatus>, Box<dyn std::error::Error>> {
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
            return Err(format!("HTTP {} from {}: {}", status, url, body).into());
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
            return Ok(None);
        }

        let json: serde_json::Value = response.json().await?;
        Ok(Some(json))
    }
}
