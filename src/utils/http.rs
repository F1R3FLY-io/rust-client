use reqwest;
use serde_json;
use std::time::{Duration, Instant};

pub struct HttpClient {
    client: reqwest::Client,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_json(
        &self,
        url: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let response = self.client.get(url).send().await?;

        if response.status().is_success() {
            let text = response.text().await?;
            let json: serde_json::Value = serde_json::from_str(&text)?;
            Ok(json)
        } else {
            Err(format!("HTTP {}: {}", response.status(), response.text().await?).into())
        }
    }

    pub async fn get_text(&self, url: &str) -> Result<String, Box<dyn std::error::Error>> {
        let response = self.client.get(url).send().await?;

        if response.status().is_success() {
            Ok(response.text().await?)
        } else {
            Err(format!("HTTP {}: {}", response.status(), response.text().await?).into())
        }
    }

    pub async fn get_with_timing(
        &self,
        url: &str,
    ) -> Result<(serde_json::Value, std::time::Duration), Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        let result = self.get_json(url).await?;
        let duration = start_time.elapsed();
        Ok((result, duration))
    }
}

pub fn build_url(host: &str, port: u16, path: &str) -> String {
    format!("http://{host}:{port}{path}")
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
