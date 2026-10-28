//! Proof-of-Stake contract state
//!
//! Reads the PoS contract's validator bookkeeping: bonded stake, the active
//! set, and validators on their way out.

use std::collections::{BTreeMap, BTreeSet};

use f1r3fly_models::rhoapi::expr::ExprInstance;
use f1r3fly_models::rhoapi::Par;
use num_bigint::BigUint;
use serde_json::Value;

use crate::error::{Contract, Error, Result};
use crate::rholang_helpers::convert_rholang_to_json;

/// Phlo limit of a bond or withdrawal deploy.
pub const POS_CALL_PHLO_LIMIT: u64 = 5_000_000_000;

fn malformed(detail: impl std::fmt::Display) -> Error {
    Error::decode("PoS snapshot", detail)
}

/// Exploratory term returning
/// `(allBonds, activeValidators, pendingWithdrawers, withdrawers, quarantineLength)`.
///
/// The getters run in one term so they describe the same block.
pub const POS_SNAPSHOT_QUERY: &str = r#"new return, rl(`rho:registry:lookup`), poSCh in {
 rl!(`rho:system:pos`, *poSCh) |
 for (@(_, PoS) <- poSCh) {
 new bondsCh, activeCh, pendingCh, withdrawersCh, quarantineCh in {
 @PoS!("getBonds", *bondsCh) |
 @PoS!("getActiveValidators", *activeCh) |
 @PoS!("getPendingWithdrawer", *pendingCh) |
 @PoS!("getWithdrawers", *withdrawersCh) |
 @PoS!("getQuarantineLength", *quarantineCh) |
 for (@bonds <- bondsCh & @active <- activeCh & @pending <- pendingCh & @withdrawers <- withdrawersCh & @quarantine <- quarantineCh) {
 return!((bonds, active, pending, withdrawers, quarantine))
 }
 }
 }
}"#;

/// A validator removed from the bond set whose stake is still quarantined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withdrawal {
    pub stake: BigUint,
    /// Stake plus accumulated rewards are paid out at the first epoch boundary
    /// at or after this block.
    pub quarantine_end: i64,
}

/// Validator bookkeeping of the PoS contract at one block.
///
/// Public keys are lowercase hex.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PosSnapshot {
    /// The block whose post-state was read.
    pub block_number: i64,
    /// Bonded validators and their stake. A bond enters as soon as its deploy executes.
    pub bonds: BTreeMap<String, BigUint>,
    /// Validators in consensus. Recomputed at each epoch boundary from `bonds`,
    /// capped at the shard's number of active validators.
    pub active: BTreeSet<String>,
    /// Requested withdrawals, keyed to their quarantine end. Applied at the next epoch boundary.
    pub pending_withdrawals: BTreeMap<String, i64>,
    /// Applied withdrawals awaiting payout. These keys are no longer in `bonds`.
    pub withdrawals: BTreeMap<String, Withdrawal>,
    /// Blocks between a withdrawal taking effect and its payout becoming due.
    pub quarantine_length: i64,
}

impl PosSnapshot {
    pub fn stake(&self, public_key: &str) -> Option<BigUint> {
        self.bonds.get(&public_key.to_ascii_lowercase()).cloned()
    }

    pub fn is_active(&self, public_key: &str) -> bool {
        self.active.contains(&public_key.to_ascii_lowercase())
    }

    /// Quarantine end of a requested withdrawal that has not been applied yet.
    pub fn pending_withdrawal(&self, public_key: &str) -> Option<i64> {
        self.pending_withdrawals
            .get(&public_key.to_ascii_lowercase())
            .copied()
    }

    pub fn withdrawal(&self, public_key: &str) -> Option<Withdrawal> {
        self.withdrawals
            .get(&public_key.to_ascii_lowercase())
            .cloned()
    }

    /// Bonded validators outside the active set.
    ///
    /// Not a queue: the active set is capped at the shard's number of active
    /// validators, so a bond listed here may never activate.
    pub fn inactive_bonds(&self) -> impl Iterator<Item = (&str, &BigUint)> {
        self.bonds
            .iter()
            .filter(|(key, _)| !self.active.contains(*key))
            .map(|(key, stake)| (key.as_str(), stake))
    }
}

/// Parse an explore-deploy response to [`POS_SNAPSHOT_QUERY`].
pub fn parse_pos_snapshot(response: &Value) -> Result<PosSnapshot> {
    let expr = response
        .get("expr")
        .and_then(Value::as_array)
        .and_then(|exprs| exprs.first())
        .ok_or_else(|| malformed(format!("response carries no PoS state: {response}")))?;
    let [bonds, active, pending, withdrawers, quarantine] = expr
        .get("ExprTuple")
        .and_then(|v| v.get("data"))
        .and_then(Value::as_array)
        .and_then(|parts| <&[Value; 5]>::try_from(parts.as_slice()).ok())
        .ok_or_else(|| {
            malformed(format!(
                "expected (bonds, active, pendingWithdrawers, withdrawers, quarantineLength), \
                 got {expr}"
            ))
        })?;
    let block_number = response
        .get("block")
        .and_then(|block| block.get("blockNumber"))
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            malformed(format!(
                "response does not name the block it read: {response}"
            ))
        })?;

    let quarantine = convert_rholang_to_json(quarantine)?;
    Ok(PosSnapshot {
        block_number,
        bonds: amount_map(bonds)?,
        active: key_set(&convert_rholang_to_json(active)?)?,
        pending_withdrawals: integer_map(&convert_rholang_to_json(pending)?, "pendingWithdrawers")?,
        withdrawals: withdrawal_map(withdrawers)?,
        quarantine_length: quarantine.as_i64().ok_or_else(|| {
            malformed(format!("quarantineLength is not an integer: {quarantine}"))
        })?,
    })
}

fn integer_map(value: &Value, name: &str) -> Result<BTreeMap<String, i64>> {
    value
        .as_object()
        .ok_or_else(|| malformed(format!("{name} is not a map: {value}")))?
        .iter()
        .map(|(key, entry)| {
            entry
                .as_i64()
                .map(|n| (key.to_ascii_lowercase(), n))
                .ok_or_else(|| malformed(format!("{name}[{key}] is not an integer: {entry}")))
        })
        .collect()
}

fn key_set(value: &Value) -> Result<BTreeSet<String>> {
    value
        .as_array()
        .ok_or_else(|| malformed(format!("activeValidators is not a set: {value}")))?
        .iter()
        .map(|key| {
            key.as_str().map(str::to_ascii_lowercase).ok_or_else(|| {
                malformed(format!("activeValidators entry is not a public key: {key}"))
            })
        })
        .collect()
}

fn amount_map(value: &Value) -> Result<BTreeMap<String, BigUint>> {
    value
        .get("ExprMap")
        .and_then(|v| v.get("data"))
        .and_then(Value::as_object)
        .ok_or_else(|| malformed(format!("bonds is not a map: {value}")))?
        .iter()
        .map(|(key, value)| {
            crate::rholang_helpers::rholang_amount(value)
                .map(|stake| (key.to_ascii_lowercase(), stake))
        })
        .collect()
}

fn withdrawal_map(value: &Value) -> Result<BTreeMap<String, Withdrawal>> {
    value
        .get("ExprMap")
        .and_then(|v| v.get("data"))
        .and_then(Value::as_object)
        .ok_or_else(|| malformed(format!("withdrawers is not a map: {value}")))?
        .iter()
        .map(|(key, entry)| {
            let pair = entry
                .get("ExprTuple")
                .and_then(|v| v.get("data"))
                .and_then(Value::as_array)
                .filter(|pair| pair.len() == 2)
                .ok_or_else(|| {
                    malformed(format!(
                        "withdrawers[{key}] is not a (stake, quarantineEnd) pair: {entry}"
                    ))
                })?;
            let stake = crate::rholang_helpers::rholang_amount(&pair[0])?;
            let quarantine_end = pair[1]
                .get("ExprInt")
                .and_then(|v| v.get("data"))
                .and_then(Value::as_i64)
                .ok_or_else(|| {
                    malformed(format!(
                        "withdrawers[{key}] quarantineEnd is not an integer"
                    ))
                })?;
            Ok((
                key.to_ascii_lowercase(),
                Withdrawal {
                    stake,
                    quarantine_end,
                },
            ))
        })
        .collect()
}

/// Deploy term that bonds the signing key with `stake`.
///
/// The PoS verdict goes to the deploy's `deployId` channel; read it with
/// [`parse_pos_call_result`].
pub fn build_bond_rholang(stake: &BigUint) -> Result<String> {
    let stake = crate::vault::amount_literal(stake)?;
    Ok(format!(
        r#"new deployId(`rho:system:deployId`), deployerId(`rho:system:deployerId`), rl(`rho:registry:lookup`), poSCh, resultCh in {{
 rl!(`rho:system:pos`, *poSCh) |
 for (@(_, PoS) <- poSCh) {{
 @PoS!("bond", *deployerId, {stake}, *resultCh)
 }} |
 for (@result <- resultCh) {{
 deployId!(result)
 }}
}}"#
    ))
}

/// Deploy term that withdraws the signing key's bond.
///
/// The withdrawal applies at the next epoch boundary. The PoS verdict goes to the
/// deploy's `deployId` channel; read it with [`parse_pos_call_result`].
pub const WITHDRAW_RHOLANG: &str = r#"new deployId(`rho:system:deployId`), deployerId(`rho:system:deployerId`), rl(`rho:registry:lookup`), poSCh, resultCh in {
 rl!(`rho:system:pos`, *poSCh) |
 for (@(_, PoS) <- poSCh) {
 @PoS!("withdraw", *deployerId, *resultCh)
 } |
 for (@result <- resultCh) {
 deployId!(result)
 }
}"#;

/// Interpret the `deployId` channel data of a PoS method call.
///
/// PoS methods answer `(true, _)` on success and `(false, reason)` on rejection.
/// No answer at all is also a failure: a call is never reported successful
/// without positive evidence.
pub fn parse_pos_call_result(data: &[Par]) -> Result<()> {
    let unexpected = |detail: String| Error::decode("PoS result", detail);
    let par = data.first().ok_or_else(|| {
        unexpected("nothing on the deployId channel; the PoS contract did not answer".into())
    })?;
    let tuple = par
        .exprs
        .iter()
        .find_map(|expr| match &expr.expr_instance {
            Some(ExprInstance::ETupleBody(tuple)) => Some(tuple),
            _ => None,
        })
        .ok_or_else(|| unexpected(format!("{par:?}")))?;
    let verdict = tuple.ps.first().and_then(|first| {
        first
            .exprs
            .iter()
            .find_map(|expr| match expr.expr_instance {
                Some(ExprInstance::GBool(verdict)) => Some(verdict),
                _ => None,
            })
    });

    match verdict {
        Some(true) => Ok(()),
        Some(false) => {
            let reason: Vec<&str> = tuple
                .ps
                .iter()
                .skip(1)
                .flat_map(|part| part.exprs.iter())
                .filter_map(|expr| match &expr.expr_instance {
                    Some(ExprInstance::GString(reason)) => Some(reason.as_str()),
                    _ => None,
                })
                .collect();
            Err(Error::ContractRejected {
                contract: Contract::ProofOfStake,
                reason: if reason.is_empty() {
                    format!("{tuple:?}")
                } else {
                    reason.join(": ")
                },
            })
        }
        None => Err(unexpected(format!("{par:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f1r3fly_models::rhoapi::{ETuple, Expr};
    use serde_json::json;

    const V1: &str = "04fa70d7be5eb750e0915c0f6d19e7085d18bb1c22d030feb2a877ca2cd226d04438aa819359c56c720142fbc66e9da03a5ab960a3d8b75363a226b7c800f60420";
    const V4: &str = "0429af984ed4da1a753030b1d06f0f3985379cf8935c953f92631afafff580c6ae447a9102789f69ec58ac69998cce512efecd0fae35dd61125f16b119455f1727";

    /// Shape of a live node's explore-deploy response to `POS_SNAPSHOT_QUERY`.
    fn response(bonds: Value, active: Value, pending: Value, withdrawers: Value) -> Value {
        let quarantine = json!({"ExprInt": {"data": 50000}});
        json!({
            "expr": [{"ExprTuple": {"data": [bonds, active, pending, withdrawers, quarantine]}}],
            "block": {"blockNumber": 51148},
            "cost": 0
        })
    }

    fn bonds(entries: &[(&str, i64)]) -> Value {
        let data: serde_json::Map<String, Value> = entries
            .iter()
            .map(|(key, stake)| (key.to_string(), json!({"ExprInt": {"data": stake}})))
            .collect();
        json!({"ExprMap": {"data": data}})
    }

    fn active(keys: &[&str]) -> Value {
        let data: Vec<Value> = keys
            .iter()
            .map(|key| json!({"ExprBytes": {"data": key}}))
            .collect();
        json!({"ExprSet": {"data": data}})
    }

    fn empty_map() -> Value {
        json!({"ExprMap": {"data": {}}})
    }

    #[test]
    fn bigint_stakes_and_withdrawals_keep_their_exact_value() {
        let stake = (BigUint::from(1u8) << 200usize) + BigUint::from(1u8);
        let bonds = json!({"ExprMap": {"data": {V4: {"ExprBigInt": {"data": stake.to_string()}}}}});
        let withdrawers = json!({"ExprMap": {"data": {V1: {"ExprTuple": {"data": [
            {"ExprBigInt": {"data": stake.to_string()}}, {"ExprInt": {"data": 49610}}
        ]}}}}});
        let snapshot =
            parse_pos_snapshot(&response(bonds, active(&[V4]), empty_map(), withdrawers)).unwrap();
        assert_eq!(snapshot.stake(V4), Some(stake.clone()));
        assert_eq!(
            snapshot.withdrawal(V1).unwrap(),
            Withdrawal {
                stake: stake.clone(),
                quarantine_end: 49610
            }
        );
        let term = build_bond_rholang(&stake).unwrap();
        assert!(term.contains(&format!("{stake}n")));
        assert!(build_bond_rholang(&(BigUint::from(1u8) << 256usize)).is_err());
    }

    #[test]
    fn parses_bonded_and_active_validators() {
        let snapshot = parse_pos_snapshot(&response(
            bonds(&[(V1, 1000), (V4, 1000)]),
            active(&[V1, V4]),
            empty_map(),
            empty_map(),
        ))
        .unwrap();

        assert_eq!(snapshot.block_number, 51148);
        assert_eq!(snapshot.quarantine_length, 50000);
        assert_eq!(snapshot.stake(V1), Some(BigUint::from(1000u32)));
        assert!(snapshot.is_active(V4));
        assert_eq!(snapshot.inactive_bonds().count(), 0);
        assert!(snapshot.pending_withdrawals.is_empty());
        assert!(snapshot.withdrawals.is_empty());
    }

    #[test]
    fn bonded_key_outside_the_active_set_is_inactive() {
        let snapshot = parse_pos_snapshot(&response(
            bonds(&[(V1, 1000), (V4, 1000)]),
            active(&[V1]),
            empty_map(),
            empty_map(),
        ))
        .unwrap();

        assert_eq!(snapshot.stake(V4), Some(BigUint::from(1000u32)));
        assert!(!snapshot.is_active(V4));
        assert_eq!(
            snapshot.inactive_bonds().collect::<Vec<_>>(),
            vec![(V4, &BigUint::from(1000u32))]
        );
    }

    #[test]
    fn parses_pending_and_applied_withdrawals() {
        let pending = json!({"ExprMap": {"data": {V4: {"ExprInt": {"data": 49610}}}}});
        let withdrawers = json!({"ExprMap": {"data": {V1: {"ExprTuple": {"data": [
            {"ExprInt": {"data": 1000}}, {"ExprInt": {"data": 49610}}
        ]}}}}});
        let snapshot = parse_pos_snapshot(&response(
            bonds(&[(V4, 1000)]),
            active(&[V4]),
            pending,
            withdrawers,
        ))
        .unwrap();

        assert_eq!(snapshot.pending_withdrawal(V4), Some(49610));
        assert_eq!(
            snapshot.withdrawal(V1),
            Some(Withdrawal {
                stake: BigUint::from(1000u32),
                quarantine_end: 49610
            })
        );
        assert_eq!(snapshot.stake(V1), None);
    }

    #[test]
    fn lookups_ignore_key_case() {
        let snapshot = parse_pos_snapshot(&response(
            bonds(&[(V1, 1000)]),
            active(&[V1]),
            empty_map(),
            empty_map(),
        ))
        .unwrap();

        assert_eq!(
            snapshot.stake(&V1.to_ascii_uppercase()),
            Some(BigUint::from(1000u32))
        );
        assert!(snapshot.is_active(&V1.to_ascii_uppercase()));
    }

    #[test]
    fn rejects_a_response_without_pos_state() {
        let err = parse_pos_snapshot(&json!({"expr": [], "block": {}})).unwrap_err();
        assert!(decode_detail(&err).contains("no PoS state"), "{err}");
    }

    #[test]
    fn rejects_a_response_that_does_not_name_its_block() {
        let mut unnamed = response(
            bonds(&[(V1, 1000)]),
            active(&[V1]),
            empty_map(),
            empty_map(),
        );
        unnamed["block"] = json!({});
        let err = parse_pos_snapshot(&unnamed).unwrap_err();
        assert!(
            decode_detail(&err).contains("does not name the block"),
            "{err}"
        );
    }

    #[test]
    fn rejects_a_non_integer_quarantine_length() {
        let mut bad = response(
            bonds(&[(V1, 1000)]),
            active(&[V1]),
            empty_map(),
            empty_map(),
        );
        bad["expr"][0]["ExprTuple"]["data"][4] = json!({"ExprString": {"data": "20"}});
        let err = parse_pos_snapshot(&bad).unwrap_err();
        assert!(
            decode_detail(&err).contains("quarantineLength is not an integer"),
            "{err}"
        );
    }

    #[test]
    fn rejects_a_result_of_the_wrong_arity() {
        let bad = json!({"expr": [{"ExprTuple": {"data": [bonds(&[(V1, 1000)]), active(&[V1])]}}]});
        let err = parse_pos_snapshot(&bad).unwrap_err();
        assert!(decode_detail(&err).contains("expected (bonds"), "{err}");
    }

    #[test]
    fn rejects_a_non_integer_stake() {
        let bad_bonds = json!({"ExprMap": {"data": {V1: {"ExprString": {"data": "1000"}}}}});
        let err = parse_pos_snapshot(&response(
            bad_bonds,
            active(&[V1]),
            empty_map(),
            empty_map(),
        ))
        .unwrap_err();
        assert!(decode_detail(&err).contains("is not an integer"), "{err}");
    }

    fn decode_detail(err: &Error) -> &str {
        match err {
            Error::Decode { detail, .. } => detail,
            other => panic!("expected a decode error, got {other:?}"),
        }
    }

    fn single(expr_instance: ExprInstance) -> Par {
        Par {
            exprs: vec![Expr {
                expr_instance: Some(expr_instance),
            }],
            ..Default::default()
        }
    }

    fn tuple(parts: Vec<Par>) -> Vec<Par> {
        vec![single(ExprInstance::ETupleBody(ETuple {
            ps: parts,
            ..Default::default()
        }))]
    }

    #[test]
    fn accepts_a_successful_pos_call() {
        let data = tuple(vec![single(ExprInstance::GBool(true)), Par::default()]);
        assert!(parse_pos_call_result(&data).is_ok());
    }

    #[test]
    fn reports_the_pos_rejection_reason() {
        let data = tuple(vec![
            single(ExprInstance::GBool(false)),
            single(ExprInstance::GString(
                "Public key is already bonded.".into(),
            )),
        ]);
        assert!(matches!(
            parse_pos_call_result(&data),
            Err(Error::ContractRejected {
                contract: Contract::ProofOfStake,
                ref reason,
            }) if reason == "Public key is already bonded."
        ));
    }

    #[test]
    fn treats_a_missing_answer_as_failure() {
        let err = parse_pos_call_result(&[]).unwrap_err();
        assert!(decode_detail(&err).contains("did not answer"), "{err}");
    }

    #[test]
    fn rejects_a_result_that_is_not_a_verdict_tuple() {
        let err = parse_pos_call_result(&[single(ExprInstance::GInt(1))]).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Decode {
                    what: "PoS result",
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn bond_term_passes_the_stake_to_pos() {
        let term = build_bond_rholang(&BigUint::from(1000u32)).unwrap();
        assert!(
            term.contains(r#"@PoS!("bond", *deployerId, 1000, *resultCh)"#),
            "{term}"
        );
        assert!(term.contains("deployId!(result)"), "{term}");
    }

    #[test]
    fn withdraw_term_reports_the_pos_verdict() {
        assert!(WITHDRAW_RHOLANG.contains(r#"@PoS!("withdraw", *deployerId, *resultCh)"#));
        assert!(WITHDRAW_RHOLANG.contains("deployId!(result)"));
    }
}
