//! Vault operations for F1r3fly
//!
//! This module provides native token transfer and balance query operations.
//! The native token is used for paying phlo (gas) on F1r3fly deployments.
//!
//! # Units
//!
//! - Precision comes from node metadata.
//! - All amounts in this module are in dust unless otherwise specified

use num_bigint::BigUint;

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
    pub amount_dust: BigUint,
}

impl TransferResult {
    pub fn amount_tokens(&self, decimals: u32) -> Result<String, String> {
        dust_to_tokens(&self.amount_dust, decimals)
    }
}

/// Build Rholang code for vault transfer
///
/// # Arguments
///
/// * `from_address` - Sender's vault address (1111...)
/// * `to_address` - Recipient's vault address (1111...)
/// * `amount_dust` - Amount in raw base units
pub fn build_transfer_rholang(
    from_address: &str,
    to_address: &str,
    amount_dust: &BigUint,
) -> Result<String, String> {
    let amount_literal = amount_literal(amount_dust)?;
    Ok(format!(
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
 @vault!("transfer", "{to_address}", {amount_literal}, *key, *resultCh)
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
    ))
}

/// Fail unless a finalized transfer deploy positively reports a completed transfer:
/// the deploy must not have errored, its deployId data must be readable, and the
/// vault must have accepted the transfer.
pub fn check_transfer_result(result: &crate::f1r3fly_api::DeployResult) -> Result<(), String> {
    if let Some(failure) = result.execution_failure() {
        return Err(format!(
            "transfer deploy errored on-chain (cost: {:?}): {failure}",
            result.cost
        ));
    }
    let data = result
        .data
        .as_ref()
        .map_err(|e| format!("transfer deploy finalized, but its result could not be read: {e}"))?;
    parse_transfer_result(data)
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

pub fn validate_amount(value: &BigUint) -> Result<(), String> {
    if value.bits() > 256 {
        return Err("amount exceeds uint256".into());
    }
    Ok(())
}

pub fn amount_literal(value: &BigUint) -> Result<String, String> {
    validate_amount(value)?;
    Ok(if value.bits() <= 63 {
        value.to_string()
    } else {
        format!("{value}n")
    })
}

pub fn parse_amount(value: &str) -> Result<BigUint, String> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err("amount must be an unsigned decimal integer".into());
    }
    let amount = value.parse::<BigUint>().map_err(|e| e.to_string())?;
    validate_amount(&amount)?;
    Ok(amount)
}

/// Decode an unsigned uint256 amount, not a potentially rejected transfer attempt.
pub fn json_amount(value: &serde_json::Value) -> Option<BigUint> {
    match value {
        serde_json::Value::String(text) => parse_amount(text).ok(),
        serde_json::Value::Number(number) => number.as_u64().map(BigUint::from),
        _ => None,
    }
}

pub fn deserialize_amount<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BigUint, D::Error> {
    let value = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    json_amount(&value).ok_or_else(|| serde::de::Error::custom("invalid uint256 amount"))
}

/// Reports include rejected attempts, so their amounts are signed and not uint256-bounded.
/// Never use this decoder to validate outgoing transfers or stored balances.
#[cfg(feature = "cli")]
pub(crate) fn json_reported_amount(value: &serde_json::Value) -> Option<num_bigint::BigInt> {
    use num_bigint::BigInt;

    match value {
        serde_json::Value::String(text) => {
            let digits = text.strip_prefix('-').unwrap_or(text);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            text.parse().ok()
        }
        serde_json::Value::Number(number) => number
            .as_i64()
            .map(BigInt::from)
            .or_else(|| number.as_u64().map(BigInt::from)),
        _ => None,
    }
}

#[cfg(feature = "cli")]
pub(crate) fn deserialize_reported_amount<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<num_bigint::BigInt, D::Error> {
    let value = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    json_reported_amount(&value)
        .ok_or_else(|| serde::de::Error::custom("invalid reported integer amount"))
}

pub fn tokens_to_dust(tokens: &str, decimals: u32) -> Result<BigUint, String> {
    if decimals > 77 {
        return Err("invalid decimals".into());
    }
    let (whole, fraction) = tokens.split_once('.').unwrap_or((tokens, ""));
    if whole.is_empty()
        || tokens.ends_with('.')
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > decimals as usize
    {
        return Err("invalid token amount or excess precision".into());
    }
    parse_amount(&format!(
        "{}{}{}",
        whole,
        fraction,
        "0".repeat(decimals as usize - fraction.len())
    ))
}

pub fn dust_to_tokens(dust: &BigUint, decimals: u32) -> Result<String, String> {
    validate_amount(dust)?;
    if decimals > 77 {
        return Err("invalid decimals".into());
    }
    let text = format!(
        "{:0>width$}",
        dust.to_string(),
        width = decimals as usize + 1
    );
    if decimals == 0 {
        return Ok(text);
    }
    let (whole, fraction) = text.split_at(text.len() - decimals as usize);
    let fraction = fraction.trim_end_matches('0');
    Ok(if fraction.is_empty() {
        whole.to_owned()
    } else {
        format!("{whole}.{fraction}")
    })
}

#[cfg(all(test, feature = "cli"))]
mod reported_amount_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reported_amounts_preserve_signed_and_unsigned_integers() {
        for value in [
            json!(0),
            json!(-1),
            json!(i64::MIN),
            json!(i64::MAX),
            json!(u64::MAX),
            json!("0"),
            json!("-1"),
            json!("18446744073709551617"),
            json!("-18446744073709551617"),
        ] {
            let expected = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            assert_eq!(json_reported_amount(&value).unwrap().to_string(), expected);
        }
    }

    #[test]
    fn rejected_attempts_do_not_relax_unsigned_amount_limits() {
        let oversized = (BigUint::from(1u8) << 300usize) + BigUint::from(1u8);
        for text in [oversized.to_string(), format!("-{oversized}"), "-1".into()] {
            let value = json!(text);
            assert_eq!(json_reported_amount(&value).unwrap().to_string(), text);
            assert!(parse_amount(&text).is_err());
            assert!(json_amount(&value).is_none());
        }
        assert!(json_amount(&json!(-1)).is_none());
        assert!(amount_literal(&oversized).is_err());
    }

    #[test]
    fn reported_amounts_reject_non_integer_values() {
        for value in [
            json!(""),
            json!("-"),
            json!("+1"),
            json!("--1"),
            json!("-+1"),
            json!("1.5"),
            json!("1e18"),
            json!("1_000"),
            json!(" 1"),
            json!("1 "),
            json!("NaN"),
            json!("１"),
            json!(1.0),
            json!(1.5),
            json!(true),
            json!(null),
            json!([]),
            json!({}),
        ] {
            assert!(json_reported_amount(&value).is_none(), "{value}");
        }
    }
}

#[cfg(test)]
mod bigint_tests {
    use super::*;
    #[test]
    fn literals_preserve_legacy_ints_and_encode_wide_amounts() {
        let last_int = BigUint::from(i64::MAX as u64);
        assert_eq!(amount_literal(&last_int).unwrap(), i64::MAX.to_string());
        let first_bigint = &last_int + BigUint::from(1u8);
        assert_eq!(
            amount_literal(&first_bigint).unwrap(),
            "9223372036854775808n"
        );
        let term = build_transfer_rholang("from", "to", &first_bigint).unwrap();
        assert!(term.contains("9223372036854775808n"));
        assert!(term.contains("deployId!(result)"));
        assert!(build_transfer_rholang("from", "to", &(BigUint::from(1u8) << 256usize)).is_err());
    }

    #[test]
    fn invalid_amounts_do_not_round_or_wrap() {
        for value in ["", "-1", "+1", "1.0", "1e18", " 1"] {
            assert!(parse_amount(value).is_err(), "{value}");
        }
        for value in [".1", "1.", "1.2.3", "NaN", "0.0000000000000000001"] {
            assert!(tokens_to_dust(value, 18).is_err(), "{value}");
        }
        assert!(tokens_to_dust("1", 78).is_err());
        assert_eq!(
            tokens_to_dust("1.00000001", 8).unwrap(),
            BigUint::from(100000001u32)
        );
        assert_eq!(
            json_amount(&serde_json::json!(u64::MAX)),
            Some(u64::MAX.into())
        );
        assert!(json_amount(&serde_json::json!(-1)).is_none());
        assert!(json_amount(&serde_json::json!(1.5)).is_none());
    }

    #[test]
    fn exact_amounts_above_machine_limits() {
        for bits in [53, 63, 64, 128, 255] {
            let value = (BigUint::from(1u8) << bits) + BigUint::from(1u8);
            assert_eq!(
                tokens_to_dust(&dust_to_tokens(&value, 18).unwrap(), 18).unwrap(),
                value
            );
        }
        let maximum = (BigUint::from(1u8) << 256usize) - BigUint::from(1u8);
        assert!(tokens_to_dust(&maximum.to_string(), 0).is_ok());
        assert!(parse_amount(&(maximum + BigUint::from(1u8)).to_string()).is_err());
        assert!(tokens_to_dust("0.0000000000000000001", 18).is_err());
        assert!(tokens_to_dust("-1", 18).is_err());
        assert_eq!(
            tokens_to_dust("1.000000000000000001", 18)
                .unwrap()
                .to_string(),
            "1000000000000000001"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::f1r3fly_api::DeployResult;
    use f1r3fly_models::rhoapi::expr::ExprInstance;
    use f1r3fly_models::rhoapi::{ETuple, Expr, Par};

    fn single(expr_instance: ExprInstance) -> Par {
        Par {
            exprs: vec![Expr {
                expr_instance: Some(expr_instance),
            }],
            ..Default::default()
        }
    }

    fn vault_answer(parts: Vec<Par>) -> Vec<Par> {
        vec![single(ExprInstance::ETupleBody(ETuple {
            ps: parts,
            ..Default::default()
        }))]
    }

    fn finalized(data: Result<Vec<Par>, String>) -> DeployResult {
        DeployResult {
            deploy_id: "3045".to_string(),
            block_hash: "abcd".to_string(),
            block_number: Some(12),
            cost: Some(252435),
            errored: false,
            system_deploy_error: None,
            data,
        }
    }

    #[test]
    fn an_accepted_transfer_passes() {
        let data = vault_answer(vec![single(ExprInstance::GBool(true)), Par::default()]);
        assert_eq!(check_transfer_result(&finalized(Ok(data))), Ok(()));
    }

    #[test]
    fn a_vault_rejection_fails_with_its_reason() {
        let data = vault_answer(vec![
            single(ExprInstance::GBool(false)),
            single(ExprInstance::GString("Insufficient funds".into())),
        ]);
        let err = check_transfer_result(&finalized(Ok(data))).unwrap_err();
        assert!(
            err.contains("vault rejected the transfer: Insufficient funds"),
            "{err}"
        );
    }

    #[test]
    fn an_errored_deploy_fails() {
        let mut result = finalized(Ok(vec![]));
        result.errored = true;
        let err = check_transfer_result(&result).unwrap_err();
        assert!(err.contains("errored on-chain"), "{err}");
    }

    #[test]
    fn an_unreadable_result_is_not_reported_as_a_rejection() {
        let err = check_transfer_result(&finalized(Err("timed out".to_string()))).unwrap_err();
        assert!(err.contains("could not be read: timed out"), "{err}");
        assert!(!err.contains("rejected"), "{err}");
    }
}
