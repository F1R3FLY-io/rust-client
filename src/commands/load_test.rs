use crate::args::LoadTestArgs;
use crate::f1r3fly_api::F1r3flyApi;
use chrono::Local;
use std::time::{Duration, Instant};

/// Format dust balance as REV with both units displayed
fn format_balance(dust_str: &str) -> String {
    let dust: u64 = dust_str.trim().parse().unwrap_or(0);
    let rev = dust as f64 / 100_000_000.0;
    format!("{:.8} REV ({} dust)", rev, dust)
}

#[derive(Debug)]
pub struct TestResult {
    pub test_num: u32,
    pub deploy_id: String,
    pub block_hash: String,
    pub on_main_chain: bool,
    pub deploy_time: Duration,
    pub inclusion_time: Duration,
    pub total_time: Duration,
}

pub async fn load_test_command(args: &LoadTestArgs) -> Result<(), Box<dyn std::error::Error>> {
    use crate::utils::CryptoUtils;

    println!("╔═══════════════════════════════════════════╗");
    println!("║  F1R3FLY Load Test                        ║");
    println!("╚═══════════════════════════════════════════╝");
    println!("Tests: {}", args.num_tests);
    println!("Amount: {} REV", args.amount);
    println!("Interval: {}s", args.interval);
    println!("Check interval: {}s (fast mode)", args.check_interval);
    println!("Target: {}:{}", args.host, args.port);
    println!();

    // Derive sender address from private key
    let secret_key = CryptoUtils::decode_private_key(&args.private_key)?;
    let public_key = CryptoUtils::derive_public_key(&secret_key);
    let public_key_hex = CryptoUtils::serialize_public_key(&public_key, false);
    let sender_address = CryptoUtils::generate_rev_address(&public_key_hex)?;

    // Check initial balances
    println!("💰 Checking initial wallet balances...");
    println!();

    match get_balance_for_address(&sender_address, args).await {
        Ok(balance) => {
            println!("Sender Wallet:");
            println!("  Address: {}", sender_address);
            println!("  Balance: {}", format_balance(&balance));
        }
        Err(e) => {
            println!("⚠️  Failed to get sender balance: {}", e);
        }
    }
    println!();

    match get_balance_for_address(&args.to_address, args).await {
        Ok(balance) => {
            println!("Recipient Wallet:");
            println!("  Address: {}", args.to_address);
            println!("  Balance: {}", format_balance(&balance));
        }
        Err(e) => {
            println!("⚠️  Failed to get recipient balance: {}", e);
        }
    }
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    // Initialize API once (reuse connection)
    let api = F1r3flyApi::new(&args.private_key, &args.host, args.port);

    let mut results = Vec::new();

    for test_num in 1..=args.num_tests {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("🧪 Test {}/{}", test_num, args.num_tests);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        // Run single test with detailed logging
        let result = run_single_test(&api, args, test_num).await?;

        results.push(result);

        // Show running stats
        print_progress_stats(&results);

        // Wait before next test (unless last one)
        if test_num < args.num_tests {
            println!("⏱️  Waiting {}s before next test...\n", args.interval);
            tokio::time::sleep(Duration::from_secs(args.interval)).await;
        }
    }

    // Final visual summary
    print_final_summary(&results);

    Ok(())
}

async fn run_single_test(
    api: &F1r3flyApi<'_>,
    args: &LoadTestArgs,
    test_num: u32,
) -> Result<TestResult, Box<dyn std::error::Error>> {
    let test_start = Instant::now();

    // Step 1: Deploy
    println!("📤 [{}] Deploying transfer...", now_timestamp());
    let deploy_start = Instant::now();

    let rholang = generate_transfer_contract(args);
    let deploy_id = api.deploy(&rholang, true, "rholang").await?.to_string();

    println!(
        "✅ [{}] Deploy submitted ({}ms)",
        now_timestamp(),
        deploy_start.elapsed().as_millis()
    );
    println!("   Deploy ID: {}", deploy_id);

    // Step 2: Wait for block inclusion (FAST polling)
    println!("⏳ [{}] Waiting for block inclusion...", now_timestamp());
    let block_wait_start = Instant::now();

    let block_hash = wait_for_block_fast(
        api,
        &deploy_id,
        args.http_port,
        args.check_interval,
        args.inclusion_timeout,
    )
    .await?;

    let inclusion_time = block_wait_start.elapsed();
    println!(
        "✅ [{}] Included in block ({:.1}s)",
        now_timestamp(),
        inclusion_time.as_secs_f32()
    );
    println!("   Block hash: {}", block_hash);

    // Step 3: Wait for finalization
    println!("🔍 [{}] Waiting for block finalization...", now_timestamp());
    let finalization_start = Instant::now();

    let max_finalization_attempts = (args.finalization_timeout / args.check_interval.max(1)) as u32;
    let is_finalized = api
        .is_finalized(&block_hash, max_finalization_attempts, args.check_interval)
        .await?;

    let finalization_time = finalization_start.elapsed();

    // Step 4: Determine final status
    let on_main_chain = if is_finalized {
        println!(
            "✅ [{}] Block finalized ({:.1}s)",
            now_timestamp(),
            finalization_time.as_secs_f32()
        );
        println!("✅ SUCCESS - Block finalized and on main chain");
        true
    } else {
        // Not finalized - check if orphaned or just slow
        println!(
            "⚠️  [{}] Block not finalized after {:.1}s",
            now_timestamp(),
            finalization_time.as_secs_f32()
        );

        // Check main chain to distinguish orphaned from timeout
        let on_chain = is_on_main_chain_fast(api, &block_hash, args.chain_depth).await?;

        if on_chain {
            println!("⚠️  TIMEOUT - Block on chain but not finalized");
            true
        } else {
            println!("❌ ORPHANED - Block not on main chain");
            false
        }
    };

    // Step 5: Get wallet balance
    println!("💰 [{}] Checking wallet balance...", now_timestamp());
    match get_wallet_balance(api, args).await {
        Ok(balance) => {
            println!(
                "✅ [{}] Wallet balance: {}",
                now_timestamp(),
                format_balance(&balance)
            );
        }
        Err(e) => {
            println!(
                "⚠️  [{}] Failed to get wallet balance: {}",
                now_timestamp(),
                e
            );
        }
    }

    let total_time = test_start.elapsed();

    Ok(TestResult {
        test_num,
        deploy_id,
        block_hash,
        on_main_chain,
        deploy_time: deploy_start.elapsed(),
        inclusion_time,
        total_time,
    })
}

fn generate_transfer_contract(args: &LoadTestArgs) -> String {
    use crate::utils::CryptoUtils;

    // Derive sender address from private key
    let secret_key =
        CryptoUtils::decode_private_key(&args.private_key).expect("Invalid private key");
    let public_key = CryptoUtils::derive_public_key(&secret_key);
    let public_key_hex = CryptoUtils::serialize_public_key(&public_key, false);
    let from_address =
        CryptoUtils::generate_rev_address(&public_key_hex).expect("Failed to generate address");

    let amount_dust = args.amount * 100_000_000;

    format!(
        r#"new 
    deployerId(`rho:rchain:deployerId`),
    stdout(`rho:io:stdout`),
    rl(`rho:registry:lookup`),
    asiVaultCh,
    vaultCh,
    toVaultCh,
    asiVaultKeyCh,
    resultCh
in {{
  rl!(`rho:rchain:asiVault`, *asiVaultCh) |
  for (@(_, ASIVault) <- asiVaultCh) {{
    @ASIVault!("findOrCreate", "{}", *vaultCh) |
    @ASIVault!("findOrCreate", "{}", *toVaultCh) |
    @ASIVault!("deployerAuthKey", *deployerId, *asiVaultKeyCh) |
    for (@(true, vault) <- vaultCh; key <- asiVaultKeyCh; @(true, toVault) <- toVaultCh) {{
      @vault!("transfer", "{}", {}, *key, *resultCh) |
      for (@result <- resultCh) {{
        match result {{
          (true, Nil) => {{
            stdout!(("Transfer successful:", {}, "REV"))
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
        from_address, args.to_address, args.to_address, amount_dust, amount_dust
    )
}

// Fast block polling (configurable interval and timeout)
async fn wait_for_block_fast(
    api: &F1r3flyApi<'_>,
    deploy_id: &str,
    http_port: u16,
    check_interval: u64,
    timeout_seconds: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let max_attempts = timeout_seconds / check_interval.max(1);

    for attempt in 1..=max_attempts {
        if attempt % 10 == 0 {
            println!(
                "   ⏱️  Still waiting... ({}s elapsed)",
                attempt * check_interval
            );
        }

        match api.get_deploy_block_hash(deploy_id, http_port).await? {
            Some(hash) => return Ok(hash),
            None => {
                tokio::time::sleep(Duration::from_secs(check_interval)).await;
            }
        }
    }

    Err("Timeout waiting for block inclusion".into())
}

// Fast main chain check (immediate, no retries)
async fn is_on_main_chain_fast(
    api: &F1r3flyApi<'_>,
    block_hash: &str,
    depth: u32,
) -> Result<bool, Box<dyn std::error::Error>> {
    let blocks = api.show_main_chain(depth).await?;

    Ok(blocks.iter().any(|b| b.block_hash == block_hash))
}

// Get wallet balance for any address
async fn get_balance_for_address(
    address: &str,
    args: &LoadTestArgs,
) -> Result<String, Box<dyn std::error::Error>> {
    // Build the Rholang query to get wallet balance
    let rholang_query = format!(
        r#"new return, rl(`rho:registry:lookup`), revVaultCh, vaultCh, balanceCh in {{
            rl!(`rho:rchain:revVault`, *revVaultCh) |
            for (@(_, RevVault) <- revVaultCh) {{
                @RevVault!("findOrCreate", "{}", *vaultCh) |
                for (@either <- vaultCh) {{
                    match either {{
                        (true, vault) => {{
                            @vault!("balance", *balanceCh) |
                            for (@balance <- balanceCh) {{
                                return!(balance)
                            }}
                        }}
                        (false, errorMsg) => {{
                            return!(errorMsg)
                        }}
                    }}
                }}
            }}
        }}"#,
        address
    );

    // Create a separate API instance for read-only port
    let readonly_api = F1r3flyApi::new(&args.private_key, &args.host, args.readonly_port);

    // Execute exploratory deploy to get balance on read-only node
    let (result, _block_info) = readonly_api
        .exploratory_deploy(&rholang_query, None, false)
        .await?;

    Ok(result.trim().to_string())
}

// Get wallet balance for the sender address (convenience wrapper)
async fn get_wallet_balance(
    _api: &F1r3flyApi<'_>,
    args: &LoadTestArgs,
) -> Result<String, Box<dyn std::error::Error>> {
    use crate::utils::CryptoUtils;

    // Derive sender address from private key
    let secret_key = CryptoUtils::decode_private_key(&args.private_key)?;
    let public_key = CryptoUtils::derive_public_key(&secret_key);
    let public_key_hex = CryptoUtils::serialize_public_key(&public_key, false);
    let sender_address = CryptoUtils::generate_rev_address(&public_key_hex)?;

    get_balance_for_address(&sender_address, args).await
}

fn print_progress_stats(results: &[TestResult]) {
    let total = results.len();
    let finalized = results.iter().filter(|r| r.on_main_chain).count();
    let orphaned = total - finalized;

    println!();
    println!("📊 Current Stats:");
    println!(
        "   ✅ Finalized: {} ({}%)",
        finalized,
        finalized * 100 / total
    );
    println!(
        "   ❌ Orphaned/Timeout: {} ({}%)",
        orphaned,
        orphaned * 100 / total
    );
    println!();
}

fn print_final_summary(results: &[TestResult]) {
    println!();
    println!("╔═══════════════════════════════════════════╗");
    println!("║  FINAL RESULTS                            ║");
    println!("╚═══════════════════════════════════════════╝");

    let total = results.len();
    let finalized = results.iter().filter(|r| r.on_main_chain).count();
    let failed = total - finalized;

    println!("Total tests: {}", total);
    println!("✅ Finalized: {} ({}%)", finalized, finalized * 100 / total);
    println!(
        "❌ Orphaned/Timeout: {} ({}%)",
        failed,
        failed * 100 / total
    );
    println!();

    // Visual bar chart
    println!("Finalization rate:");
    print_bar_chart(finalized as f32 / total as f32);

    println!();
    println!("Failure rate:");
    print_bar_chart(failed as f32 / total as f32);

    // Timing stats
    if !results.is_empty() {
        let avg_inclusion = results
            .iter()
            .map(|r| r.inclusion_time.as_secs_f32())
            .sum::<f32>()
            / total as f32;

        let avg_total = results
            .iter()
            .map(|r| r.total_time.as_secs_f32())
            .sum::<f32>()
            / total as f32;

        println!();
        println!("⏱️  Timing Statistics:");
        println!("   Average inclusion time: {:.1}s", avg_inclusion);
        println!("   Average total time: {:.1}s", avg_total);
    }

    println!();

    // Exit code hint
    if failed > 0 {
        println!("⚠️  {} blocks failed to finalize or were orphaned", failed);
    } else {
        println!("✅ All blocks successfully finalized");
    }
}

fn print_bar_chart(percentage: f32) {
    let bar_length = 40;
    let filled = (percentage * bar_length as f32) as usize;
    let empty = bar_length - filled;

    print!("[");
    print!("{}", "█".repeat(filled));
    print!("{}", "░".repeat(empty));
    println!("] {:.1}%", percentage * 100.0);
}

fn now_timestamp() -> String {
    Local::now().format("%H:%M:%S").to_string()
}
