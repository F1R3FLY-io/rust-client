//! REV Vault operations for F1r3fly
//!
//! This module provides REV (native F1r3fly currency) transfer and balance query
//! operations. REV is used for paying phlo (gas) on F1r3fly deployments.
//!
//! # Units
//!
//! - 1 REV = 100,000,000 dust
//! - All amounts in this module are in dust unless otherwise specified
//!
//! # Example
//!
//! ```ignore
//! use node_cli::connection_manager::F1r3flyConnectionManager;
//!
//! let manager = F1r3flyConnectionManager::from_env()?;
//!
//! // Transfer 1 REV (100M dust) to another address
//! let result = manager.transfer_rev("1111abc...", 100_000_000).await?;
//! println!("Transfer complete: {}", result.deploy_id);
//! ```

/// REV to dust conversion factor (1 REV = 100,000,000 dust)
pub const REV_TO_DUST: u64 = 100_000_000;

/// Result of a REV transfer operation
#[derive(Debug, Clone)]
pub struct RevTransferResult {
    /// Deploy ID of the transfer transaction
    pub deploy_id: String,
    /// Block hash containing the transfer
    pub block_hash: String,
    /// Sender's REV address
    pub from_address: String,
    /// Recipient's REV address
    pub to_address: String,
    /// Amount transferred in dust
    pub amount_dust: u64,
}

impl RevTransferResult {
    /// Get amount in REV (1 REV = 100,000,000 dust)
    pub fn amount_rev(&self) -> f64 {
        self.amount_dust as f64 / REV_TO_DUST as f64
    }
}

/// Build Rholang code for REV vault transfer
///
/// # Arguments
///
/// * `from_address` - Sender's REV address (1111...)
/// * `to_address` - Recipient's REV address (1111...)
/// * `amount_dust` - Amount in dust (1 REV = 100,000,000 dust)
///
/// # Returns
///
/// Rholang code that transfers REV between vaults
pub fn build_rev_transfer_rholang(from_address: &str, to_address: &str, amount_dust: u64) -> String {
    format!(
        r#"new 
    deployerId(`rho:rchain:deployerId`),
    rl(`rho:registry:lookup`),
    revVaultCh,
    vaultCh,
    toVaultCh,
    revVaultKeyCh,
    resultCh
in {{
  rl!(`rho:rchain:revVault`, *revVaultCh) |
  for (@(_, RevVault) <- revVaultCh) {{
    @RevVault!("findOrCreate", "{from_address}", *vaultCh) |
    @RevVault!("findOrCreate", "{to_address}", *toVaultCh) |
    @RevVault!("deployerAuthKey", *deployerId, *revVaultKeyCh) |
    for (@(true, vault) <- vaultCh; key <- revVaultKeyCh; @(true, toVault) <- toVaultCh) {{
      @vault!("transfer", "{to_address}", {amount_dust}, *key, *resultCh)
    }} |
    for (@(false, errorMsg) <- vaultCh) {{
      resultCh!(("error", "Sender vault error", errorMsg))
    }} |
    for (@(false, errorMsg) <- toVaultCh) {{
      resultCh!(("error", "Recipient vault error", errorMsg))
    }}
  }}
}}"#
    )
}

/// Build Rholang code to query REV balance
///
/// # Arguments
///
/// * `address` - REV address to query (1111...)
///
/// # Returns
///
/// Rholang code that returns the balance via return!()
pub fn build_rev_balance_query(address: &str) -> String {
    format!(
        r#"new return, rl(`rho:registry:lookup`), revVaultCh, vaultCh, balanceCh in {{
    rl!(`rho:rchain:revVault`, *revVaultCh) |
    for (@(_, RevVault) <- revVaultCh) {{
        @RevVault!("findOrCreate", "{address}", *vaultCh) |
        for (@either <- vaultCh) {{
            match either {{
                (true, vault) => {{
                    @vault!("balance", *balanceCh) |
                    for (@balance <- balanceCh) {{ return!(balance) }}
                }}
                (false, _) => return!(-1)
            }}
        }}
    }}
}}"#
    )
}

/// Validate REV address format
///
/// REV addresses start with "1111" and are base58-encoded.
///
/// # Arguments
///
/// * `address` - The address to validate
///
/// # Returns
///
/// Ok(()) if valid, Err with message if invalid
pub fn validate_rev_address(address: &str) -> Result<(), String> {
    if !address.starts_with("1111") {
        return Err("Invalid REV address format: must start with '1111'".to_string());
    }

    if address.len() < 40 {
        return Err("Invalid REV address format: too short".to_string());
    }

    Ok(())
}

/// Convert REV amount to dust
pub fn rev_to_dust(rev: f64) -> u64 {
    (rev * REV_TO_DUST as f64) as u64
}

/// Convert dust amount to REV
pub fn dust_to_rev(dust: u64) -> f64 {
    dust as f64 / REV_TO_DUST as f64
}

