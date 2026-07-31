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

/// Phlo limit for vault transfer deploys.
///
/// The RevVault transfer contract (registry lookup, two `findOrCreate` calls,
/// and the transfer itself) costs more than the 50k default deploy limit, and
/// a 5B limit would require the deployer's balance to cover it upfront. 500k
/// comfortably covers the measured cost while staying affordable for small
/// vaults.
pub const TRANSFER_PHLO_LIMIT: i64 = 500_000;

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
 deployId(`rho:rchain:deployId`),
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
 }} |
 for (@result <- resultCh) {{
 deployId!(result)
 }}
}}"#
    )
}

/// Interpret the deployId-channel data of a transfer deploy.
///
/// The transfer contract forwards the vault's result tuple to the deployId
/// channel: `(true, Nil)` on success, `(false, reason)` when the vault
/// rejects the transfer (e.g. insufficient balance), and
/// `("error", context, reason)` when a vault lookup fails. No result at all
/// means the contract's joins never fired, which is also a failure — a
/// transfer must never be reported successful without positive evidence.
pub fn parse_transfer_result(data: &[f1r3fly_models::rhoapi::Par]) -> Result<(), String> {
    use f1r3fly_models::rhoapi::expr::ExprInstance;

    let par = data.first().ok_or_else(|| {
        "no transfer result on the deployId channel — the transfer contract did not complete"
            .to_string()
    })?;

    for expr in &par.exprs {
        if let Some(ExprInstance::ETupleBody(tuple)) = &expr.expr_instance {
            let first_is_true = tuple.ps.first().is_some_and(|p| {
                p.exprs
                    .iter()
                    .any(|e| e.expr_instance == Some(ExprInstance::GBool(true)))
            });
            if first_is_true {
                return Ok(());
            }
            let reasons: Vec<String> = tuple
                .ps
                .iter()
                .skip(1)
                .flat_map(|p| p.exprs.iter())
                .filter_map(|e| match &e.expr_instance {
                    Some(ExprInstance::GString(s)) => Some(s.clone()),
                    _ => None,
                })
                .collect();
            let reason = if reasons.is_empty() {
                format!("{tuple:?}")
            } else {
                reasons.join(": ")
            };
            return Err(format!("vault rejected the transfer: {reason}"));
        }
    }
    Err(format!("unexpected transfer result shape: {par:?}"))
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
