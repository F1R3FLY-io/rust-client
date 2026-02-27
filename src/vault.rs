//! Vault operations for F1r3fly
//!
//! This module provides native token transfer and balance query operations.
//! The native token is used for paying phlo (gas) on F1r3fly deployments.
//!
//! # Units
//!
//! - 1 token = 100,000,000 dust
//! - All amounts in this module are in dust unless otherwise specified

/// Token to dust conversion factor (1 token = 100,000,000 dust)
pub const DUST_FACTOR: u64 = 100_000_000;

/// Result of a vault transfer operation
#[derive(Debug, Clone)]
pub struct TransferResult {
    /// Deploy ID of the transfer transaction
    pub deploy_id: String,
    /// Block hash containing the transfer
    pub block_hash: String,
    /// Sender's vault address
    pub from_address: String,
    /// Recipient's vault address
    pub to_address: String,
    /// Amount transferred in dust
    pub amount_dust: u64,
}

impl TransferResult {
    /// Get amount in tokens (1 token = 100,000,000 dust)
    pub fn amount_tokens(&self) -> f64 {
        self.amount_dust as f64 / DUST_FACTOR as f64
    }
}

/// Build Rholang code for vault transfer
///
/// # Arguments
///
/// * `from_address` - Sender's vault address (1111...)
/// * `to_address` - Recipient's vault address (1111...)
/// * `amount_dust` - Amount in dust (1 token = 100,000,000 dust)
pub fn build_transfer_rholang(from_address: &str, to_address: &str, amount_dust: u64) -> String {
    format!(
        r#"new
    deployerId(`rho:system:deployerId`),
    rl(`rho:registry:lookup`),
    systemVaultCh,
    vaultCh,
    toVaultCh,
    systemVaultKeyCh,
    resultCh
in {{
  rl!(`rho:vault:system`, *systemVaultCh) |
  for (@(_, SystemVault) <- systemVaultCh) {{
    @SystemVault!("findOrCreate", "{from_address}", *vaultCh) |
    @SystemVault!("findOrCreate", "{to_address}", *toVaultCh) |
    @SystemVault!("deployerAuthKey", *deployerId, *systemVaultKeyCh) |
    for (@(true, vault) <- vaultCh; key <- systemVaultKeyCh; @(true, toVault) <- toVaultCh) {{
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

/// Build Rholang code to query vault balance
///
/// # Arguments
///
/// * `address` - Vault address to query (1111...)
pub fn build_balance_query(address: &str) -> String {
    format!(
        r#"new return, rl(`rho:registry:lookup`), systemVaultCh, vaultCh, balanceCh in {{
    rl!(`rho:vault:system`, *systemVaultCh) |
    for (@(_, SystemVault) <- systemVaultCh) {{
        @SystemVault!("findOrCreate", "{address}", *vaultCh) |
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

/// Validate vault address format
///
/// Vault addresses start with "1111" and are base58-encoded.
pub fn validate_address(address: &str) -> Result<(), String> {
    if !address.starts_with("1111") {
        return Err("Invalid vault address format: must start with '1111'".to_string());
    }

    if address.len() < 40 {
        return Err("Invalid vault address format: too short".to_string());
    }

    Ok(())
}

/// Convert token amount to dust
pub fn tokens_to_dust(tokens: f64) -> u64 {
    (tokens * DUST_FACTOR as f64) as u64
}

/// Convert dust amount to tokens
pub fn dust_to_tokens(dust: u64) -> f64 {
    dust as f64 / DUST_FACTOR as f64
}
