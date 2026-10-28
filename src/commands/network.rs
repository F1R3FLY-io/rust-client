use crate::args::*;
use crate::commands::CliResult;
use crate::connection_manager::{ConnectionConfig, F1r3flyConnectionManager};
use crate::f1r3fly_api::{DeployResult, ProposeResult};
use crate::pos::{build_bond_rholang, parse_pos_call_result, WITHDRAW_RHOLANG};
use std::fs;
use std::time::Instant;

fn build_config(
    wait: &DeployWaitArgs,
    private_key: &str,
    check_interval: u64,
) -> CliResult<ConnectionConfig> {
    let transport = &wait.node.transport;
    let mut config = ConnectionConfig::new(
        transport.apply(&wait.node.grpc)?,
        transport.apply(&wait.node.http)?,
        private_key.to_string(),
    );
    if let Some(observer) = &wait.observer {
        config = config.with_observer(transport.apply(observer)?);
    }
    config.finalization_timeout_secs = wait.max_wait as u32;
    config.poll_interval_secs = check_interval;
    config.shard_id = wait.deploy.shard_id.clone();
    Ok(config)
}

fn connect(
    wait: &DeployWaitArgs,
    private_key: &str,
    check_interval: u64,
) -> CliResult<F1r3flyConnectionManager> {
    Ok(F1r3flyConnectionManager::new(build_config(
        wait,
        private_key,
        check_interval,
    )?)?)
}

/// Propose a block on the deploy node, reporting rather than failing: the
/// deploy has already finalized by the time this runs.
async fn propose_after(manager: &F1r3flyConnectionManager) {
    match manager.api().propose().await {
        Ok(ProposeResult::Proposed(hash)) => println!("Block proposed: {hash}"),
        Ok(ProposeResult::Skipped(reason)) => println!("Propose skipped: {reason}"),
        Err(e) => println!("Propose failed: {e}"),
    }
}

pub async fn exploratory_deploy_command(args: &ExploratoryDeployArgs) -> CliResult {
    // Read the Rholang code from file
    println!(" Reading Rholang from: {}", args.file.display());
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {e}"))?;
    println!(" Code size: {} bytes", rholang_code.len());

    let f1r3fly_api = args.node.api()?;
    println!(" Connecting to F1r3fly node at {}", f1r3fly_api.endpoint());

    // Execute the exploratory deployment
    println!(" Executing Rholang code (exploratory deploy)...");

    // Display block hash if provided
    if let Some(block_hash) = &args.block_hash {
        println!(" Using block hash: {block_hash}");
    }

    // Display state hash preference
    if args.use_pre_state {
        println!(" Using pre-state hash");
    } else {
        println!(" Using post-state hash");
    }

    let start_time = Instant::now();

    match f1r3fly_api
        .exploratory_deploy(
            &rholang_code,
            args.block_hash.as_deref(),
            args.use_pre_state,
        )
        .await
    {
        Ok((result, block_info, cost)) => {
            let duration = start_time.elapsed();
            println!("Execution successful!");
            println!("Cost:    {cost} phlogiston");
            println!("Time:    {duration:.2?}");
            println!("{block_info}");
            println!("Result:");
            println!("{result}");
        }
        Err(e) => {
            println!("Execution failed!");
            println!("Error: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

pub async fn estimate_cost_command(args: &EstimateCostArgs) -> CliResult {
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {e}"))?;

    let deployer_hex = resolve_deployer(&args.deployer, &args.private_key)?;

    if deployer_hex.is_none() {
        eprintln!(
            " Warning: no deployer identity supplied. The estimate may be significantly \
             lower than the real cost for identity-dependent terms (e.g. vault transfers). \
             Pass --deployer <PUBKEY> or --private-key <PRIVKEY> to fix this."
        );
    }

    let response = args
        .node
        .node()?
        .estimate_cost(
            &rholang_code,
            deployer_hex.as_deref(),
            args.block_hash.as_deref(),
        )
        .await?;

    println!("{response}");

    Ok(())
}

/// Resolve the deployer public key hex from CLI arguments.
///
/// Priority: `--deployer` directly, or derive from `--private-key`, or `None`.
fn resolve_deployer(
    deployer: &Option<String>,
    private_key: &Option<String>,
) -> CliResult<Option<String>> {
    if let Some(pk) = deployer {
        return Ok(Some(pk.clone()));
    }
    if let Some(sk_hex) = private_key {
        let secret_key = crate::utils::CryptoUtils::decode_private_key(sk_hex)?;
        let public_key = crate::utils::CryptoUtils::derive_public_key(&secret_key);
        let pub_hex = crate::utils::CryptoUtils::serialize_public_key(&public_key, false);
        return Ok(Some(pub_hex));
    }
    Ok(None)
}

pub async fn deploy_command(args: &DeployArgs) -> CliResult {
    // Read the Rholang code from file
    println!("Reading Rholang from: {}", args.file.display());
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {e}"))?;
    println!("Code size: {} bytes", rholang_code.len());

    let f1r3fly_api = args
        .node
        .api()?
        .with_signing_key(&args.private_key)?
        .with_shard_id(&args.deploy.shard_id);
    println!("Connecting to F1r3fly node at {}", f1r3fly_api.endpoint());
    let options = args.deploy.options(args.phlo_limit);
    println!("Using phlo limit: {}", options.phlo_limit);
    if let Some(expiration) = options.expiration {
        println!("Deploy expiration: {expiration} ms");
    }

    // Deploy the Rholang code
    println!("Deploying Rholang code...");
    let start_time = Instant::now();

    match f1r3fly_api.deploy(&rholang_code, &options).await {
        Ok(deploy_id) => {
            let duration = start_time.elapsed();
            println!("Deployment successful!");
            println!("Time taken: {duration:.2?}");
            println!("Deploy ID: {deploy_id}");
        }
        Err(e) => {
            println!("Deployment failed!");
            println!("Error: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

pub async fn propose_command(args: &ProposeArgs) -> CliResult {
    let f1r3fly_api = args.api()?;
    println!(" Connecting to F1r3fly node at {}", f1r3fly_api.endpoint());

    // Propose a block
    println!(" Proposing a new block...");
    let start_time = Instant::now();

    match f1r3fly_api.propose().await {
        Ok(ProposeResult::Proposed(block_hash)) => {
            let duration = start_time.elapsed();
            println!(" Block proposed successfully!");
            println!(" Block hash: {block_hash}");
            println!(" Time taken: {duration:.2?}");
        }
        Ok(ProposeResult::Skipped(reason)) => {
            let duration = start_time.elapsed();
            println!(" Proposal was skipped: {reason}");
            println!(" Time taken: {duration:.2?}");
        }
        Err(e) => {
            println!(" Block proposal failed!");
            println!("Error: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

pub async fn is_finalized_command(args: &IsFinalizedArgs) -> CliResult {
    let f1r3fly_api = args.node.api()?;
    println!(" Connecting to F1r3fly node at {}", f1r3fly_api.endpoint());

    // Check if the block is finalized
    println!(" Checking if block is finalized: {}", args.block_hash);
    println!(
        " Will retry every {} seconds, up to {} times",
        args.retry_delay, args.max_attempts
    );
    let start_time = Instant::now();

    match f1r3fly_api
        .is_finalized(&args.block_hash, args.max_attempts, args.retry_delay)
        .await
    {
        Ok(is_finalized) => {
            let duration = start_time.elapsed();
            if is_finalized {
                println!(" Block is finalized!");
            } else {
                println!(
                    " Block is not finalized after {} attempts",
                    args.max_attempts
                );
            }
            println!(" Time taken: {duration:.2?}");
        }
        Err(e) => {
            println!(" Error checking block finalization!");
            println!("Error: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

pub async fn bond_validator_command(args: &BondValidatorArgs) -> CliResult {
    println!("Bonding validator with stake: {}", args.stake);

    let manager = connect(&args.wait, &args.private_key, args.check_interval)?;
    let start = Instant::now();

    let result = manager
        .deploy_and_wait(
            &build_bond_rholang(&crate::vault::parse_amount(&args.stake)?)?,
            &args.wait.deploy.options(args.phlo_limit),
        )
        .await?;

    println!("Deploy ID: {}", result.deploy_id);
    println!("Block hash: {}", result.block_hash);
    println!("Total time: {:.2?}", start.elapsed());

    ensure_pos_call_succeeded("Bond", &result)?;

    if args.propose {
        propose_after(&manager).await;
    }

    println!("Bonding complete. Verify with: node_cli bonds --http <read-only node>");
    Ok(())
}

pub async fn unbond_validator_command(args: &UnbondValidatorArgs) -> CliResult {
    use crate::utils::CryptoUtils;

    let public_key = {
        let secret_key = CryptoUtils::decode_private_key(&args.private_key)?;
        CryptoUtils::serialize_public_key(&CryptoUtils::derive_public_key(&secret_key), false)
    };
    println!("Requesting withdrawal for validator: {public_key}");

    let manager = connect(&args.wait, &args.private_key, args.check_interval)?;
    let start = Instant::now();

    let result = manager
        .deploy_and_wait(WITHDRAW_RHOLANG, &args.wait.deploy.options(args.phlo_limit))
        .await?;

    println!("Deploy ID: {}", result.deploy_id);
    println!("Block hash: {}", result.block_hash);
    println!("Total time: {:.2?}", start.elapsed());

    ensure_pos_call_succeeded("Withdrawal", &result)?;

    println!("Withdrawal requested. The validator leaves the bond set at the next epoch boundary.");
    println!(
        "Track progress with: node_cli validator-status -k {public_key} --http <read-only node>"
    );
    Ok(())
}

/// Fail when a PoS method deploy errored, its verdict could not be read, or the
/// contract rejected the call.
fn ensure_pos_call_succeeded(action: &str, result: &DeployResult) -> CliResult {
    let detail = result
        .detail
        .as_ref()
        .map_err(|err| format!("{action} outcome could not be read: {err}"))?;
    if let Some(failure) = detail.execution_failure() {
        return Err(format!("{action} deploy failed: {failure}").into());
    }
    let data = result
        .data
        .as_ref()
        .map_err(|err| format!("{action} verdict could not be read: {err}"))?;
    Ok(parse_pos_call_result(data)?)
}

pub async fn transfer_command(args: &TransferArgs) -> CliResult {
    use crate::utils::CryptoUtils;

    // Derive sender address
    let from_address = {
        let secret_key = CryptoUtils::decode_private_key(&args.private_key)?;
        let public_key = CryptoUtils::derive_public_key(&secret_key);
        let public_key_hex = CryptoUtils::serialize_public_key(&public_key, false);
        CryptoUtils::generate_vault_address(&public_key_hex)?
    };

    validate_vault_address(&from_address)?;
    validate_vault_address(&args.to_address)?;

    let manager = connect(&args.wait, &args.private_key, args.check_interval)?;

    let amount_dust = if args.whole_tokens {
        // Whole-token mode: ask the node how many decimals the native token has.
        let status_json = manager.http().get_json("/status").await?;
        let decimals = status_json
            .get("nativeTokenDecimals")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok())
            .ok_or("node did not report valid token decimals; cannot convert whole tokens")?;
        crate::vault::tokens_to_dust(&args.amount, decimals)?
    } else {
        crate::vault::parse_amount(&args.amount)?
    };

    println!(
        "Transfer: {} -> {} ({} dust)",
        from_address, args.to_address, amount_dust
    );

    let rholang_code =
        crate::vault::build_transfer_rholang(&from_address, &args.to_address, &amount_dust)?;

    let start = Instant::now();

    let result = manager
        .deploy_and_wait(&rholang_code, &args.wait.deploy.options(args.phlo_limit))
        .await?;

    println!("Deploy ID: {}", result.deploy_id);
    println!("Block hash: {}", result.block_hash);
    if let Ok(detail) = &result.detail {
        println!("Cost: {}", detail.cost);
    }
    println!("Total time: {:.2?}", start.elapsed());

    crate::vault::check_transfer_result(result)?;

    if args.propose {
        propose_after(&manager).await;
    }

    println!("Transfer complete.");
    Ok(())
}

pub async fn deploy_and_wait_command(args: &DeployAndWaitArgs) -> CliResult {
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {e}"))?;

    let manager = connect(&args.wait, &args.private_key, args.check_interval)?;

    println!("Deploying and waiting for finalization...");
    let start = Instant::now();

    let result = manager
        .deploy_and_wait(&rholang_code, &args.wait.deploy.options(args.phlo_limit))
        .await?;

    println!("Deploy ID: {}", result.deploy_id);
    println!("Block hash: {}", result.block_hash);
    match &result.detail {
        Ok(detail) => {
            println!("Block number: {}", detail.block_number);
            println!("Cost: {}", detail.cost);
            if let Some(failure) = detail.execution_failure() {
                println!("Errored: true");
                println!("Deploy error: {failure}");
            }
        }
        Err(err) => println!("Execution: unreadable ({err})"),
    }
    match &result.data {
        Ok(data) if data.is_empty() => println!("Data: (none)"),
        Ok(data) => {
            for (i, par) in data.iter().enumerate() {
                let simplified =
                    crate::f1r3fly_api::extract_par_data(par).unwrap_or_else(|| format!("{par:?}"));
                println!("Data[{i}]: {simplified}");
            }
        }
        Err(err) => println!("Data: unreadable ({err})"),
    }
    println!("Total time: {:.2?}", start.elapsed());

    if args.propose {
        propose_after(&manager).await;
    }

    result.detail?;
    result.data?;
    Ok(())
}

pub async fn get_deploy_command(args: &GetDeployArgs) -> CliResult {
    let node = args.node.node()?;
    let start_time = Instant::now();

    // Try detail view first (Rust node with PR #472+)
    if let Ok(Some(detail)) = node.get_deploy_detail(&args.deploy_id).await {
        let duration = start_time.elapsed();
        match args.format.as_str() {
            "json" => {
                println!("{}", serde_json::to_string_pretty(&detail)?);
            }
            "summary" => {
                println!(
                    "Deploy {} in block {} (#{}) cost={} errored={}",
                    args.deploy_id,
                    detail.block_hash,
                    detail.block_number,
                    detail.cost,
                    detail.errored
                );
            }
            _ => {
                println!("Deploy Information");
                println!("----------------------------------------");
                println!("Deploy ID:    {}", detail.deploy_id);
                println!("Block Hash:   {}", detail.block_hash);
                println!("Block Number: {}", detail.block_number);
                println!("Finalized:    {}", detail.is_finalized);
                if let Some(ref deployer) = detail.deployer {
                    println!("Deployer:     {deployer}");
                }
                println!("Cost:         {}", detail.cost);
                println!("Errored:      {}", detail.errored);
                if let Some(ref err) = detail.system_deploy_error {
                    if !err.is_empty() {
                        println!("Error:        {err}");
                    }
                }
                if let Some(price) = detail.phlo_price {
                    println!("Phlo Price:   {price}");
                }
                if let Some(limit) = detail.phlo_limit {
                    println!("Phlo Limit:   {limit}");
                }
                println!("Timestamp:    {}", detail.timestamp);
                if let Some(ref algo) = detail.sig_algorithm {
                    println!("Sig Algo:     {algo}");
                }
                if args.verbose {
                    if let Some(vabn) = detail.valid_after_block_number {
                        println!("VABN:         {vabn}");
                    }
                }
                println!("Query time:   {duration:.2?}");
            }
        }
        return Ok(());
    }

    // Fall back to default view (works on all nodes)
    match node.get_deploy(&args.deploy_id).await {
        Ok(Some(json)) => {
            let duration = start_time.elapsed();
            match args.format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                _ => {
                    println!("Deploy Information (basic view)");
                    println!("----------------------------------------");
                    println!("Deploy ID:    {}", args.deploy_id);
                    if let Some(hash) = json.get("blockHash").and_then(|v| v.as_str()) {
                        println!("Block Hash:   {hash}");
                    }
                    if let Some(num) = json.get("blockNumber").and_then(|v| v.as_i64()) {
                        println!("Block Number: {num}");
                    }
                    if let Some(sender) = json.get("sender").and_then(|v| v.as_str()) {
                        println!("Sender:       {sender}");
                    }
                    if let Some(ts) = json.get("timestamp").and_then(|v| v.as_i64()) {
                        println!("Timestamp:    {ts}");
                    }
                    println!("Query time:   {duration:.2?}");
                    println!();
                    println!(
                        "Note: deploy execution details (cost, errored) require Rust node v0.4.11+"
                    );
                }
            }
        }
        Ok(None) => {
            println!("Deploy {} not found", args.deploy_id);
        }
        Err(e) => {
            println!("Error retrieving deploy information: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

fn validate_vault_address(address: &str) -> CliResult {
    if !address.starts_with("1111") {
        return Err("Invalid vault address format: must start with '1111'".into());
    }

    if address.len() < 40 {
        return Err("Invalid vault address format: too short".into());
    }

    Ok(())
}

/// Read data at a deploy ID from a specific block
pub async fn get_data_command(args: &GetDataArgs) -> CliResult {
    let pars = args
        .node
        .api()?
        .get_data_at_deploy_id(&args.deploy_id, &args.block_hash)
        .await?;

    if pars.is_empty() {
        println!(
            "No data found for deploy {} at block {}",
            args.deploy_id, args.block_hash
        );
    } else {
        for (i, par) in pars.iter().enumerate() {
            let simplified =
                crate::f1r3fly_api::extract_par_data(par).unwrap_or_else(|| format!("{par:?}"));
            println!("{simplified}");
            if i < pars.len() - 1 {
                println!("---");
            }
        }
    }

    Ok(())
}

/// Check canonical-state finalization status of a deploy by signature.
///
/// Hits the node's `/api/deploy-finalization-status/{sig}` endpoint and
/// renders the result. State is one of `Finalized`, `Failed`, `Pending`,
/// `Expired` — see `DeployFinalizationStatus` for terminality semantics.
pub async fn deploy_status_command(args: &DeployStatusArgs) -> CliResult {
    let sig_hex = args.sig.trim_start_matches("0x");

    let node = args.node.node()?;
    let start = Instant::now();

    let status = match node.deploy_finalization_status(sig_hex).await? {
        Some(s) => s,
        None => {
            return Err(node.finalization_status_unserved().await.into());
        }
    };
    let duration = start.elapsed();

    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        _ => {
            println!("Deploy Finalization Status");
            println!("----------------------------------------");
            println!("Sig:              {sig_hex}");
            println!("State:            {}", status.state);
            println!("Rejection count:  {}", status.rejection_count);
            match status.latest_block_hash.as_deref() {
                Some(hash) => println!("Latest block:     {hash}"),
                None => println!("Latest block:     (not yet included)"),
            }
            println!("Query time:       {duration:.2?}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use f1r3fly_models::rhoapi::expr::ExprInstance;
    use f1r3fly_models::rhoapi::{ETuple, Expr, Par};

    fn finalized(data: crate::Result<Vec<Par>>) -> DeployResult {
        DeployResult {
            deploy_id: "3045".to_string(),
            block_hash: "abcd".to_string(),
            detail: Ok(crate::f1r3fly_api::DeployDetail::finalized("3045", false)),
            data,
        }
    }

    fn verdict(accepted: bool) -> Vec<Par> {
        let flag = Par {
            exprs: vec![Expr {
                expr_instance: Some(ExprInstance::GBool(accepted)),
            }],
            ..Default::default()
        };
        vec![Par {
            exprs: vec![Expr {
                expr_instance: Some(ExprInstance::ETupleBody(ETuple {
                    ps: vec![flag, Par::default()],
                    ..Default::default()
                })),
            }],
            ..Default::default()
        }]
    }

    #[test]
    fn an_unreadable_verdict_is_not_reported_as_a_rejection() {
        let err = ensure_pos_call_succeeded(
            "Withdrawal",
            &finalized(Err(crate::Error::transport(
                "http://localhost:40401",
                crate::TransportError::TimedOut(std::time::Duration::from_secs(1)),
            ))),
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("could not be read"), "{err}");
        assert!(err.contains("no answer within 1s"), "{err}");
        assert!(!err.contains("rejected"), "{err}");
    }

    fn endpoint(url: &str) -> crate::Endpoint {
        crate::Endpoint::parse(url).unwrap()
    }

    #[test]
    fn endpoint_flags_reach_the_connection_config() {
        use clap::Parser;

        let args = DeployAndWaitArgs::try_parse_from([
            "deploy-and-wait",
            "-f",
            "contract.rho",
            "-k",
            "00",
            "--grpc",
            "https://node.example:443",
            "--http",
            "https://node.example:8443",
            "--observer",
            "http://observer.example:40403",
        ])
        .unwrap();
        let config = build_config(&args.wait, &args.private_key, args.check_interval).unwrap();
        assert_eq!(config.grpc, endpoint("https://node.example:443"));
        assert_eq!(config.http, endpoint("https://node.example:8443"));
        assert_eq!(
            config.observer,
            Some(endpoint("http://observer.example:40403"))
        );
    }

    #[test]
    fn without_observer_flag_no_observer_is_polled() {
        use clap::Parser;

        let args =
            DeployAndWaitArgs::try_parse_from(["deploy-and-wait", "-f", "c.rho", "-k", "00"])
                .unwrap();
        let config = build_config(&args.wait, &args.private_key, args.check_interval).unwrap();
        assert_eq!(config.grpc, endpoint("http://localhost:40401"));
        assert_eq!(config.http, endpoint("http://localhost:40403"));
        assert_eq!(config.observer, None);
    }

    #[test]
    fn port_flags_are_gone_and_endpoints_are_validated() {
        use clap::Parser;

        for args in [
            &["deploy-and-wait", "-f", "c.rho", "-k", "00", "-p", "40401"][..],
            &[
                "deploy-and-wait",
                "-f",
                "c.rho",
                "-k",
                "00",
                "--http-port",
                "40403",
            ],
            &[
                "deploy-and-wait",
                "-f",
                "c.rho",
                "-k",
                "00",
                "--observer-port",
                "1",
            ],
            &[
                "deploy-and-wait",
                "-f",
                "c.rho",
                "-k",
                "00",
                "--grpc",
                "localhost:40401",
            ],
        ] {
            assert!(
                DeployAndWaitArgs::try_parse_from(args).is_err(),
                "{args:?} was accepted"
            );
        }
    }

    #[test]
    fn a_read_verdict_decides_the_outcome() {
        assert!(ensure_pos_call_succeeded("Bond", &finalized(Ok(verdict(true)))).is_ok());

        let err = ensure_pos_call_succeeded("Bond", &finalized(Ok(verdict(false)))).unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<crate::Error>(),
                Some(crate::Error::ContractRejected {
                    contract: crate::Contract::ProofOfStake,
                    ..
                })
            ),
            "{err}"
        );
    }
}
