//! Query operations (exploratory deploy, data reads, deploy lookup)

use super::F1r3flyApi;
use f1r3fly_models::casper::v1::deploy_service_client::DeployServiceClient;
use f1r3fly_models::casper::v1::exploratory_deploy_response::Message as ExploratoryDeployResponseMessage;
use f1r3fly_models::casper::v1::rho_data_response;
use f1r3fly_models::casper::{DataAtNameByBlockQuery, ExploratoryDeployQuery, FindDeployQuery, LightBlockInfo};
use f1r3fly_models::rhoapi::g_unforgeable::UnfInstance;
use f1r3fly_models::rhoapi::{GDeployId, GUnforgeable, Par};

impl<'a> F1r3flyApi<'a> {
    pub async fn exploratory_deploy(
        &self,
        rho_code: &str,
        block_hash: Option<&str>,
        use_pre_state_hash: bool,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        let mut client = DeployServiceClient::connect(self.grpc_url()).await?;

        let query = ExploratoryDeployQuery {
            term: rho_code.to_string(),
            block_hash: block_hash.unwrap_or("").to_string(),
            use_pre_state_hash,
        };

        let response = client.exploratory_deploy(query).await?;

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
                let data = if !result.post_block_data.is_empty() {
                    result
                        .post_block_data
                        .iter()
                        .enumerate()
                        .map(|(i, par)| {
                            extract_par_data(par)
                                .unwrap_or_else(|| format!("Result {}: Complex data structure", i + 1))
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    "No data returned".to_string()
                };

                let block_info = result
                    .block
                    .as_ref()
                    .map(|b| format!("Block hash: {}, Block number: {}", b.block_hash, b.block_number))
                    .unwrap_or_else(|| "No block info".to_string());

                Ok((data, block_info))
            }
        }
    }

    pub async fn get_data_at_deploy_id(
        &self,
        deploy_id: &str,
        block_hash: &str,
    ) -> Result<Vec<Par>, Box<dyn std::error::Error>> {
        let deploy_id_bytes = hex::decode(deploy_id)?;
        let par = Par {
            unforgeables: vec![GUnforgeable {
                unf_instance: Some(UnfInstance::GDeployIdBody(GDeployId {
                    sig: deploy_id_bytes.into(),
                })),
            }],
            ..Default::default()
        };

        let mut client = DeployServiceClient::connect(self.grpc_url()).await?;

        let response = client
            .get_data_at_name(DataAtNameByBlockQuery {
                par: Some(par),
                block_hash: block_hash.to_string(),
                use_pre_state_hash: false,
            })
            .await?
            .into_inner();

        match response.message {
            Some(rho_data_response::Message::Payload(payload)) => Ok(payload.par),
            Some(rho_data_response::Message::Error(err)) => {
                Err(format!("getDataAtName error: {}", err.messages.join("; ")).into())
            }
            None => Err("getDataAtName: empty response".into()),
        }
    }

    pub async fn find_deploy_grpc(
        &self,
        deploy_id: &str,
    ) -> Result<Option<LightBlockInfo>, Box<dyn std::error::Error>> {
        let deploy_id_bytes = hex::decode(deploy_id)?;
        let mut client = DeployServiceClient::connect(self.grpc_url()).await?;

        let response = client
            .find_deploy(FindDeployQuery {
                deploy_id: deploy_id_bytes.into(),
            })
            .await?
            .into_inner();

        use f1r3fly_models::casper::v1::find_deploy_response::Message;
        match response.message {
            Some(Message::BlockInfo(block_info)) => Ok(Some(block_info)),
            Some(Message::Error(_)) => Ok(None),
            None => Ok(None),
        }
    }
}

pub fn extract_par_data(par: &Par) -> Option<String> {
    if !par.exprs.is_empty() && par.exprs[0].expr_instance.is_some() {
        let expr = &par.exprs[0];
        if let Some(instance) = &expr.expr_instance {
            match instance {
                f1r3fly_models::rhoapi::expr::ExprInstance::GString(s) => {
                    Some(format!("\"{}\"", s))
                }
                f1r3fly_models::rhoapi::expr::ExprInstance::GInt(i) => Some(i.to_string()),
                f1r3fly_models::rhoapi::expr::ExprInstance::GBool(b) => Some(b.to_string()),
                _ => Some("Complex expression".to_string()),
            }
        } else {
            None
        }
    } else if !par.sends.is_empty() {
        Some("Send operation".to_string())
    } else if !par.receives.is_empty() {
        Some("Receive operation".to_string())
    } else if !par.news.is_empty() {
        Some("New declaration".to_string())
    } else {
        None
    }
}
