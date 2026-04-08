//! Deploy and propose operations

use super::F1r3flyApi;
use blake2::{Blake2b, Digest};
use f1r3fly_models::casper::v1::deploy_response::Message as DeployResponseMessage;
use f1r3fly_models::casper::v1::deploy_service_client::DeployServiceClient;
use f1r3fly_models::casper::v1::propose_response::Message as ProposeResponseMessage;
use f1r3fly_models::casper::v1::propose_service_client::ProposeServiceClient;
use f1r3fly_models::casper::{DeployDataProto, ProposeQuery};
use f1r3fly_models::ByteString;
use prost::Message;
use secp256k1::{Message as Secp256k1Message, Secp256k1};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use typenum::U32;

use crate::f1r3fly_api::ProposeResult;

const DEPLOY_VALIDITY_WINDOW_BLOCKS: i64 = 50;

impl<'a> F1r3flyApi<'a> {
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

 let tip_lookup_start = Instant::now();
 let current_block = match self.get_current_block_number_monotonic().await {
 Ok(block_num) => {
 tracing::info!(block_num, "Current block");
 tracing::info!(
 from = block_num,
 to = block_num + DEPLOY_VALIDITY_WINDOW_BLOCKS,
 window = DEPLOY_VALIDITY_WINDOW_BLOCKS,
 "Setting validity window"
 );
 block_num
 }
 Err(e) => {
 tracing::warn!("Could not get current block number ({}), using VABN=0", e);
 0
 }
 };
 tracing::debug!(elapsed = ?tip_lookup_start.elapsed(), "Tip selection");

 if expiration_timestamp > 0 {
 tracing::info!(expiration_timestamp, "Deploy expiration timestamp (ms)");
 }

 let deployment = self.build_deploy_msg(
 rho_code.to_string(),
 phlo_limit,
 language.to_string(),
 current_block,
 expiration_timestamp,
 None,
 );

 let connect_start = Instant::now();
 let mut deploy_service_client =
 DeployServiceClient::connect(self.grpc_url()).await?;
 tracing::debug!(elapsed = ?connect_start.elapsed(), "gRPC connect");

 let do_deploy_start = Instant::now();
 let deploy_response = deploy_service_client.do_deploy(deployment).await?;
 tracing::debug!(elapsed = ?do_deploy_start.elapsed(), "do_deploy RPC");

 let deploy_message = deploy_response
 .get_ref()
 .message
 .as_ref()
 .ok_or("Deploy result not found")?;

 match deploy_message {
 DeployResponseMessage::Error(service_error) => Err(service_error.clone().into()),
 DeployResponseMessage::Result(result) => Self::extract_deploy_id(result),
 }
 }

 pub async fn propose(&self) -> Result<ProposeResult, Box<dyn std::error::Error>> {
 let mut propose_client =
 ProposeServiceClient::connect(self.grpc_url()).await?;

 let propose_response = propose_client
 .propose(ProposeQuery { is_async: false })
 .await?
 .into_inner();

 let message = propose_response.message.ok_or("Missing propose response")?;

 match message {
 ProposeResponseMessage::Result(block_hash) => {
 if let Some(hash) = block_hash
 .strip_prefix("Success! Block ")
 .and_then(|s| s.strip_suffix(" created and added."))
 {
 Ok(ProposeResult::Proposed(hash.to_string()))
 } else if Self::is_recoverable_propose_error(&block_hash) {
 Ok(ProposeResult::Skipped(block_hash))
 } else {
 Ok(ProposeResult::Proposed(block_hash))
 }
 }
 ProposeResponseMessage::Error(error) => {
 let error_message = error.messages.join("; ");
 if Self::is_recoverable_propose_error(&error_message) {
 Ok(ProposeResult::Skipped(error_message))
 } else {
 Err(format!("Propose error: {:?}", error).into())
 }
 }
 }
 }

 pub async fn full_deploy(
 &self,
 rho_code: &str,
 use_bigger_phlo_price: bool,
 language: &str,
 expiration_timestamp: i64,
 ) -> Result<ProposeResult, Box<dyn std::error::Error>> {
 self.deploy(rho_code, use_bigger_phlo_price, language, expiration_timestamp)
 .await?;
 self.propose().await
 }

 pub async fn deploy_with_phlo_limit(
 &self,
 rho_code: &str,
 phlo_limit: i64,
 language: &str,
 ) -> Result<String, Box<dyn std::error::Error>> {
 self.deploy_internal(rho_code, phlo_limit, language, 0, None).await
 }

 pub async fn deploy_with_timestamp_and_phlo_limit(
 &self,
 rho_code: &str,
 language: &str,
 timestamp_millis: Option<i64>,
 phlo_limit: i64,
 ) -> Result<String, Box<dyn std::error::Error>> {
 self.deploy_internal(rho_code, phlo_limit, language, 0, timestamp_millis).await
 }

 pub(crate) async fn deploy_internal(
 &self,
 rho_code: &str,
 phlo_limit: i64,
 language: &str,
 expiration_timestamp: i64,
 timestamp_override: Option<i64>,
 ) -> Result<String, Box<dyn std::error::Error>> {
 let current_block = self.get_current_block_number().await.unwrap_or(0);

 let deployment = self.build_deploy_msg(
 rho_code.to_string(),
 phlo_limit,
 language.to_string(),
 current_block,
 expiration_timestamp,
 timestamp_override,
 );

 let mut client = DeployServiceClient::connect(self.grpc_url()).await?;
 let deploy_response = client.do_deploy(deployment).await?;

 let deploy_message = deploy_response
 .get_ref()
 .message
 .as_ref()
 .ok_or("Deploy result not found")?;

 match deploy_message {
 DeployResponseMessage::Error(service_error) => Err(service_error.clone().into()),
 DeployResponseMessage::Result(result) => Self::extract_deploy_id(result),
 }
 }

 fn extract_deploy_id(result: &str) -> Result<String, Box<dyn std::error::Error>> {
 let cleaned = result.trim();
 if let Some(id) = cleaned.strip_prefix("Success! DeployId is: ") {
 Ok(id.trim().to_string())
 } else if let Some(id) = cleaned.strip_prefix("Success!\nDeployId is: ") {
 Ok(id.trim().to_string())
 } else if cleaned.starts_with("Success!") {
 for line in cleaned.lines() {
 let trimmed = line.trim();
 if trimmed.len() > 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
 return Ok(trimmed.to_string());
 }
 }
 Err(format!("Could not extract deploy ID from response: {}", result).into())
 } else {
 Ok(cleaned.to_string())
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
 RECOVERABLE_PATTERNS.iter().any(|p| normalized.contains(p))
 }

 pub(crate) fn build_deploy_msg(
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

 let projection = DeployDataProto {
 term: code.clone(),
 timestamp,
 phlo_price: 1,
 phlo_limit,
 valid_after_block_number,
 shard_id: "root".into(),
 language: String::new(),
 sig: ByteString::new(),
 deployer: ByteString::new(),
 sig_algorithm: String::new(),
 expiration_timestamp,
 };

 let serialized = projection.encode_to_vec();
 let digest = blake2b_256_hash(&serialized);

 let secp = Secp256k1::new();
 let message = Secp256k1Message::from_digest(digest.into());
 let signature = secp.sign_ecdsa(message, &self.signing_key);
 let sig_bytes = signature.serialize_der().to_vec();
 let public_key = self.signing_key.public_key(&secp);
 let pub_key_bytes = public_key.serialize_uncompressed().to_vec();

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

fn blake2b_256_hash(data: &[u8]) -> [u8; 32] {
 let mut blake = Blake2b::<U32>::new();
 blake.update(data);
 let hash = blake.finalize();
 let mut result = [0u8; 32];
 result.copy_from_slice(&hash);
 result
}
