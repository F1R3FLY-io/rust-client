//! Query operations (exploratory deploy, data reads, deploy lookup)

use super::{rejected, F1r3flyApi};
use crate::error::{Error, Result};
use crate::f1r3fly_api::{BlockRef, ExploratoryResult};
use crate::utils::http::EXPLORATORY_RETRY_BACKOFF;
use f1r3fly_models::casper::v1::exploratory_deploy_response::Message as ExploratoryDeployResponseMessage;
use f1r3fly_models::casper::v1::rho_data_response;
use f1r3fly_models::casper::{DataAtNameByBlockQuery, ExploratoryDeployQuery};
use f1r3fly_models::rhoapi::g_unforgeable::UnfInstance;
use f1r3fly_models::rhoapi::{GDeployId, GUnforgeable, Par};

/// Report whether the node rejected an exploratory query because its
/// exploratory-query capacity was occupied.
///
/// The check is narrower than the status code alone. `Unavailable` also covers a
/// node that is down, and retrying that case for the full backoff would only
/// delay the failure rather than recover from it.
fn is_exploratory_capacity_rejection(status: &tonic::Status) -> bool {
    status.code() == tonic::Code::Unavailable
        && status.message().contains("exploratory query capacity")
}

impl F1r3flyApi {
    pub async fn exploratory_deploy(
        &self,
        rho_code: &str,
        block_hash: Option<&str>,
        use_pre_state_hash: bool,
    ) -> Result<ExploratoryResult> {
        let mut client = self.deploy_client().await?;

        let query = ExploratoryDeployQuery {
            term: rho_code.to_string(),
            block_hash: block_hash.unwrap_or("").to_string(),
            use_pre_state_hash,
        };

        let mut attempt = 0;
        let retry_started = std::time::Instant::now();
        let response = loop {
            match client.exploratory_deploy(query.clone()).await {
                Ok(response) => break response,
                Err(status) if is_exploratory_capacity_rejection(&status) => {
                    let delay = EXPLORATORY_RETRY_BACKOFF.get(attempt).copied();
                    let within_budget = delay.is_some_and(|delay| {
                        self.exploratory_retry_budget
                            .is_none_or(|budget| retry_started.elapsed() + delay <= budget)
                    });
                    if !within_budget {
                        return Err(self.grpc_error(status));
                    }
                    tokio::time::sleep(delay.expect("checked delay")).await;
                    attempt += 1;
                }
                Err(status) => return Err(self.grpc_error(status)),
            }
        };

        let response = response.into_inner();
        match response
            .message
            .ok_or_else(|| Error::decode("exploratory deploy response", "empty message"))?
        {
            ExploratoryDeployResponseMessage::Error(error) => {
                Err(rejected("the exploratory deploy", &error))
            }
            ExploratoryDeployResponseMessage::Result(result) => {
                let block = result.block.ok_or_else(|| {
                    Error::decode(
                        "exploratory deploy response",
                        "it does not name the block it read",
                    )
                })?;
                Ok(ExploratoryResult {
                    data: result.post_block_data,
                    block: BlockRef {
                        hash: block.block_hash,
                        number: block.block_number,
                    },
                    cost: response.cost,
                })
            }
        }
    }

    pub async fn get_data_at_deploy_id(
        &self,
        deploy_id: &str,
        block_hash: &str,
    ) -> Result<Vec<Par>> {
        let deploy_id_bytes = hex::decode(deploy_id).map_err(|e| Error::invalid("deploy ID", e))?;
        let par = Par {
            unforgeables: vec![GUnforgeable {
                unf_instance: Some(UnfInstance::GDeployIdBody(GDeployId {
                    sig: deploy_id_bytes,
                })),
            }],
            ..Default::default()
        };

        let mut client = self.deploy_client().await?;

        let response = client
            .get_data_at_name(DataAtNameByBlockQuery {
                par: Some(par),
                block_hash: block_hash.to_string(),
                use_pre_state_hash: false,
            })
            .await
            .map_err(|status| self.grpc_error(status))?
            .into_inner();

        match response.message {
            Some(rho_data_response::Message::Payload(payload)) => Ok(payload.par),
            Some(rho_data_response::Message::Error(error)) => {
                Err(rejected("the getDataAtName read", &error))
            }
            None => Err(Error::decode("getDataAtName response", "empty message")),
        }
    }
}
