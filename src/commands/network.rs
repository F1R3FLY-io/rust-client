use crate::args::*;
use crate::f1r3fly_api::{DeployInfo, DeployStatus, F1r3flyApi};
use crate::utils::output::{CompressedDeployStatus, DeployCompressedInfo, FinalizeStatus};
use crate::utils::rho_helpers::change_contract_token_name;
use std::fs;
use std::time::Instant;

pub async fn exploratory_deploy_command(
    args: &ExploratoryDeployArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read the Rholang code from file
    println!("📄 Reading Rholang from: {}", args.file.display());
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {}", e))?;
    println!("📊 Code size: {} bytes", rholang_code.len());

    // Initialize the F1r3fly API client
    println!(
        "🔌 Connecting to F1r3fly node at {}:{}",
        args.host, args.grpc_port
    );
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.grpc_port);

    // Execute the exploratory deployment
    println!("🚀 Executing Rholang code (exploratory deploy)...");

    // Display block hash if provided
    if let Some(block_hash) = &args.block_hash {
        println!("🧱 Using block hash: {}", block_hash);
    }

    // Display state hash preference
    if args.use_pre_state {
        println!("🔍 Using pre-state hash");
    } else {
        println!("🔍 Using post-state hash");
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
        Ok((result, block_info)) => {
            let duration = start_time.elapsed();
            println!("✅ Execution successful!");
            println!("⏱️  Time taken: {:.2?}", duration);
            println!("🧱 {}", block_info);
            println!("📊 Result:");
            println!("{}", result);
        }
        Err(e) => {
            println!("❌ Execution failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub async fn deploy_command(args: &DeployArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Read the Rholang code from file
    println!("📄 Reading Rholang from: {}", args.file.display());
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {}", e))?;
    println!("📊 Code size: {} bytes", rholang_code.len());

    // Initialize the F1r3fly API client
    println!(
        "🔌 Connecting to F1r3fly node at {}:{}",
        args.host, args.grpc_port
    );
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.grpc_port);

    let phlo_limit = if args.bigger_phlo {
        "5,000,000,000"
    } else {
        "50,000"
    };
    println!("💰 Using phlo limit: {}", phlo_limit);

    // Deploy the Rholang code
    println!("🚀 Deploying Rholang code...");
    let start_time = Instant::now();

    match f1r3fly_api
        .deploy(&rholang_code, args.bigger_phlo, "rholang")
        .await
    {
        Ok(deploy_id) => {
            let duration = start_time.elapsed();
            println!("✅ Deployment successful!");
            println!("⏱️  Time taken: {:.2?}", duration);
            println!("🆔 Deploy ID: {}", deploy_id);
        }
        Err(e) => {
            println!("❌ Deployment failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub async fn propose_command(args: &ProposeArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the F1r3fly API client
    println!(
        "🔌 Connecting to F1r3fly node at {}:{}",
        args.host, args.grpc_port
    );
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.grpc_port);

    // Propose a block
    println!("📦 Proposing a new block...");
    let start_time = Instant::now();

    match f1r3fly_api.propose().await {
        Ok(block_hash) => {
            let duration = start_time.elapsed();
            println!("✅ Block proposed successfully!");
            println!("⏱️  Time taken: {:.2?}", duration);
            println!("🧱 Block hash: {}", block_hash);
        }
        Err(e) => {
            println!("❌ Block proposal failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub async fn full_deploy_command(args: &DeployArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Read the Rholang code from file
    println!("📄 Reading Rholang from: {}", args.file.display());
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {}", e))?;
    println!("📊 Code size: {} bytes", rholang_code.len());

    // Initialize the F1r3fly API client
    println!(
        "🔌 Connecting to F1r3fly node at {}:{}",
        args.host, args.grpc_port
    );
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.grpc_port);

    let phlo_limit = if args.bigger_phlo {
        "5,000,000,000"
    } else {
        "50,000"
    };
    println!("💰 Using phlo limit: {}", phlo_limit);

    // Deploy and propose
    println!("🚀 Deploying Rholang code and proposing a block...");
    let start_time = Instant::now();

    match f1r3fly_api
        .full_deploy(&rholang_code, args.bigger_phlo, "rholang")
        .await
    {
        Ok(block_hash) => {
            let duration = start_time.elapsed();
            println!("✅ Deployment and block proposal successful!");
            println!("⏱️  Time taken: {:.2?}", duration);
            println!("🧱 Block hash: {}", block_hash);
        }
        Err(e) => {
            println!("❌ Operation failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    }

    Ok(())
}

pub async fn is_finalized_command(
    args: &IsFinalizedArgs,
) -> Result<FinalizeStatus, Box<dyn std::error::Error>> {
    // Initialize the F1r3fly API client
    println!(
        "🔌 Connecting to F1r3fly node at {}:{}",
        args.host, args.grpc_port
    );
    let f1r3fly_api = F1r3flyApi::new_readonly(&args.host, args.grpc_port);

    // Check if the block is finalized
    println!("🔍 Checking if block is finalized: {}", args.block_hash);
    println!(
        "⏱️  Will retry every {} seconds, up to {} times",
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
                println!("✅ Block is finalized!");
                println!("⏱️  Time taken: {:.2?}", duration);

                return Ok(FinalizeStatus::Finalized);
            } else {
                println!(
                    "❌ Block is not finalized after {} attempts",
                    args.max_attempts
                );
                println!("⏱️  Time taken: {:.2?}", duration);

                return Ok(FinalizeStatus::Finalizing);
            }
        }
        Err(e) => {
            println!("❌ Error checking block finalization!");
            println!("Error: {}", e);

            return Ok(FinalizeStatus::FinalizationError(e.to_string()));
        }
    }
}

pub async fn transfer_deploy(args: &TransferArgs) -> Result<String, Box<dyn std::error::Error>> {
    let token = &args.token.to_uppercase();
    println!("💸 Initiating {} transfer", token);

    println!(
        "🔌 Connecting to F1r3fly node at {}:{}",
        args.host, args.grpc_port
    );
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.grpc_port);

    println!("🔍 Deriving sender address from private key...");
    let from_address = {
        use crate::utils::CryptoUtils;
        let secret_key = CryptoUtils::decode_private_key(&args.private_key)?;
        let public_key = CryptoUtils::derive_public_key(&secret_key);
        let public_key_hex = CryptoUtils::serialize_public_key(&public_key, false);
        CryptoUtils::generate_address(&public_key_hex)?
    };

    validate_address(&from_address)?;
    validate_address(&args.to_address)?;

    let amount_dust = args.amount * 100_000_000;

    println!("📋 Transfer Details:");
    println!("   From: {}", from_address);
    println!("   To: {}", args.to_address);
    println!(
        "   Amount: {} {} ({} dust)",
        args.amount, token, amount_dust
    );
    println!(
        "   Phlo limit: {}",
        if args.bigger_phlo {
            "High (recommended for transfers)"
        } else {
            "Standard"
        }
    );

    let mut rholang_code =
        generate_transfer_contract(&from_address, &args.to_address, amount_dust)?;
    if token != "ASI" {
        rholang_code = change_contract_token_name(&rholang_code, &token);
    }

    println!("🚀 Deploying transfer contract...");

    match f1r3fly_api
        .deploy(&rholang_code, args.bigger_phlo, "rholang")
        .await
    {
        Ok(deploy_id) => {
            println!("✅ Transfer contract deployed successfully!");
            println!("🆔 Deploy ID: {}", deploy_id);

            return Ok(deploy_id);
        }
        Err(e) => {
            println!("❌ Transfer deployment failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    };
}

// Whether deploy is in block & check for finalization
pub async fn check_deploy_status(
    deploy_id: String,
    args: &WaitArgs,
) -> Result<DeployCompressedInfo, Box<dyn std::error::Error>> {
    let f1r3fly_api =
        F1r3flyApi::new_readonly(&args.http_node_args.host, args.http_node_args.http_port - 1);

    let block_wait_start = Instant::now();
    let max_block_wait_attempts = args.max_attempts;
    let mut block_wait_attempts = 0;

    println!("⏳ Waiting for deploy to be included in a block");

    let block_hash = loop {
        block_wait_attempts += 1;

        // Show progress every 10 attempts or if we're at the end
        if block_wait_attempts % 10 == 0 || block_wait_attempts >= max_block_wait_attempts {
            println!(
                "   ⏱️  Checking... ({}/{} attempts)",
                block_wait_attempts, max_block_wait_attempts
            );
        }

        match f1r3fly_api
            .get_deploy_block_hash(&deploy_id, args.http_node_args.http_port)
            .await
        {
            Ok(Some(hash)) => {
                println!("✅ Deploy found in block: {}", hash);
                break hash;
            }
            Ok(None) => {
                if block_wait_attempts >= max_block_wait_attempts {
                    println!(
                        "❌ Timeout waiting for deploy to be included in block after {} seconds",
                        max_block_wait_attempts * args.check_interval as u32
                    );
                    return Ok(DeployCompressedInfo::ok(
                        CompressedDeployStatus::Deploying,
                        None,
                    ));
                }
                // Likely deploy not in block yet, continue waiting
            }
            Err(e) => {
                println!("❌ Error checking deploy status: {}", e);
                return Ok(DeployCompressedInfo::error(
                    CompressedDeployStatus::DeployError,
                    e.to_string().as_str(),
                    None,
                ));
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(args.check_interval)).await;
    };

    let block_wait_duration = block_wait_start.elapsed();
    println!("⏱️  Block inclusion time: {:.2?}", block_wait_duration);

    println!("🔍 Wait for block finalization using observer node");

    // check with observer api
    let finalized_args = IsFinalizedArgs::from_wait_args(block_hash.clone(), args);
    let finalize_status = match is_finalized_command(&finalized_args).await {
        Ok(status) => status,
        Err(e) => {
            return Ok(DeployCompressedInfo::error(
                CompressedDeployStatus::FinalizationError,
                e.to_string().as_str(),
                Some(block_hash),
            ));
        }
    };

    return Ok(DeployCompressedInfo::from_finalize(
        finalize_status,
        Some(block_hash),
    ));
}

pub async fn transfer_command(args: &TransferArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("STEP 1: Transfer deploy");
    let deploy_start_time = Instant::now();
    let deploy_id = transfer_deploy(args).await?;
    let deploy_duration = deploy_start_time.elapsed();
    println!("⏱️  Deploy time: {:.2?}", deploy_duration);

    // Handle propose logic if enabled
    if args.propose {
        println!("STEP 1.2: Transfer propose block");
        let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.grpc_port);
        let propose_start = Instant::now();

        match f1r3fly_api.propose().await {
            Ok(block_hash) => {
                let propose_duration = propose_start.elapsed();
                println!("✅ Block proposed successfully!");
                println!("⏱️  Propose time: {:.2?}", propose_duration);
                println!("🧱 Block hash: {}", block_hash);
            }
            Err(e) => {
                println!("❌ Block proposal failed!");
                println!("Error: {}", e);
                return Err(e);
            }
        }
    }

    println!("STEP 2: Wait for transfer deploy to be finalized");
    let wait_args: WaitArgs = WaitArgs::from_transfer_args(args);
    let deploy_info: DeployCompressedInfo = check_deploy_status(deploy_id, &wait_args).await?;

    if *deploy_info.status() == CompressedDeployStatus::Finalized {
        let total_duration = deploy_start_time.elapsed();
        println!("🎉 Total transfer time: {:.2?}", total_duration);
        println!("🎯 Transfer process completed!");
    } else {
        println!(
            "⚠️  Transfer deploy status {:?} after {} attempts",
            deploy_info.status(),
            wait_args.max_attempts
        );
    }

    Ok(())
}

pub async fn bond_validator_command(
    args: &BondValidatorArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔗 Bonding new validator to the network");
    println!("💰 Stake amount: {}", args.stake);

    // Initialize the F1r3fly API client for deploying
    let f1r3fly_api = F1r3flyApi::new(&args.private_key, &args.host, args.grpc_port);

    let bond_template = include_str!("../../rho_examples/cli/bond.rho");

    // Create the bonding Rholang code
    let bonding_code = bond_template.replacen("{}", &args.stake.to_string(), 1);

    println!("🚀 Deploying bonding transaction...");
    let deploy_start_time = Instant::now();

    // STEP 1: Deploy the bonding code
    let deploy_id = match f1r3fly_api.deploy(&bonding_code, true, "rholang").await {
        Ok(deploy_id) => {
            let deploy_duration = deploy_start_time.elapsed();
            println!("✅ Bonding deploy successful! Deploy ID: {}", deploy_id);
            println!("⏱️  Deploy time: {:.2?}", deploy_duration);
            deploy_id
        }
        Err(e) => {
            println!("❌ Bonding deploy failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    };

    // STEP 2: Wait for deploy to be included in a block

    // Handle propose logic if enabled
    if args.propose {
        println!("📦 Proposing block to help finalize the bonding transaction...");
        let propose_start = Instant::now();

        match f1r3fly_api.propose().await {
            Ok(block_hash) => {
                let propose_duration = propose_start.elapsed();
                println!("✅ Block proposed successfully!");
                println!("⏱️  Propose time: {:.2?}", propose_duration);
                println!("🧱 Block hash: {}", block_hash);
            }
            Err(e) => {
                println!("❌ Block proposal failed!");
                println!("Error: {}", e);
                return Err(e);
            }
        }
    }

    let wait_args = WaitArgs::from_bond_validator_args(args);
    let deploy_info = check_deploy_status(deploy_id, &wait_args).await?;

    if *deploy_info.status() == CompressedDeployStatus::Finalized {
        let total_duration = deploy_start_time.elapsed();
        println!("🎉 Total bonding time: {:.2?}", total_duration);
        println!("🎯 Bonding process completed!");
    } else {
        println!(
            "⚠️  Bonding deploy status {:?} after {} attempts",
            deploy_info.status(),
            wait_args.max_attempts
        );
    }

    println!("🎯 Validator bonding process completed!");
    println!("📋 Next steps:");
    println!("   1. Verify the validator appears in the bonds list");
    println!("   2. Check that the validator is participating in consensus");
    println!("   3. Monitor for block proposals from the new validator");

    Ok(())
}

pub async fn deploy_and_wait_command(
    args: &DeployAndWaitArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read the Rholang code from file
    println!("📄 Reading Rholang from: {}", args.file);
    let rholang_code =
        fs::read_to_string(&args.file).map_err(|e| format!("Failed to read file: {}", e))?;
    println!("📊 Code size: {} bytes", rholang_code.len());

    // Initialize the F1r3fly API client
    println!(
        "🔌 Connecting to F1r3fly node at {}:{}",
        args.host, args.grpc_port
    );
    let private_key = args.private_key.as_deref().unwrap();
    let f1r3fly_api = F1r3flyApi::new(private_key, &args.host, args.grpc_port);

    let phlo_limit = if args.bigger_phlo {
        "5,000,000,000"
    } else {
        "50,000"
    };
    println!("💰 Using phlo limit: {}", phlo_limit);

    // STEP 1: Deploy the Rholang code
    println!("🚀 Deploying Rholang code...");
    let deploy_start_time = Instant::now();

    let deploy_id = match f1r3fly_api
        .deploy(&rholang_code, args.bigger_phlo, "rholang")
        .await
    {
        Ok(deploy_id) => {
            let deploy_duration = deploy_start_time.elapsed();
            println!("✅ Deploy successful! Deploy ID: {}", deploy_id);
            println!("⏱️  Deploy time: {:.2?}", deploy_duration);
            deploy_id
        }
        Err(e) => {
            println!("❌ Deployment failed!");
            println!("Error: {}", e);
            return Err(e);
        }
    };

    // STEP 2: Wait for deploy to be included in a block
    let wait_args = WaitArgs::from_deploy_args(args);
    let deploy_info = check_deploy_status(deploy_id, &wait_args).await?;

    if *deploy_info.status() == CompressedDeployStatus::Finalized {
        let total_duration = deploy_start_time.elapsed();
        println!("🎉 Total deploy time: {:.2?}", total_duration);
        println!("🎯 Deploy process completed!");
    } else {
        println!(
            "⚠️  Deploy status {:?} after {} attempts",
            deploy_info.status(),
            wait_args.max_attempts
        );
    }

    Ok(())
}

pub async fn get_deploy_command(
    args: &GetDeployArgs,
) -> Result<DeployInfo, Box<dyn std::error::Error>> {
    println!("🔍 Looking up deploy: {}", args.deploy_id);
    println!(
        "🔌 Connecting to F1r3fly node at {}:{}",
        args.host, args.http_port
    );

    // Initialize the F1r3fly API client (private key not needed for read operations)
    let f1r3fly_api = F1r3flyApi::new_readonly(&args.host, args.http_port);

    let start_time = Instant::now();

    match f1r3fly_api
        .get_deploy_info(&args.deploy_id, args.http_port)
        .await
    {
        Ok(deploy_info) => {
            let duration = start_time.elapsed();

            match args.format.as_str() {
                "none" => {}
                "json" => {
                    let json_output = serde_json::to_string_pretty(&deploy_info)?;
                    println!("{}", json_output);
                }
                "summary" => match deploy_info.status {
                    DeployStatus::Included => {
                        if let Some(block_hash) = &deploy_info.block_hash {
                            println!(
                                "✅ Deploy {} included in block {}",
                                deploy_info.deploy_id, block_hash
                            );
                        } else {
                            println!("✅ Deploy {} included in block", deploy_info.deploy_id);
                        }
                    }
                    DeployStatus::Deploying => {
                        println!(
                            "⏳ Deploy {} pending (not yet in block)",
                            deploy_info.deploy_id
                        );
                    }
                    DeployStatus::DeployError(ref err) => {
                        println!("❌ Deploy {} error: {}", deploy_info.deploy_id, err);
                    }
                },
                "pretty" | _ => {
                    println!("📋 Deploy Information");
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    println!("🆔 Deploy ID: {}", deploy_info.deploy_id);

                    match deploy_info.status {
                        DeployStatus::Included => {
                            println!("✅ Status: Included in block");
                            if let Some(block_hash) = &deploy_info.block_hash {
                                println!("🧱 Block Hash: {}", block_hash);
                            }
                        }
                        DeployStatus::Deploying => {
                            println!("⏳ Status: Deploying (not yet in block)");
                        }
                        DeployStatus::DeployError(ref err) => {
                            println!("❌ Status: Error - {}", err);
                            println!("⏱️  Query time: {:.2?}", duration);
                        }
                    }

                    if args.verbose || deploy_info.status == DeployStatus::Included {
                        if let Some(sender) = &deploy_info.sender {
                            println!("👤 Sender: {}", sender);
                        }
                        if let Some(seq_num) = deploy_info.seq_num {
                            println!("🔢 Sequence Number: {}", seq_num);
                        }
                        if let Some(timestamp) = deploy_info.timestamp {
                            println!("🕐 Timestamp: {}", timestamp);
                        }
                        if let Some(shard_id) = &deploy_info.shard_id {
                            println!("🌐 Shard ID: {}", shard_id);
                        }
                        if let Some(sig_algorithm) = &deploy_info.sig_algorithm {
                            println!("🔐 Signature Algorithm: {}", sig_algorithm);
                        }
                        if args.verbose {
                            if let Some(sig) = &deploy_info.sig {
                                println!("✍️  Signature: {}", sig);
                            }
                        }
                    }

                    println!("⏱️  Query time: {:.2?}", duration);
                }
            }
            Ok(deploy_info)
        }
        Err(e) => {
            println!("❌ Error retrieving deploy information: {}", e);
            return Err(e);
        }
    }
}

pub fn validate_address(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !address.starts_with("1111") {
        return Err("Invalid address format: must start with '1111'".into());
    }

    if address.len() < 40 {
        return Err("Invalid address format: too short".into());
    }

    Ok(())
}

fn generate_transfer_contract(
    from_address: &str,
    to_address: &str,
    amount_dust: u64,
) -> Result<String, String> {
    let transfer_template = include_str!("../../rho_examples/cli/transfer.rho");

    let transfer_code = transfer_template
        .replacen("{}", from_address, 1)
        .replacen("{}", to_address, 1)
        .replacen("{}", to_address, 1)
        .replacen("{}", &amount_dust.to_string(), 1)
        .replacen("{}", &amount_dust.to_string(), 1);

    // println!("Generated transfer Rholang code:");
    // println!("{}", transfer_code);

    Ok(transfer_code)
}
