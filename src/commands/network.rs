use crate::args::*;
use crate::connection_manager::{ConnectionConfig, F1r3flyConnectionManager};
use crate::f1r3fly_api::{F1r3flyApi, ProposeResult};
use std::fs;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::args::DEV_PRIVATE_KEY;

fn build_config(
    host: &str,
    port: u16,
    http_port: u16,
    private_key: &str,
    max_wait: u64,
    finalization_timeout: u64,
    check_interval: u64,
    observer_host: Option<&str>,
    observer_port: Option<u16>,
) -> ConnectionConfig {
    let mut config =
        ConnectionConfig::new(host.to_string(), port, http_port, private_key.to_string());
    config.deploy_timeout_secs = max_wait as u32;
    config.finalization_timeout_secs = finalization_timeout as u32;
    config.poll_interval_secs = check_interval;
    if let Some(obs_host) = observer_host {
        config.observer_host = Some(obs_host.to_string());
    }
    if let Some(obs_port) = observer_port {
        config.observer_grpc_port = obs_port;
    }
    config
}

fn config_from_deploy_args(args: &DeployAndWaitArgs) -> ConnectionConfig {
    let private_key = args.private_key.as_deref().unwrap_or(DEV_PRIVATE_KEY);
    build_config(
        &args.host,
        args.port,
        args.http_port,
        private_key,
        args.max_wait,
        args.finalization_timeout,
        args.check_interval,
        args.observer_host.as_deref(),
        args.observer_port,
    )
}

fn config_from_transfer_args(args: &TransferArgs) -> ConnectionConfig {
    build_config(
        &args.host,
        args.port,
        args.http_port,
        &args.private_key,
        args.max_wait,
        30,
        args.check_interval,
        args.observer_host.as_deref(),
        args.observer_port,
    )
}

fn config_from_bond_args(args: &BondValidatorArgs) -> ConnectionConfig {
    build_config(
        &args.host,
        args.port,
        args.http_port,
        &args.private_key,
        args.max_wait,
        30,
        args.check_interval,
        args.observer_host.as_deref(),
        args.observer_port,
    )
}

/// Calculates the expiration timestamp from CLI arguments.
/// Returns 0 if no expiration is specified.
fn calculate_expiration_timestamp(expiration: Option<i64>, expires_in: Option<u64>) -> i64 {
    if let Some(exp_ts) = expiration {
        exp_ts
    } else if let Some(duration_secs) = expires_in {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get system time")
            .as_millis() as i64;
        now + (duration_secs as i64 * 1000)
    } else {
        0 // No expiration
    }
}

pub async fn exploratory_deploy_command(
    args: &ExploratoryDeployArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read the Rholang code from file
    println!(" Reading Rholang from: {}", args.file.display());
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {}", e))?;
    println!(" Code size: {} bytes", rholang_code.len());

    // Initialize the F1r3fly API client
    println!(" Connecting to F1r3fly node at {}:{}", args.host, args.port);
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.port)?;

    // Execute the exploratory deployment
    println!(" Executing Rholang code (exploratory deploy)...");

    // Display block hash if provided
    if let Some(block_hash) = &args.block_hash {
        println!(" Using block hash: {}", block_hash);
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
            println!("Cost:    {} phlogiston", cost);
            println!("Time:    {:.2?}", duration);
            println!("{}", block_info);
            println!("Result:");
            println!("{}", result);
        }
        Err(e) => {
            println!("Execution failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub async fn estimate_cost_command(
    args: &ExploratoryDeployArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {}", e))?;

    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.port)?;

    let (_result, _block_info, cost) = f1r3fly_api
        .exploratory_deploy(
            &rholang_code,
            args.block_hash.as_deref(),
            args.use_pre_state,
        )
        .await?;

    println!("{}", cost);

    Ok(())
}

pub async fn deploy_command(args: &DeployArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Read the Rholang code from file
    println!("Reading Rholang from: {}", args.file.display());
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {}", e))?;
    println!("Code size: {} bytes", rholang_code.len());

    // Initialize the F1r3fly API client
    println!("Connecting to F1r3fly node at {}:{}", args.host, args.port);
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.port)?;

    let phlo_limit = if args.bigger_phlo {
        "5,000,000,000"
    } else {
        "50,000"
    };
    println!("Using phlo limit: {}", phlo_limit);

    // Calculate expiration timestamp
    let expiration_timestamp = calculate_expiration_timestamp(args.expiration, args.expires_in);
    if expiration_timestamp > 0 {
        println!("Deploy expiration: {} ms", expiration_timestamp);
    }

    // Deploy the Rholang code
    println!("Deploying Rholang code...");
    let start_time = Instant::now();

    match f1r3fly_api
        .deploy(
            &rholang_code,
            args.bigger_phlo,
            "rholang",
            expiration_timestamp,
        )
        .await
    {
        Ok(deploy_id) => {
            let duration = start_time.elapsed();
            println!("Deployment successful!");
            println!("Time taken: {:.2?}", duration);
            println!("Deploy ID: {}", deploy_id);
        }
        Err(e) => {
            println!("Deployment failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub async fn propose_command(args: &ProposeArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the F1r3fly API client
    println!(" Connecting to F1r3fly node at {}:{}", args.host, args.port);
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.port)?;

    // Propose a block
    println!(" Proposing a new block...");
    let start_time = Instant::now();

    match f1r3fly_api.propose().await {
        Ok(ProposeResult::Proposed(block_hash)) => {
            let duration = start_time.elapsed();
            println!(" Block proposed successfully!");
            println!(" Block hash: {}", block_hash);
            println!(" Time taken: {:.2?}", duration);
        }
        Ok(ProposeResult::Skipped(reason)) => {
            let duration = start_time.elapsed();
            println!(" Proposal was skipped: {}", reason);
            println!(" Time taken: {:.2?}", duration);
        }
        Err(e) => {
            println!(" Block proposal failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub async fn full_deploy_command(args: &DeployArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Read the Rholang code from file
    println!("Reading Rholang from: {}", args.file.display());
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {}", e))?;
    println!("Code size: {} bytes", rholang_code.len());

    // Initialize the F1r3fly API client
    println!("Connecting to F1r3fly node at {}:{}", args.host, args.port);
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.port)?;

    let phlo_limit = if args.bigger_phlo {
        "5,000,000,000"
    } else {
        "50,000"
    };
    println!("Using phlo limit: {}", phlo_limit);

    // Calculate expiration timestamp
    let expiration_timestamp = calculate_expiration_timestamp(args.expiration, args.expires_in);
    if expiration_timestamp > 0 {
        println!("Deploy expiration: {} ms", expiration_timestamp);
    }

    // Deploy and propose
    println!("Deploying Rholang code and proposing a block...");
    let start_time = Instant::now();

    match f1r3fly_api
        .full_deploy(
            &rholang_code,
            args.bigger_phlo,
            "rholang",
            expiration_timestamp,
        )
        .await
    {
        Ok(ProposeResult::Proposed(block_hash)) => {
            let duration = start_time.elapsed();
            println!("Deployment and block proposal successful!");
            println!("Time taken: {:.2?}", duration);
            println!("Block hash: {}", block_hash);
        }
        Ok(ProposeResult::Skipped(reason)) => {
            let duration = start_time.elapsed();
            println!("Deployment successful, but proposal was skipped.");
            println!("Time taken: {:.2?}", duration);
            println!("Skip reason: {}", reason);
        }
        Err(e) => {
            println!("Operation failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub async fn is_finalized_command(
    args: &IsFinalizedArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the F1r3fly API client
    println!(" Connecting to F1r3fly node at {}:{}", args.host, args.port);
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.port)?;

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
            println!(" Time taken: {:.2?}", duration);
        }
        Err(e) => {
            println!(" Error checking block finalization!");
            println!("Error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub async fn bond_validator_command(
    args: &BondValidatorArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Bonding validator with stake: {}", args.stake);

    let bonding_code = format!(
        r#"new rl(`rho:registry:lookup`), poSCh, retCh, stdout(`rho:io:stdout`) in {{
 stdout!("About to lookup PoS contract...") |
 rl!(`rho:system:pos`, *poSCh) |
 for(@(_, PoS) <- poSCh) {{
 stdout!("About to bond...") |
 new deployerId(`rho:system:deployerId`) in {{
 @PoS!("bond", *deployerId, {}, *retCh) |
 for (@(result, message) <- retCh) {{
 stdout!(("Bond result:", result, "Message:", message))
 }}
 }}
 }}
}}"#,
        args.stake
    );

    let expiration = calculate_expiration_timestamp(args.expiration, args.expires_in);
    let manager = F1r3flyConnectionManager::new(config_from_bond_args(args));
    let start = Instant::now();

    let result = manager
        .deploy_and_wait(&bonding_code, true, expiration)
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

    println!("Deploy ID: {}", result.deploy_id);
    println!("Block hash: {}", result.block_hash);
    println!("Total time: {:.2?}", start.elapsed());

    if args.propose {
        let api = F1r3flyApi::new(&args.private_key, &args.host, args.port)?;
        match api.propose().await {
            Ok(ProposeResult::Proposed(hash)) => println!("Block proposed: {}", hash),
            Ok(ProposeResult::Skipped(reason)) => println!("Propose skipped: {}", reason),
            Err(e) => println!("Propose failed: {}", e),
        }
    }

    println!("Bonding complete. Verify with: node_cli bonds");
    Ok(())
}

pub async fn transfer_command(args: &TransferArgs) -> Result<(), Box<dyn std::error::Error>> {
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

    let amount_dust = args.amount * 100_000_000;
    println!(
        "Transfer: {} -> {} ({} dust)",
        from_address, args.to_address, amount_dust
    );

    let rholang_code = generate_transfer_contract(&from_address, &args.to_address, amount_dust);
    let expiration = calculate_expiration_timestamp(args.expiration, args.expires_in);

    let manager = F1r3flyConnectionManager::new(config_from_transfer_args(args));
    let start = Instant::now();

    let result = manager
        .deploy_and_wait(&rholang_code, args.bigger_phlo, expiration)
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

    if result.errored {
        let err = result
            .system_deploy_error
            .as_deref()
            .unwrap_or("unknown error");
        println!("Transfer failed: {}", err);
        return Err(format!("Transfer failed: {}", err).into());
    }

    println!("Deploy ID: {}", result.deploy_id);
    println!("Block hash: {}", result.block_hash);
    if let Some(cost) = result.cost {
        println!("Cost: {}", cost);
    }
    println!("Total time: {:.2?}", start.elapsed());

    if args.propose {
        let api = F1r3flyApi::new(&args.private_key, &args.host, args.port)?;
        match api.propose().await {
            Ok(ProposeResult::Proposed(hash)) => println!("Block proposed: {}", hash),
            Ok(ProposeResult::Skipped(reason)) => println!("Propose skipped: {}", reason),
            Err(e) => println!("Propose failed: {}", e),
        }
    }

    println!("Transfer complete.");
    Ok(())
}

pub async fn deploy_and_wait_command(
    args: &DeployAndWaitArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {}", e))?;

    let manager = F1r3flyConnectionManager::new(config_from_deploy_args(args));
    let expiration = calculate_expiration_timestamp(args.expiration, args.expires_in);

    println!("Deploying and waiting for finalization...");
    let start = Instant::now();

    let result = manager
        .deploy_and_wait(&rholang_code, args.bigger_phlo, expiration)
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

    println!("Deploy ID: {}", result.deploy_id);
    println!("Block hash: {}", result.block_hash);
    if let Some(block_num) = result.block_number {
        println!("Block number: {}", block_num);
    }
    if let Some(cost) = result.cost {
        println!("Cost: {}", cost);
    }
    if result.errored {
        println!("Errored: true");
        if let Some(ref err) = result.system_deploy_error {
            println!("Deploy error: {}", err);
        }
    }
    if result.data.is_empty() {
        println!("Data: (none)");
    } else {
        for (i, par) in result.data.iter().enumerate() {
            let simplified =
                crate::f1r3fly_api::extract_par_data(par).unwrap_or_else(|| format!("{:?}", par));
            println!("Data[{}]: {}", i, simplified);
        }
    }
    println!("Total time: {:.2?}", start.elapsed());

    if args.propose {
        let private_key = args.private_key.as_deref().unwrap_or(DEV_PRIVATE_KEY);
        let api = F1r3flyApi::new(private_key, &args.host, args.port)?;
        match api.propose().await {
            Ok(ProposeResult::Proposed(hash)) => println!("Block proposed: {}", hash),
            Ok(ProposeResult::Skipped(reason)) => println!("Propose skipped: {}", reason),
            Err(e) => println!("Propose failed: {}", e),
        }
    }

    Ok(())
}

pub async fn get_deploy_command(args: &GetDeployArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("Looking up deploy: {}", args.deploy_id);
    println!(
        "Connecting to F1r3fly node at {}:{}",
        args.host, args.http_port
    );

    let f1r3fly_api = F1r3flyApi::new(DEV_PRIVATE_KEY, &args.host, 40412)?;

    let start_time = Instant::now();

    match f1r3fly_api
        .get_deploy_detail(&args.deploy_id, args.http_port)
        .await
    {
        Ok(Some(detail)) => {
            let duration = start_time.elapsed();

            match args.format.as_str() {
                "json" => {
                    let json_output = serde_json::to_string_pretty(&detail)?;
                    println!("{}", json_output);
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
                "pretty" | _ => {
                    println!("Deploy Information");
                    println!("----------------------------------------");
                    println!("Deploy ID: {}", args.deploy_id);
                    println!("Block Hash: {}", detail.block_hash);
                    println!("Block Number: {}", detail.block_number);
                    println!("Deployer: {}", detail.deployer);
                    println!("Cost: {}", detail.cost);
                    println!("Errored: {}", detail.errored);
                    if !detail.system_deploy_error.is_empty() {
                        println!("Error: {}", detail.system_deploy_error);
                    }
                    println!("Phlo Price: {}", detail.phlo_price);
                    println!("Phlo Limit: {}", detail.phlo_limit);
                    println!("Timestamp: {}", detail.timestamp);
                    println!("Sig Algo: {}", detail.sig_algorithm);
                    if args.verbose {
                        println!("Signature: {}", detail.sig);
                        println!("VABN: {}", detail.valid_after_block_number);
                    }
                    println!("Query time: {:.2?}", duration);
                }
            }
        }
        Ok(None) => {
            println!("Deploy {} not found", args.deploy_id);
        }
        Err(e) => {
            println!("Error retrieving deploy information: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

fn validate_vault_address(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !address.starts_with("1111") {
        return Err("Invalid vault address format: must start with '1111'".into());
    }

    if address.len() < 40 {
        return Err("Invalid vault address format: too short".into());
    }

    Ok(())
}

fn generate_transfer_contract(from_address: &str, to_address: &str, amount_dust: u64) -> String {
    format!(
        r#"new 
 deployerId(`rho:system:deployerId`),
 stdout(`rho:io:stdout`),
 rl(`rho:registry:lookup`),
 systemVaultCh,
 vaultCh,
 toVaultCh,
 systemVaultKeyCh,
 resultCh
in {{
 rl!(`rho:vault:system`, *systemVaultCh) |
 for (@(_, SystemVault) <- systemVaultCh) {{
 @SystemVault!("findOrCreate", "{}", *vaultCh) |
 @SystemVault!("findOrCreate", "{}", *toVaultCh) |
 @SystemVault!("deployerAuthKey", *deployerId, *systemVaultKeyCh) |
 for (@(true, vault) <- vaultCh; key <- systemVaultKeyCh; @(true, toVault) <- toVaultCh) {{
 @vault!("transfer", "{}", {}, *key, *resultCh) |
 for (@result <- resultCh) {{
 match result {{
 (true, Nil) => {{
 stdout!(("Transfer successful:", {}, "tokens"))
 }}
 (false, reason) => {{
 stdout!(("Transfer failed:", reason))
 }}
 }}
 }}
 }} |
 for (@(false, errorMsg) <- vaultCh) {{
 stdout!(("Sender vault error:", errorMsg))
 }} |
 for (@(false, errorMsg) <- toVaultCh) {{
 stdout!(("Destination vault error:", errorMsg))
 }}
 }}
}}"#,
        from_address, // findOrCreate sender
        to_address,   // findOrCreate recipient
        to_address,   // transfer target
        amount_dust,  // transfer amount
        amount_dust   // success message amount
    )
}

/// Read data at a deploy ID from a specific block
pub async fn get_data_command(args: &GetDataArgs) -> crate::error::Result<()> {
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.port)?;

    let pars = f1r3fly_api
        .get_data_at_deploy_id(&args.deploy_id, &args.block_hash)
        .await
        .map_err(|e| crate::error::NodeCliError::General(e.to_string()))?;

    if pars.is_empty() {
        println!(
            "No data found for deploy {} at block {}",
            args.deploy_id, args.block_hash
        );
    } else {
        for (i, par) in pars.iter().enumerate() {
            let simplified =
                crate::f1r3fly_api::extract_par_data(par).unwrap_or_else(|| format!("{:?}", par));
            println!("{}", simplified);
            if i < pars.len() - 1 {
                println!("---");
            }
        }
    }

    Ok(())
}
