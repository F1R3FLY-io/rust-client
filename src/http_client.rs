// HTTP client for F1r3node API
//
// Provides an HTTP-based client for interacting with F1r3node,
// using the node's HTTP API endpoints instead of gRPC.

use reqwest;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::connection_manager::ConnectionConfig;
use crate::signing::sign_deploy_data;

/// HTTP client for F1r3node operations
#[derive(Clone, Debug)]
pub struct F1r3nodeHttpClient {
    base_url: String,
    private_key: SecretKey,
    client: reqwest::Client,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DeployRequest {
    pub term: String,
    pub timestamp: i64,
    pub phlo_limit: i64,
    pub phlo_price: i64,
    pub valid_after_block_number: i64,
    pub sig_algorithm: String,
    pub signature: String,
    pub signer_public_key: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DeployResponse {
    pub deploy_id: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockInfo {
    pub block_hash: String,
    pub block_number: i64,
    pub timestamp: i64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LightBlockInfo {
    pub block_hash: String,
    pub block_number: i64,
}

#[derive(Deserialize, Debug)]
pub struct RhoDataResponse {
    pub expr: Vec<serde_json::Value>,
    pub block: BlockInfo,
}

#[derive(Debug)]
pub enum HttpError {
    Request(reqwest::Error),
    Json(serde_json::Error),
    Signing(String),
    Config(String),
    Timeout(String),
    InvalidResponse(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Request(e) => write!(f, "HTTP request failed: {}", e),
            HttpError::Json(e) => write!(f, "JSON error: {}", e),
            HttpError::Signing(e) => write!(f, "Signing failed: {}", e),
            HttpError::Config(e) => write!(f, "Configuration error: {}", e),
            HttpError::Timeout(e) => write!(f, "Timeout: {}", e),
            HttpError::InvalidResponse(e) => write!(f, "Invalid response: {}", e),
        }
    }
}

impl std::error::Error for HttpError {}

impl From<reqwest::Error> for HttpError {
    fn from(err: reqwest::Error) -> Self {
        HttpError::Request(err)
    }
}

impl From<serde_json::Error> for HttpError {
    fn from(err: serde_json::Error) -> Self {
        HttpError::Json(err)
    }
}

impl F1r3nodeHttpClient {
    /// Create a new HTTP client from connection configuration
    pub fn from_config(config: &ConnectionConfig) -> Result<Self, HttpError> {
        let base_url = format!("http://{}:{}", config.node_host, config.http_port);

        let key_bytes = hex::decode(&config.signing_key)
            .map_err(|e| HttpError::Config(format!("Invalid signing key hex: {}", e)))?;
        let private_key = SecretKey::from_slice(&key_bytes)
            .map_err(|e| HttpError::Config(format!("Invalid secp256k1 key: {}", e)))?;

        Self::new(base_url, private_key)
    }

    /// Create a new HTTP client with explicit parameters
    pub fn new(base_url: String, private_key: SecretKey) -> Result<Self, HttpError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| HttpError::Config(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            base_url,
            private_key,
            client,
        })
    }

    /// Deploy Rholang code to F1r3node
    pub async fn deploy(&self, term: &str) -> Result<String, HttpError> {
        let request = self.create_deploy_request(term)?;

        let response = self
            .client
            .post(&format!("{}/api/deploy", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "unable to read body".to_string());
            return Err(HttpError::InvalidResponse(format!(
                "Deploy failed with status {}: {}",
                status, body
            )));
        }

        let deploy_response: DeployResponse = response.json().await?;
        Ok(deploy_response.deploy_id)
    }

    /// Find deployment information by deploy ID
    pub async fn find_deploy(&self, deploy_id: &str) -> Result<BlockInfo, HttpError> {
        let response = self
            .client
            .get(&format!("{}/api/deploy/{}", self.base_url, deploy_id))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "unable to read body".to_string());
            return Err(HttpError::InvalidResponse(format!(
                "Find deploy failed with status {}: {}",
                status, body
            )));
        }

        Ok(response.json().await?)
    }

    /// Check if a block is finalized
    pub async fn is_finalized(&self, block_hash: &str) -> Result<bool, HttpError> {
        let response = self
            .client
            .get(&format!(
                "{}/api/is-finalized/{}",
                self.base_url, block_hash
            ))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "unable to read body".to_string());
            return Err(HttpError::InvalidResponse(format!(
                "Is finalized check failed with status {}: {}",
                status, body
            )));
        }

        Ok(response.json().await?)
    }

    /// Get the last finalized block
    pub async fn last_finalized_block(&self) -> Result<BlockInfo, HttpError> {
        let response = self
            .client
            .get(&format!("{}/api/last-finalized-block", self.base_url))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "unable to read body".to_string());
            return Err(HttpError::InvalidResponse(format!(
                "Last finalized block failed with status {}: {}",
                status, body
            )));
        }

        Ok(response.json().await?)
    }

    /// Exploratory deploy (read-only execution)
    pub async fn explore_deploy(&self, term: &str) -> Result<RhoDataResponse, HttpError> {
        let request = self.create_deploy_request(term)?;

        let response = self
            .client
            .post(&format!("{}/api/explore-deploy", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "unable to read body".to_string());
            return Err(HttpError::InvalidResponse(format!(
                "Explore deploy failed with status {}: {}",
                status, body
            )));
        }

        Ok(response.json().await?)
    }

    /// Get block information by hash
    pub async fn get_block(&self, hash: &str) -> Result<BlockInfo, HttpError> {
        let response = self
            .client
            .get(&format!("{}/api/block/{}", self.base_url, hash))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "unable to read body".to_string());
            return Err(HttpError::InvalidResponse(format!(
                "Get block failed with status {}: {}",
                status, body
            )));
        }

        Ok(response.json().await?)
    }

    /// Wait for a block to be finalized (polls with interval)
    pub async fn wait_for_finalization(
        &self,
        block_hash: &str,
        max_attempts: u32,
        poll_interval_secs: u64,
    ) -> Result<(), HttpError> {
        for attempt in 0..max_attempts {
            if self.is_finalized(block_hash).await? {
                return Ok(());
            }

            if attempt < max_attempts - 1 {
                tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval_secs)).await;
            }
        }

        Err(HttpError::Timeout(format!(
            "Block {} not finalized after {} attempts ({}s interval)",
            block_hash, max_attempts, poll_interval_secs
        )))
    }

    fn create_deploy_request(&self, term: &str) -> Result<DeployRequest, HttpError> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| HttpError::Config(format!("System time error: {}", e)))?
            .as_millis() as i64;

        let phlo_limit = 1_000_000;
        let phlo_price = 1;
        let valid_after_block_number = -1;

        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &self.private_key);
        let public_key_bytes = public_key.serialize_uncompressed();
        let signer_public_key = hex::encode(&public_key_bytes[1..]);

        let signature = sign_deploy_data(
            term.as_bytes(),
            timestamp,
            &self.private_key,
        )
        .map_err(|e| HttpError::Signing(e.to_string()))?;

        Ok(DeployRequest {
            term: term.to_string(),
            timestamp,
            phlo_limit,
            phlo_price,
            valid_after_block_number,
            sig_algorithm: "secp256k1".to_string(),
            signature: hex::encode(signature),
            signer_public_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_private_key() -> SecretKey {
        SecretKey::from_slice(&[0x42u8; 32]).expect("32 bytes is valid")
    }

    fn test_private_key_hex() -> String {
        hex::encode([0x42u8; 32])
    }

    #[test]
    fn test_create_deploy_request() {
        let client = F1r3nodeHttpClient::new(
            "http://localhost:40403".to_string(),
            test_private_key(),
        )
        .unwrap();

        let request = client.create_deploy_request("new x in { x!(1) }").unwrap();

        assert_eq!(request.term, "new x in { x!(1) }");
        assert_eq!(request.phlo_limit, 1_000_000);
        assert_eq!(request.phlo_price, 1);
        assert_eq!(request.valid_after_block_number, -1);
        assert_eq!(request.sig_algorithm, "secp256k1");
        assert!(!request.signature.is_empty());
        assert!(!request.signer_public_key.is_empty());
    }

    #[test]
    fn test_client_creation() {
        let config = ConnectionConfig {
            node_host: "localhost".to_string(),
            http_port: 40403,
            grpc_port: 40402,
            signing_key: test_private_key_hex(),
        };

        let client = F1r3nodeHttpClient::from_config(&config).unwrap();
        assert_eq!(client.base_url, "http://localhost:40403");
    }
}
