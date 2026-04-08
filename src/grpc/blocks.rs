//! Block queries, finalization checks, and tip sampling

use super::F1r3flyApi;
use f1r3fly_models::casper::v1::deploy_service_client::DeployServiceClient;
use f1r3fly_models::casper::v1::is_finalized_response::Message as IsFinalizedResponseMessage;
use f1r3fly_models::casper::{BlocksQuery, BlocksQueryByHeight, IsFinalizedQuery, LightBlockInfo};
use std::sync::atomic::Ordering;

const BLOCK_SAMPLE_DEPTH: u32 = 8;
const TIP_SAMPLE_ATTEMPTS: usize = 2;
const TIP_SAMPLE_TIMEOUT_SECS: u64 = 2;
const TIP_SAMPLE_DELAY_MS: u64 = 50;

impl<'a> F1r3flyApi<'a> {
 pub async fn is_finalized(
 &self,
 block_hash: &str,
 max_attempts: u32,
 retry_delay_sec: u64,
 ) -> Result<bool, Box<dyn std::error::Error>> {
 let mut attempts = 0;

 loop {
 attempts += 1;

 let mut client = DeployServiceClient::connect(self.grpc_url()).await?;

 let query = IsFinalizedQuery {
 hash: block_hash.to_string(),
 };

 match client.is_finalized(query).await {
 Ok(response) => {
 if let Some(message) = &response.get_ref().message {
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

 tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_sec)).await;
 }
 }

 pub async fn show_main_chain(
 &self,
 depth: u32,
 ) -> Result<Vec<LightBlockInfo>, Box<dyn std::error::Error>> {
 use f1r3fly_models::casper::v1::block_info_response::Message;

 let mut client = DeployServiceClient::connect(self.grpc_url()).await?;

 let query = BlocksQuery {
 depth: depth as i32,
 };

 let mut stream = client.show_main_chain(query).await?.into_inner();

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

 pub async fn get_blocks_by_height(
 &self,
 start_block_number: i64,
 end_block_number: i64,
 ) -> Result<Vec<LightBlockInfo>, Box<dyn std::error::Error>> {
 use f1r3fly_models::casper::v1::block_info_response::Message;

 let mut client = DeployServiceClient::connect(self.grpc_url()).await?;

 let query = BlocksQueryByHeight {
 start_block_number,
 end_block_number,
 };

 let mut stream = client.get_blocks_by_heights(query).await?.into_inner();

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

 pub async fn get_current_block_number(&self) -> Result<i64, Box<dyn std::error::Error>> {
 let blocks = self.show_main_chain(1).await?;
 Ok(blocks.first().map(|b| b.block_number).unwrap_or(0))
 }

 pub(crate) async fn get_current_block_number_monotonic(
 &self,
 ) -> Result<i64, Box<dyn std::error::Error>> {
 let sampled_tip = self.get_current_block_number_sampled().await?;
 let cached_tip = self.tip_floor.load(Ordering::Relaxed);
 let selected_tip = if cached_tip != super::TIP_FLOOR_UNSET && sampled_tip < cached_tip {
 tracing::warn!(
 cached_tip, sampled_tip,
 "Tip sample regressed from in-memory floor. Using cached floor."
 );
 cached_tip
 } else {
 sampled_tip
 };
 self.tip_floor.store(selected_tip, Ordering::Relaxed);
 Ok(selected_tip)
 }

 async fn get_current_block_number_sampled(
 &self,
 ) -> Result<i64, Box<dyn std::error::Error>> {
 let mut best_tip: Option<i64> = None;
 let mut min_tip: Option<i64> = None;
 let mut max_tip: Option<i64> = None;
 let mut successful_samples: usize = 0;

 for attempt in 1..=TIP_SAMPLE_ATTEMPTS {
 let sampled_tip = match tokio::time::timeout(
 tokio::time::Duration::from_secs(TIP_SAMPLE_TIMEOUT_SECS),
 self.show_main_chain(BLOCK_SAMPLE_DEPTH),
 )
 .await
 {
 Ok(Ok(blocks)) => blocks.iter().map(|b| b.block_number).max(),
 Ok(Err(err)) => {
 tracing::warn!(attempt, total = TIP_SAMPLE_ATTEMPTS, %err, "Tip sample failed");
 None
 }
 Err(_) => {
 tracing::warn!(attempt, total = TIP_SAMPLE_ATTEMPTS, timeout_secs = TIP_SAMPLE_TIMEOUT_SECS, "Tip sample timed out");
 None
 }
 };

 if let Some(tip) = sampled_tip {
 successful_samples += 1;
 best_tip = Some(best_tip.map_or(tip, |prev| prev.max(tip)));
 min_tip = Some(min_tip.map_or(tip, |prev| prev.min(tip)));
 max_tip = Some(max_tip.map_or(tip, |prev| prev.max(tip)));
 }

 if attempt < TIP_SAMPLE_ATTEMPTS {
 tokio::time::sleep(tokio::time::Duration::from_millis(TIP_SAMPLE_DELAY_MS)).await;
 }
 }

 if let Some(best_tip) = best_tip {
 let min_tip = min_tip.unwrap_or(best_tip);
 let max_tip = max_tip.unwrap_or(best_tip);
 if successful_samples > 1 {
 tracing::debug!(
 successful_samples, min_tip, max_tip, selected = best_tip,
 "Tip sampling complete"
 );
 }
 Ok(best_tip)
 } else {
 Err("Failed to sample current block number from main chain".into())
 }
 }
}
