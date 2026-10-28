//! Query operations (exploratory deploy, data reads, deploy lookup)

use super::{rejected, F1r3flyApi};
use crate::error::{Error, Result};
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
    ) -> Result<(String, String, u64)> {
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

        let resp = response.get_ref();
        let cost = resp.cost;

        let message = resp
            .message
            .as_ref()
            .ok_or_else(|| Error::decode("exploratory deploy response", "empty message"))?;

        match message {
            ExploratoryDeployResponseMessage::Error(error) => {
                Err(rejected("the exploratory deploy", error))
            }
            ExploratoryDeployResponseMessage::Result(result) => {
                let data = if !result.post_block_data.is_empty() {
                    result
                        .post_block_data
                        .iter()
                        .enumerate()
                        .map(|(i, par)| {
                            extract_par_data(par).unwrap_or_else(|| {
                                format!("Result {}: Complex data structure", i + 1)
                            })
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    "No data returned".to_string()
                };

                let block_info = result
                    .block
                    .as_ref()
                    .map(|b| {
                        format!(
                            "Block hash: {}, Block number: {}",
                            b.block_hash, b.block_number
                        )
                    })
                    .unwrap_or_else(|| "No block info".to_string());

                Ok((data, block_info, cost))
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

pub fn extract_par_data(par: &Par) -> Option<String> {
    use f1r3fly_models::rhoapi::expr::ExprInstance;

    if !par.exprs.is_empty() && par.exprs[0].expr_instance.is_some() {
        let expr = &par.exprs[0];
        if let Some(instance) = &expr.expr_instance {
            match instance {
                ExprInstance::GString(s) => Some(format!("\"{s}\"")),
                ExprInstance::GUri(u) => Some(format!("`{u}`")),
                ExprInstance::GInt(i) => Some(i.to_string()),
                ExprInstance::GBigInt(bytes) => {
                    Some(num_bigint::BigInt::from_signed_bytes_be(bytes).to_string())
                }
                ExprInstance::GBool(b) => Some(b.to_string()),
                ExprInstance::GByteArray(bytes) => Some(format!("0x{}", hex::encode(bytes))),
                ExprInstance::EListBody(list) => {
                    let items: Vec<String> = list
                        .ps
                        .iter()
                        .map(|p| extract_par_data(p).unwrap_or_else(|| "?".to_string()))
                        .collect();
                    Some(format!("[{}]", items.join(", ")))
                }
                ExprInstance::ETupleBody(tup) => {
                    let items: Vec<String> = tup
                        .ps
                        .iter()
                        .map(|p| extract_par_data(p).unwrap_or_else(|| "?".to_string()))
                        .collect();
                    Some(format!("({})", items.join(", ")))
                }
                ExprInstance::ESetBody(set) => {
                    let items: Vec<String> = set
                        .ps
                        .iter()
                        .map(|p| extract_par_data(p).unwrap_or_else(|| "?".to_string()))
                        .collect();
                    Some(format!("Set({{{}}})", items.join(", ")))
                }
                ExprInstance::EMapBody(map) => {
                    let items: Vec<String> = map
                        .kvs
                        .iter()
                        .map(|kv| {
                            let k = kv
                                .key
                                .as_ref()
                                .and_then(extract_par_data)
                                .unwrap_or_else(|| "?".to_string());
                            let v = kv
                                .value
                                .as_ref()
                                .and_then(extract_par_data)
                                .unwrap_or_else(|| "?".to_string());
                            format!("{k}: {v}")
                        })
                        .collect();
                    Some(format!("{{{}}}", items.join(", ")))
                }
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

#[cfg(test)]
mod bigint_tests {
    use super::*;
    use f1r3fly_models::rhoapi::{expr::ExprInstance, EList, Expr};
    use num_bigint::BigInt;

    #[test]
    fn decodes_signed_bigints_and_nested_values_exactly() {
        for value in [
            BigInt::from(0),
            (BigInt::from(1) << 200usize) + 1,
            -(BigInt::from(1) << 200usize) - 1,
        ] {
            let par = Par {
                exprs: vec![Expr {
                    expr_instance: Some(ExprInstance::GBigInt(value.to_signed_bytes_be())),
                }],
                ..Default::default()
            };
            assert_eq!(extract_par_data(&par), Some(value.to_string()));
            let list = Par {
                exprs: vec![Expr {
                    expr_instance: Some(ExprInstance::EListBody(EList {
                        ps: vec![par],
                        ..Default::default()
                    })),
                }],
                ..Default::default()
            };
            assert_eq!(extract_par_data(&list), Some(format!("[{value}]")));
        }
    }
}
