use blake2::{Blake2b, Digest};
use f1r3fly_models::casper::v1::deploy_response::Message as DeployResponseMessage;
use f1r3fly_models::casper::v1::deploy_service_client::DeployServiceClient;
use f1r3fly_models::casper::v1::exploratory_deploy_response::Message as ExploratoryDeployResponseMessage;
use f1r3fly_models::casper::v1::is_finalized_response::Message as IsFinalizedResponseMessage;
use f1r3fly_models::casper::v1::propose_response::Message as ProposeResponseMessage;
use f1r3fly_models::casper::v1::propose_service_client::ProposeServiceClient;
use f1r3fly_models::casper::{
    BlocksQuery, BlocksQueryByHeight, DeployDataProto, ExploratoryDeployQuery, IsFinalizedQuery,
    LightBlockInfo, ProposeQuery,
};
use f1r3fly_models::rhoapi::Par;
use f1r3fly_models::ByteString;
use prost::Message;
use secp256k1::{Message as Secp256k1Message, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use typenum::U32;

const DEPLOY_VALIDITY_WINDOW_BLOCKS: i64 = 50;
const BLOCK_SAMPLE_DEPTH: u32 = 8;
const BLOCK_FAST_SAMPLE_ATTEMPTS: usize = 2;
const BLOCK_FAST_SAMPLE_TIMEOUT_SECS: u64 = 1;
const BLOCK_FAST_SAMPLE_DELAY_MS: u64 = 25;
const BLOCK_SAMPLE_ATTEMPTS: usize = 5;
const BLOCK_SAMPLE_TIMEOUT_SECS: u64 = 4;
const BLOCK_SAMPLE_DELAY_MS: u64 = 150;
const TIP_CACHE_MAX_AGE_SECS: u64 = 2 * 60;
const TIP_CACHE_MAX_REGRESSION_BLOCKS: i64 = DEPLOY_VALIDITY_WINDOW_BLOCKS;
const TIP_CACHE_RESET_GUARD_MIN_CACHED_TIP: i64 = 512;
const TIP_CACHE_RESET_GUARD_MAX_SAMPLED_TIP: i64 = 128;

#[derive(Debug, Clone, Copy)]
struct TipSampleSummary {
    best_tip: i64,
    min_tip: i64,
    max_tip: i64,
    successful_samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployInfo {
    pub deploy_id: String,
    pub block_hash: Option<String>,
    pub sender: Option<String>,
    pub seq_num: Option<u64>,
    pub sig: Option<String>,
    pub sig_algorithm: Option<String>,
    pub shard_id: Option<String>,
    pub version: Option<u64>,
    pub timestamp: Option<u64>,
    pub status: DeployStatus,
    /// Whether the deploy execution errored
    pub errored: bool,
    /// System deploy error message (e.g., "Insufficient funds")
    pub system_deploy_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeployStatus {
    Pending,       // Deploy submitted but not yet in a block
    Included,      // Deploy included in a block
    NotFound,      // Deploy ID not found
    Error(String), // Error occurred
}

/// Client for interacting with the F1r3fly API
pub struct F1r3flyApi<'a> {
    signing_key: SecretKey,
    node_host: &'a str,
    grpc_port: u16,
}

impl<'a> F1r3flyApi<'a> {
    /// Creates a new F1r3fly API client
    ///
    /// # Arguments
    ///
    /// * `signing_key` - Hex-encoded private key for signing deploys
    /// * `node_host` - Hostname or IP address of the F1r3fly node
    /// * `grpc_port` - gRPC port for the node's API service
    ///
    /// # Returns
    ///
    /// A new `F1r3flyApi` instance
    pub fn new(signing_key: &str, node_host: &'a str, grpc_port: u16) -> Self {
        F1r3flyApi {
            signing_key: SecretKey::from_slice(&hex::decode(signing_key).unwrap()).unwrap(),
            node_host,
            grpc_port,
        }
    }

    /// Deploys Rholang code to the F1r3fly node
    ///
    /// # Arguments
    ///
    /// * `rho_code` - Rholang source code to deploy
    /// * `use_bigger_phlo_price` - Whether to use a larger phlo limit
    /// * `language` - Language of the deploy (typically "rholang")
    /// * `expiration_timestamp` - Optional expiration timestamp in milliseconds (0 = no expiration)
    ///
    /// # Returns
    ///
    /// The deploy ID if successful, otherwise an error
    pub async fn deploy(
        &self,
        rho_code: &str,
        use_bigger_phlo_price: bool,
        language: &str,
        expiration_timestamp: i64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let phlo_limit: i64 = if use_bigger_phlo_price {
            5_000_000_000
        } else {
            50_000
        };

        // Get current block number for VABN (solves Block 50 issue).
        // Use robust multi-sampling because some nodes can briefly return stale chain tips.
        let tip_lookup_start = Instant::now();
        let current_block = match self.get_current_block_number_monotonic().await {
            Ok(block_num) => {
                println!("Current block: {}", block_num);
                println!(
                    "✅ Setting validity window: blocks {} to {} ({}-block window)",
                    block_num,
                    block_num + DEPLOY_VALIDITY_WINDOW_BLOCKS,
                    DEPLOY_VALIDITY_WINDOW_BLOCKS
                );
                block_num
            }
            Err(e) => {
                println!(
                    "Warning: Could not get current block number ({}), using VABN=0",
                    e
                );
                println!("This may cause Block 50 issues if blockchain has > 50 blocks");
                0
            }
        };
        println!(
            "⏱️  Phase tip selection: {:.2?}",
            tip_lookup_start.elapsed()
        );

        // Log expiration info if set
        if expiration_timestamp > 0 {
            println!("Deploy expiration timestamp: {} ms", expiration_timestamp);
        }

        // Build and sign the deployment
        let deployment = self.build_deploy_msg(
            rho_code.to_string(),
            phlo_limit,
            language.to_string(),
            current_block,
            expiration_timestamp,
            None,
        );

        // Connect to the F1r3fly node
        let connect_start = Instant::now();
        let mut deploy_service_client =
            DeployServiceClient::connect(format!("http://{}:{}/", self.node_host, self.grpc_port))
                .await?;
        println!("⏱️  Phase grpc connect: {:.2?}", connect_start.elapsed());

        // Send the deploy
        let do_deploy_start = Instant::now();
        let deploy_response = deploy_service_client.do_deploy(deployment).await?;
        println!("⏱️  Phase do_deploy rpc: {:.2?}", do_deploy_start.elapsed());

        // Process the response
        let deploy_message = deploy_response
            .get_ref()
            .message
            .as_ref()
            .ok_or("Deploy result not found")?;

        match deploy_message {
            DeployResponseMessage::Error(service_error) => Err(service_error.clone().into()),
            DeployResponseMessage::Result(result) => {
                // Extract the deploy ID from the response - handle various formats
                let cleaned_result = result.trim();

                // Try different possible prefixes and formats
                if let Some(deploy_id) = cleaned_result.strip_prefix("Success! DeployId is: ") {
                    Ok(deploy_id.trim().to_string())
                } else if let Some(deploy_id) =
                    cleaned_result.strip_prefix("Success!\nDeployId is: ")
                {
                    Ok(deploy_id.trim().to_string())
                } else if cleaned_result.starts_with("Success!") {
                    // Look for any long hex string in the response
                    let lines: Vec<&str> = cleaned_result.lines().collect();
                    for line in lines {
                        let trimmed = line.trim();
                        if trimmed.len() > 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                            return Ok(trimmed.to_string());
                        }
                    }
                    Err(format!("Could not extract deploy ID from response: {}", result).into())
                } else {
                    // Assume it's already just the deploy ID
                    Ok(cleaned_result.to_string())
                }
            }
        }
    }

    /// Executes Rholang code without committing to the blockchain (exploratory deployment)
    ///
    /// # Arguments
    ///
    /// * `rho_code` - Rholang source code to execute
    /// * `block_hash` - Optional block hash to use as reference
    /// * `use_pre_state_hash` - Whether to use pre-state hash instead of post-state hash
    ///
    /// # Returns
    ///
    /// A tuple of (result data as JSON string, block info) if successful, otherwise an error
    pub async fn exploratory_deploy(
        &self,
        rho_code: &str,
        block_hash: Option<&str>,
        use_pre_state_hash: bool,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        // Connect to the F1r3fly node
        let mut deploy_service_client =
            DeployServiceClient::connect(format!("http://{}:{}/", self.node_host, self.grpc_port))
                .await?;

        // Build the exploratory deploy query
        let query = ExploratoryDeployQuery {
            term: rho_code.to_string(),
            block_hash: block_hash.unwrap_or("").to_string(),
            use_pre_state_hash,
        };

        // Send the exploratory deploy
        let response = deploy_service_client.exploratory_deploy(query).await?;

        // Process the response
        let message = response
            .get_ref()
            .message
            .as_ref()
            .ok_or("Exploratory deploy result not found")?;

        match message {
            ExploratoryDeployResponseMessage::Error(service_error) => {
                Err(service_error.clone().into())
            }
            ExploratoryDeployResponseMessage::Result(result) => {
                // Format the Par data structure to a readable string
                let data = {
                    let mut result_str = String::new();

                    // Process the data
                    if !result.post_block_data.is_empty() {
                        for (i, par) in result.post_block_data.iter().enumerate() {
                            if i > 0 {
                                result_str.push_str("\n");
                            }
                            // We're using a simplified representation of the Par data
                            // A more sophisticated approach would be to recursively traverse the structure
                            match extract_par_data(par) {
                                Some(data) => result_str.push_str(&data),
                                None => result_str
                                    .push_str(&format!("Result {}: Complex data structure", i + 1)),
                            }
                        }
                    } else {
                        result_str = "No data returned".to_string();
                    }

                    result_str
                };

                // Format the block info to a readable string
                let block_info = {
                    if let Some(block) = &result.block {
                        format!(
                            "Block hash: {}, Block number: {}",
                            block.block_hash, block.block_number
                        )
                    } else {
                        "No block info".to_string()
                    }
                };

                Ok((data, block_info))
            }
        }
    }

    /// Sends a proposal to the network to create a new block
    ///
    /// # Returns
    ///
    /// The block hash of the proposed block if successful, otherwise an error
    pub async fn propose(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Connect to the F1r3fly node's propose service
        let mut propose_client =
            ProposeServiceClient::connect(format!("http://{}:{}/", self.node_host, self.grpc_port))
                .await?;

        // Send the propose request
        let propose_response = propose_client
            .propose(ProposeQuery { is_async: false })
            .await?
            .into_inner();

        // Process the response
        let message = propose_response.message.ok_or("Missing propose response")?;

        match message {
            ProposeResponseMessage::Result(block_hash) => {
                // Extract the block hash from the response
                if let Some(hash) = block_hash
                    .strip_prefix("Success! Block ")
                    .and_then(|s| s.strip_suffix(" created and added."))
                {
                    Ok(hash.to_string())
                } else {
                    if Self::is_recoverable_propose_error(&block_hash) {
                        Ok(format!("SKIPPED: {}", block_hash))
                    } else {
                        Ok(block_hash) // Return the full message if we can't extract the hash
                    }
                }
            }
            ProposeResponseMessage::Error(error) => {
                let error_message = error.messages.join("; ");
                if Self::is_recoverable_propose_error(&error_message) {
                    Ok(format!("SKIPPED: {}", error_message))
                } else {
                    Err(format!("Propose error: {:?}", error).into())
                }
            }
        }
    }

    fn is_recoverable_propose_error(error_message: &str) -> bool {
        let normalized = error_message.to_ascii_lowercase();
        const RECOVERABLE_PATTERNS: [&str; 6] = [
            "must wait for more blocks from other validators",
            "no new blocks from peers yet; synchronize with network first",
            "no new deploys to propose",
            "propose skipped due to transient proposal race",
            "must wait for more blocks",
            "not enough new blocks",
        ];

        RECOVERABLE_PATTERNS
            .iter()
            .any(|pattern| normalized.contains(pattern))
    }

    /// Performs a full deployment cycle: deploy and propose
    ///
    /// # Arguments
    ///
    /// * `rho_code` - Rholang source code to deploy
    /// * `use_bigger_phlo_price` - Whether to use a larger phlo limit
    /// * `language` - Language of the deploy (typically "rholang")
    /// * `expiration_timestamp` - Optional expiration timestamp in milliseconds (0 = no expiration)
    ///
    /// # Returns
    ///
    /// The block hash if successful, otherwise an error
    pub async fn full_deploy(
        &self,
        rho_code: &str,
        use_bigger_phlo_price: bool,
        language: &str,
        expiration_timestamp: i64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // First deploy the code
        self.deploy(
            rho_code,
            use_bigger_phlo_price,
            language,
            expiration_timestamp,
        )
        .await?;

        // Then propose a block
        self.propose().await
    }

    /// Checks if a block is finalized, with retry logic
    ///
    /// # Arguments
    ///
    /// * `block_hash` - The hash of the block to check
    /// * `max_attempts` - Maximum number of retry attempts (default: 12)
    /// * `retry_delay_sec` - Delay between retries in seconds (default: 5)
    ///
    /// # Returns
    ///
    /// true if the block is finalized, false if the block is not finalized after all retry attempts
    pub async fn is_finalized(
        &self,
        block_hash: &str,
        max_attempts: u32,
        retry_delay_sec: u64,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let mut attempts = 0;

        loop {
            attempts += 1;

            // Connect to the F1r3fly node
            let mut deploy_service_client = DeployServiceClient::connect(format!(
                "http://{}:{}/",
                self.node_host, self.grpc_port
            ))
            .await?;

            // Query if the block is finalized
            let query = IsFinalizedQuery {
                hash: block_hash.to_string(),
            };

            match deploy_service_client.is_finalized(query).await {
                Ok(response) => {
                    let finalized_response = response.get_ref();
                    if let Some(message) = &finalized_response.message {
                        match message {
                            IsFinalizedResponseMessage::Error(_) => {
                                return Err("Error checking finalization status".into());
                            }
                            IsFinalizedResponseMessage::IsFinalized(is_finalized) => {
                                if *is_finalized {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    if attempts >= max_attempts {
                        return Err("Failed to connect to node after maximum attempts".into());
                    }
                }
            }

            if attempts >= max_attempts {
                return Ok(false);
            }

            // Wait before retrying
            tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_sec)).await;
        }
    }

    /// Get the block hash for a given deploy ID by querying the HTTP API
    ///
    /// # Arguments
    ///
    /// * `deploy_id` - The deploy ID to look up
    /// * `http_port` - The HTTP port for the deploy API endpoint
    ///
    /// # Returns
    ///
    /// Some(block_hash) if the deploy is included in a block, None if not yet included, or an error
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

                    // Extract blockHash from the response
                    if let Some(block_hash) = deploy_info.get("blockHash").and_then(|v| v.as_str())
                    {
                        Ok(Some(block_hash.to_string()))
                    } else {
                        Ok(None) // Deploy exists but no blockHash yet
                    }
                } else if response.status().as_u16() == 404 {
                    Ok(None) // Deploy not found yet
                } else {
                    let status = response.status();
                    let error_body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unable to read response body".to_string());

                    // Handle the case where the deploy exists but isn't in a block yet
                    if error_body.contains("Couldn't find block containing deploy with id:") {
                        Ok(None) // Deploy exists but not in a block yet
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

    /// Gets comprehensive information about a deploy by ID
    ///
    /// # Arguments
    ///
    /// * `deploy_id` - The deploy ID to look up
    /// * `http_port` - HTTP port for API queries
    ///
    /// # Returns
    ///
    /// DeployInfo struct with deploy details and status
    pub async fn get_deploy_info(
        &self,
        deploy_id: &str,
        http_port: u16,
    ) -> Result<DeployInfo, Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        // Step 1: Get block hash from /api/deploy/{id}
        let deploy_url = format!(
            "http://{}:{}/api/deploy/{}",
            self.node_host, http_port, deploy_id
        );

        let deploy_response = match client.get(&deploy_url).send().await {
            Ok(response) => response,
            Err(e) => {
                return Ok(DeployInfo {
                    deploy_id: deploy_id.to_string(),
                    block_hash: None,
                    sender: None,
                    seq_num: None,
                    sig: None,
                    sig_algorithm: None,
                    shard_id: None,
                    version: None,
                    timestamp: None,
                    status: DeployStatus::Error(format!("Network error: {}", e)),
                    errored: false,
                    system_deploy_error: None,
                });
            }
        };

        if deploy_response.status().as_u16() == 404 {
            return Ok(DeployInfo {
                deploy_id: deploy_id.to_string(),
                block_hash: None,
                sender: None,
                seq_num: None,
                sig: None,
                sig_algorithm: None,
                shard_id: None,
                version: None,
                timestamp: None,
                status: DeployStatus::NotFound,
                errored: false,
                system_deploy_error: None,
            });
        }

        if !deploy_response.status().is_success() {
            let error_body = deploy_response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());

            if error_body.contains("Couldn't find block containing deploy with id:") {
                return Ok(DeployInfo {
                    deploy_id: deploy_id.to_string(),
                    block_hash: None,
                    sender: None,
                    seq_num: None,
                    sig: None,
                    sig_algorithm: None,
                    shard_id: None,
                    version: None,
                    timestamp: None,
                    status: DeployStatus::Pending,
                    errored: false,
                    system_deploy_error: None,
                });
            }

            return Ok(DeployInfo {
                deploy_id: deploy_id.to_string(),
                block_hash: None,
                sender: None,
                seq_num: None,
                sig: None,
                sig_algorithm: None,
                shard_id: None,
                version: None,
                timestamp: None,
                status: DeployStatus::Error(format!("HTTP error: {}", error_body)),
                errored: false,
                system_deploy_error: None,
            });
        }

        let deploy_data: serde_json::Value = deploy_response.json().await?;
        let block_hash = deploy_data
            .get("blockHash")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Step 2: Get full block info to find deploy details with systemDeployError
        let (errored, system_deploy_error) = if let Some(ref bh) = block_hash {
            let block_url = format!("http://{}:{}/api/block/{}", self.node_host, http_port, bh);

            match client.get(&block_url).send().await {
                Ok(block_response) if block_response.status().is_success() => {
                    let block_data: serde_json::Value = block_response.json().await?;

                    // Find our deploy in the block's deploys array by matching sig
                    let deploy_details = block_data
                        .get("deploys")
                        .and_then(|d| d.as_array())
                        .and_then(|deploys| {
                            deploys.iter().find(|d| {
                                d.get("sig")
                                    .and_then(|s| s.as_str())
                                    .map(|s| s == deploy_id)
                                    .unwrap_or(false)
                            })
                        });

                    if let Some(details) = deploy_details {
                        let errored = details
                            .get("errored")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let system_error = details
                            .get("systemDeployError")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string());
                        (errored, system_error)
                    } else {
                        (false, None)
                    }
                }
                _ => (false, None),
            }
        } else {
            (false, None)
        };

        // Parse the response into DeployInfo
        let deploy_info = DeployInfo {
            deploy_id: deploy_id.to_string(),
            block_hash,
            sender: deploy_data
                .get("sender")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            seq_num: deploy_data.get("seqNum").and_then(|v| v.as_u64()),
            sig: deploy_data
                .get("sig")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            sig_algorithm: deploy_data
                .get("sigAlgorithm")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            shard_id: deploy_data
                .get("shardId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            version: deploy_data.get("version").and_then(|v| v.as_u64()),
            timestamp: deploy_data.get("timestamp").and_then(|v| v.as_u64()),
            status: DeployStatus::Included,
            errored,
            system_deploy_error,
        };
        Ok(deploy_info)
    }

    /// Gets blocks in the main chain
    ///
    /// # Arguments
    ///
    /// * `depth` - Number of blocks to retrieve from the main chain
    ///
    /// # Returns
    ///
    /// A vector of LightBlockInfo representing blocks in the main chain
    pub async fn show_main_chain(
        &self,
        depth: u32,
    ) -> Result<Vec<LightBlockInfo>, Box<dyn std::error::Error>> {
        use f1r3fly_models::casper::v1::block_info_response::Message;
        use f1r3fly_models::casper::v1::deploy_service_client::DeployServiceClient;

        // Connect to the F1r3fly node
        let mut deploy_service_client =
            DeployServiceClient::connect(format!("http://{}:{}/", self.node_host, self.grpc_port))
                .await?;

        // Create the query
        let query = BlocksQuery {
            depth: depth as i32,
        };

        // Send the query and collect streaming response
        let mut stream = deploy_service_client
            .show_main_chain(query)
            .await?
            .into_inner();

        let mut blocks = Vec::new();
        while let Some(response) = stream.message().await? {
            if let Some(message) = response.message {
                match message {
                    Message::Error(service_error) => {
                        return Err(
                            format!("gRPC Error: {}", service_error.messages.join("; ")).into()
                        );
                    }
                    Message::BlockInfo(block_info) => {
                        blocks.push(block_info);
                    }
                }
            }
        }

        Ok(blocks)
    }

    fn tip_cache_path(&self) -> PathBuf {
        let safe_host: String = self
            .node_host
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        std::env::temp_dir().join(format!(
            "f1r3fly_tip_floor_{}_{}.txt",
            safe_host, self.grpc_port
        ))
    }

    fn read_cached_tip(&self) -> Option<i64> {
        let path = self.tip_cache_path();
        let metadata = fs::metadata(&path).ok()?;
        let modified = metadata.modified().ok()?;
        let age_secs = SystemTime::now().duration_since(modified).ok()?.as_secs();
        if age_secs > TIP_CACHE_MAX_AGE_SECS {
            return None;
        }
        let raw = fs::read_to_string(path).ok()?;
        raw.trim().parse::<i64>().ok()
    }

    fn write_cached_tip(&self, tip: i64) {
        let _ = fs::write(self.tip_cache_path(), tip.to_string());
    }

    async fn get_current_block_number_monotonic(
        &self,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let cached_tip = self.read_cached_tip();
        let sampled_tip = match self.get_current_block_number_fast().await {
            Ok(summary) => {
                let spread = summary.max_tip - summary.min_tip;
                let cold_start_needs_robust = cached_tip.is_none()
                    && (summary.successful_samples < BLOCK_FAST_SAMPLE_ATTEMPTS
                        || spread > DEPLOY_VALIDITY_WINDOW_BLOCKS);

                if cold_start_needs_robust {
                    println!(
                        "⚠️  Cold-start fast sampling insufficient (samples={}, spread={} blocks). Falling back to robust sampling.",
                        summary.successful_samples, spread
                    );
                    self.get_current_block_number_robust().await?
                } else {
                    summary.best_tip
                }
            }
            Err(err) => {
                println!(
                    "⚠️  Fast tip sampling failed ({}). Falling back to robust sampling.",
                    err
                );
                self.get_current_block_number_robust().await?
            }
        };

        let selected_tip = if let Some(cached_tip) = cached_tip {
            if sampled_tip < cached_tip {
                let tip_drop = cached_tip - sampled_tip;
                // If tip regresses by more than one validity window, stale cache floor is harmful:
                // it can push validAfterBlockNumber far into the future and trigger empty-block churn.
                if tip_drop > TIP_CACHE_MAX_REGRESSION_BLOCKS {
                    println!(
                        "♻️  Detected stale tip floor: sampled tip {} is {} blocks behind cached {} (> {}). Resetting tip floor.",
                        sampled_tip, tip_drop, cached_tip, TIP_CACHE_MAX_REGRESSION_BLOCKS
                    );
                    sampled_tip
                // If the sampled tip is very low while cached tip is high, this is most likely
                // a local chain reset/restart, so stale cache floor must be ignored.
                } else
                // If the sampled tip is very low while cached tip is high, this is most likely
                // a local chain reset/restart, so stale cache floor must be ignored.
                if cached_tip >= TIP_CACHE_RESET_GUARD_MIN_CACHED_TIP
                    && sampled_tip <= TIP_CACHE_RESET_GUARD_MAX_SAMPLED_TIP
                    && tip_drop > 0
                {
                    println!(
                        "♻️  Detected probable chain reset: sampled tip {} (cached {}). Resetting tip floor.",
                        sampled_tip, cached_tip
                    );
                    sampled_tip
                } else {
                    println!(
                        "⚠️  Tip sample regressed from cached {} to {}. Using cached tip.",
                        cached_tip, sampled_tip
                    );
                    cached_tip
                }
            } else {
                sampled_tip
            }
        } else {
            sampled_tip
        };

        self.write_cached_tip(selected_tip);
        Ok(selected_tip)
    }

    /// Gets a low-latency estimate of the current block number.
    ///
    /// This is the common fast path and is designed to avoid adding
    /// significant overhead to every deploy while still sampling more than once.
    async fn get_current_block_number_fast(
        &self,
    ) -> Result<TipSampleSummary, Box<dyn std::error::Error>> {
        let mut best_tip: Option<i64> = None;
        let mut min_tip: Option<i64> = None;
        let mut max_tip: Option<i64> = None;
        let mut successful_samples: usize = 0;

        for attempt in 1..=BLOCK_FAST_SAMPLE_ATTEMPTS {
            let sampled_tip = match tokio::time::timeout(
                tokio::time::Duration::from_secs(BLOCK_FAST_SAMPLE_TIMEOUT_SECS),
                self.show_main_chain(BLOCK_SAMPLE_DEPTH),
            )
            .await
            {
                Ok(Ok(blocks)) => blocks.iter().map(|b| b.block_number).max(),
                Ok(Err(err)) => {
                    println!(
                        "⚠️  Fast tip sample {}/{} failed: {}",
                        attempt, BLOCK_FAST_SAMPLE_ATTEMPTS, err
                    );
                    None
                }
                Err(_) => {
                    println!(
                        "⚠️  Fast tip sample {}/{} timed out after {}s",
                        attempt, BLOCK_FAST_SAMPLE_ATTEMPTS, BLOCK_FAST_SAMPLE_TIMEOUT_SECS
                    );
                    None
                }
            };

            if let Some(tip) = sampled_tip {
                successful_samples += 1;
                best_tip = Some(best_tip.map_or(tip, |prev| prev.max(tip)));
                min_tip = Some(min_tip.map_or(tip, |prev| prev.min(tip)));
                max_tip = Some(max_tip.map_or(tip, |prev| prev.max(tip)));
            }

            if attempt < BLOCK_FAST_SAMPLE_ATTEMPTS {
                tokio::time::sleep(tokio::time::Duration::from_millis(BLOCK_FAST_SAMPLE_DELAY_MS))
                    .await;
            }
        }

        if let Some(best_tip) = best_tip {
            let min_tip = min_tip.unwrap_or(best_tip);
            let max_tip = max_tip.unwrap_or(best_tip);
            if successful_samples > 1 {
                println!(
                    "⚡ Fast tip sampling: {} successful samples, range {}..{}, selected {}",
                    successful_samples, min_tip, max_tip, best_tip
                );
            }
            Ok(TipSampleSummary {
                best_tip,
                min_tip,
                max_tip,
                successful_samples,
            })
        } else {
            Err("Failed to sample current block number from main chain".into())
        }
    }

    /// Gets blocks by height range from the blockchain
    ///
    /// # Arguments
    ///
    /// * `start_block_number` - Start block number (inclusive)
    /// * `end_block_number` - End block number (inclusive)
    ///
    /// # Returns
    ///
    /// A vector of LightBlockInfo representing blocks in the specified height range
    pub async fn get_blocks_by_height(
        &self,
        start_block_number: i64,
        end_block_number: i64,
    ) -> Result<Vec<LightBlockInfo>, Box<dyn std::error::Error>> {
        use f1r3fly_models::casper::v1::block_info_response::Message;
        use f1r3fly_models::casper::v1::deploy_service_client::DeployServiceClient;

        // Connect to the F1r3fly node
        let mut deploy_service_client =
            DeployServiceClient::connect(format!("http://{}:{}/", self.node_host, self.grpc_port))
                .await?;

        // Create the query
        let query = BlocksQueryByHeight {
            start_block_number,
            end_block_number,
        };

        // Send the query and collect streaming response
        let mut stream = deploy_service_client
            .get_blocks_by_heights(query)
            .await?
            .into_inner();

        let mut blocks = Vec::new();
        while let Some(response) = stream.message().await? {
            if let Some(message) = response.message {
                match message {
                    Message::Error(service_error) => {
                        return Err(
                            format!("gRPC Error: {}", service_error.messages.join("; ")).into()
                        );
                    }
                    Message::BlockInfo(block_info) => {
                        blocks.push(block_info);
                    }
                }
            }
        }

        Ok(blocks)
    }

    /// Gets the current block number from the blockchain
    ///
    /// # Returns
    ///
    /// The current block number if successful, otherwise an error
    pub async fn get_current_block_number(&self) -> Result<i64, Box<dyn std::error::Error>> {
        // Get the most recent block using show_main_chain with depth 1
        let blocks = self.show_main_chain(1).await?;

        if let Some(latest_block) = blocks.first() {
            Ok(latest_block.block_number)
        } else {
            // Fallback to 0 if no blocks found (genesis case)
            Ok(0)
        }
    }

    /// Deploy Rholang code with a specific phlo limit
    ///
    /// Unlike `deploy()` which uses a fixed phlo limit based on a boolean flag,
    /// this method allows specifying an exact phlo limit.
    pub async fn deploy_with_phlo_limit(
        &self,
        rho_code: &str,
        phlo_limit: i64,
        language: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.deploy_internal(rho_code, phlo_limit, language, 0, None)
            .await
    }

    /// Deploy Rholang code with a specific timestamp and phlo limit
    ///
    /// Required for `insertSigned` compatibility where the deploy timestamp
    /// must match the signature timestamp.
    pub async fn deploy_with_timestamp_and_phlo_limit(
        &self,
        rho_code: &str,
        language: &str,
        timestamp_millis: Option<i64>,
        phlo_limit: i64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.deploy_internal(rho_code, phlo_limit, language, 0, timestamp_millis)
            .await
    }

    /// Internal deploy implementation with all parameters
    async fn deploy_internal(
        &self,
        rho_code: &str,
        phlo_limit: i64,
        language: &str,
        expiration_timestamp: i64,
        timestamp_override: Option<i64>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let current_block = match self.get_current_block_number().await {
            Ok(block_num) => block_num,
            Err(_) => 0,
        };

        let deployment = self.build_deploy_msg(
            rho_code.to_string(),
            phlo_limit,
            language.to_string(),
            current_block,
            expiration_timestamp,
            timestamp_override,
        );

        let mut deploy_service_client =
            DeployServiceClient::connect(format!("http://{}:{}/", self.node_host, self.grpc_port))
                .await?;

        let deploy_response = deploy_service_client.do_deploy(deployment).await?;

        let deploy_message = deploy_response
            .get_ref()
            .message
            .as_ref()
            .ok_or("Deploy result not found")?;

        match deploy_message {
            DeployResponseMessage::Error(service_error) => Err(service_error.clone().into()),
            DeployResponseMessage::Result(result) => {
                let cleaned_result = result.trim();
                if let Some(deploy_id) = cleaned_result.strip_prefix("Success! DeployId is: ") {
                    Ok(deploy_id.trim().to_string())
                } else if let Some(deploy_id) =
                    cleaned_result.strip_prefix("Success!\nDeployId is: ")
                {
                    Ok(deploy_id.trim().to_string())
                } else if cleaned_result.starts_with("Success!") {
                    let lines: Vec<&str> = cleaned_result.lines().collect();
                    for line in lines {
                        let trimmed = line.trim();
                        if trimmed.len() > 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                            return Ok(trimmed.to_string());
                        }
                    }
                    Err(format!("Could not extract deploy ID from response: {}", result).into())
                } else {
                    Ok(cleaned_result.to_string())
                }
            }
        }
    }

    /// Gets a robust estimate of the current block number by sampling multiple tips.
    ///
    /// This mitigates intermittent stale-tip responses by:
    /// - sampling multiple times,
    /// - using a deeper chain view than depth=1,
    /// - taking the maximum observed tip.
    pub async fn get_current_block_number_robust(
        &self,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let mut best_tip: Option<i64> = None;
        let mut successful_samples: usize = 0;

        for attempt in 1..=BLOCK_SAMPLE_ATTEMPTS {
            let sampled_tip = match tokio::time::timeout(
                tokio::time::Duration::from_secs(BLOCK_SAMPLE_TIMEOUT_SECS),
                self.show_main_chain(BLOCK_SAMPLE_DEPTH),
            )
            .await
            {
                Ok(Ok(blocks)) => blocks.iter().map(|b| b.block_number).max(),
                Ok(Err(err)) => {
                    println!("⚠️  Tip sample {}/{} failed: {}", attempt, BLOCK_SAMPLE_ATTEMPTS, err);
                    None
                }
                Err(_) => {
                    println!(
                        "⚠️  Tip sample {}/{} timed out after {}s",
                        attempt, BLOCK_SAMPLE_ATTEMPTS, BLOCK_SAMPLE_TIMEOUT_SECS
                    );
                    None
                }
            };

            if let Some(tip) = sampled_tip {
                successful_samples += 1;
                best_tip = Some(best_tip.map_or(tip, |prev| prev.max(tip)));
            }

            if attempt < BLOCK_SAMPLE_ATTEMPTS {
                tokio::time::sleep(tokio::time::Duration::from_millis(BLOCK_SAMPLE_DELAY_MS))
                    .await;
            }
        }

        if let Some(tip) = best_tip {
            if successful_samples > 1 {
                println!(
                    "📈 Tip sampling: {} successful samples, selected max tip {}",
                    successful_samples, tip
                );
            }
            Ok(tip)
        } else {
            Err("Failed to sample current block number from main chain".into())
        }
    }

    /// Builds and signs a deploy message
    ///
    /// # Arguments
    ///
    /// * `code` - Rholang source code to deploy
    /// * `phlo_limit` - Maximum amount of phlo to use for execution
    /// * `language` - Language of the deploy (typically "rholang")
    /// * `valid_after_block_number` - Block number after which the deploy is valid
    /// * `expiration_timestamp` - Expiration timestamp in milliseconds (0 = no expiration)
    ///
    /// # Returns
    ///
    /// A signed `DeployDataProto` ready to be sent to the node
    fn build_deploy_msg(
        &self,
        code: String,
        phlo_limit: i64,
        language: String,
        valid_after_block_number: i64,
        expiration_timestamp: i64,
        timestamp_override: Option<i64>,
    ) -> DeployDataProto {
        let timestamp = timestamp_override.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get system time")
                .as_millis() as i64
        });

        // Create a projection with only the fields used for signature calculation
        // IMPORTANT: The language field is deliberately excluded from signature calculation.
        // expiration_timestamp IS included when set (non-zero).
        // Proto3 will not serialize 0 values, so 0 = "not set" = backward compatible.
        let projection = DeployDataProto {
            term: code.clone(),
            timestamp,
            phlo_price: 1,
            phlo_limit,
            valid_after_block_number,
            shard_id: "root".into(),
            language: String::new(), // Excluded from signature calculation
            sig: ByteString::new(),
            deployer: ByteString::new(),
            sig_algorithm: String::new(),
            expiration_timestamp, // Included when non-zero (proto3 omits 0 values)
        };

        // Serialize the projection for hashing
        let serialized = projection.encode_to_vec();

        // Hash with Blake2b256
        let digest = blake2b_256_hash(&serialized);

        // Sign the digest with secp256k1
        let secp = Secp256k1::new();
        let message = Secp256k1Message::from_digest(digest.into());
        let signature = secp.sign_ecdsa(&message, &self.signing_key);

        // Get signature in DER format
        let sig_bytes = signature.serialize_der().to_vec();

        // Get the public key in uncompressed format
        let public_key = self.signing_key.public_key(&secp);
        let pub_key_bytes = public_key.serialize_uncompressed().to_vec();

        // Return the complete deploy message
        DeployDataProto {
            term: code,
            timestamp,
            phlo_price: 1,
            phlo_limit,
            valid_after_block_number,
            shard_id: "root".into(),
            language,
            sig: ByteString::from(sig_bytes),
            sig_algorithm: "secp256k1".into(),
            deployer: ByteString::from(pub_key_bytes),
            expiration_timestamp,
        }
    }
}

/// Extracts a simplified string representation from a Par object
fn extract_par_data(par: &Par) -> Option<String> {
    // Check for expressions
    if !par.exprs.is_empty() && par.exprs[0].expr_instance.is_some() {
        // Extract data from the first expression
        let expr = &par.exprs[0];
        if let Some(instance) = &expr.expr_instance {
            match instance {
                // Handle different types of expressions
                f1r3fly_models::rhoapi::expr::ExprInstance::GString(s) => {
                    Some(format!("\"{}\"", s))
                }
                f1r3fly_models::rhoapi::expr::ExprInstance::GInt(i) => Some(i.to_string()),
                f1r3fly_models::rhoapi::expr::ExprInstance::GBool(b) => Some(b.to_string()),
                // Add other types as needed
                _ => Some("Complex expression".to_string()),
            }
        } else {
            None
        }
    }
    // Check for sends
    else if !par.sends.is_empty() {
        Some("Send operation".to_string())
    }
    // Check for receives
    else if !par.receives.is_empty() {
        Some("Receive operation".to_string())
    }
    // Check for news
    else if !par.news.is_empty() {
        Some("New declaration".to_string())
    }
    // Empty or unsupported Par object
    else {
        None
    }
}

/// Computes a Blake2b 256-bit hash of the provided data
fn blake2b_256_hash(data: &[u8]) -> [u8; 32] {
    let mut blake = Blake2b::<U32>::new();
    blake.update(data);
    let hash = blake.finalize();
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);
    result
}
