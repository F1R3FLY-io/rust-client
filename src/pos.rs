//! Proof-of-Stake contract state
//!
//! Reads the PoS contract's validator bookkeeping: bonded stake, the active
//! set, and validators on their way out.

use std::collections::{BTreeMap, BTreeSet};

use f1r3fly_models::rhoapi::expr::ExprInstance;
use f1r3fly_models::rhoapi::Par;
use serde_json::Value;

use crate::rholang_helpers::convert_rholang_to_json;

/// Exploratory term returning `(allBonds, activeValidators, pendingWithdrawers, withdrawers)`.
///
/// The four getters run in one term so they describe the same block.
pub const POS_SNAPSHOT_QUERY: &str = r#"new return, rl(`rho:registry:lookup`), poSCh in {
 rl!(`rho:system:pos`, *poSCh) |
 for (@(_, PoS) <- poSCh) {
 new bondsCh, activeCh, pendingCh, withdrawersCh in {
 @PoS!("getBonds", *bondsCh) |
 @PoS!("getActiveValidators", *activeCh) |
 @PoS!("getPendingWithdrawer", *pendingCh) |
 @PoS!("getWithdrawers", *withdrawersCh) |
 for (@bonds <- bondsCh & @active <- activeCh & @pending <- pendingCh & @withdrawers <- withdrawersCh) {
 return!((bonds, active, pending, withdrawers))
 }
 }
 }
}"#;

/// A validator removed from the bond set whose stake is still quarantined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Withdrawal {
    pub stake: i64,
    /// Stake is paid out at the first epoch boundary at or after this block.
    pub quarantine_end: i64,
}

/// Validator bookkeeping of the PoS contract at one block.
///
/// Public keys are lowercase hex.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PosSnapshot {
    /// Bonded validators and their stake. A bond enters as soon as its deploy executes.
    pub bonds: BTreeMap<String, i64>,
    /// Validators in consensus. Recomputed only at epoch boundaries.
    pub active: BTreeSet<String>,
    /// Requested withdrawals, keyed to their quarantine end. Applied at the next epoch boundary.
    pub pending_withdrawals: BTreeMap<String, i64>,
    /// Applied withdrawals awaiting payout. These keys are no longer in `bonds`.
    pub withdrawals: BTreeMap<String, Withdrawal>,
}

impl PosSnapshot {
    pub fn stake(&self, public_key: &str) -> Option<i64> {
        self.bonds.get(&public_key.to_ascii_lowercase()).copied()
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
            .copied()
    }

    /// Bonded validators that have not entered the active set yet.
    pub fn pending_activation(&self) -> impl Iterator<Item = (&str, i64)> {
        self.bonds
            .iter()
            .filter(|(key, _)| !self.active.contains(*key))
            .map(|(key, stake)| (key.as_str(), *stake))
    }
}

/// Parse an explore-deploy response to [`POS_SNAPSHOT_QUERY`].
pub fn parse_pos_snapshot(response: &Value) -> Result<PosSnapshot, String> {
    let expr = response
        .get("expr")
        .and_then(Value::as_array)
        .and_then(|exprs| exprs.first())
        .ok_or_else(|| format!("response carries no PoS state: {response}"))?;
    let state = convert_rholang_to_json(expr).map_err(|e| e.to_string())?;
    let [bonds, active, pending, withdrawers] = state
        .as_array()
        .and_then(|parts| <&[Value; 4]>::try_from(parts.as_slice()).ok())
        .ok_or_else(|| {
            format!("expected (bonds, active, pendingWithdrawers, withdrawers), got {state}")
        })?;

    Ok(PosSnapshot {
        bonds: integer_map(bonds, "bonds")?,
        active: key_set(active)?,
        pending_withdrawals: integer_map(pending, "pendingWithdrawers")?,
        withdrawals: withdrawal_map(withdrawers)?,
    })
}

fn integer_map(value: &Value, name: &str) -> Result<BTreeMap<String, i64>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{name} is not a map: {value}"))?
        .iter()
        .map(|(key, entry)| {
            entry
                .as_i64()
                .map(|n| (key.to_ascii_lowercase(), n))
                .ok_or_else(|| format!("{name}[{key}] is not an integer: {entry}"))
        })
        .collect()
}

fn key_set(value: &Value) -> Result<BTreeSet<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("activeValidators is not a set: {value}"))?
        .iter()
        .map(|key| {
            key.as_str()
                .map(str::to_ascii_lowercase)
                .ok_or_else(|| format!("activeValidators entry is not a public key: {key}"))
        })
        .collect()
}

fn withdrawal_map(value: &Value) -> Result<BTreeMap<String, Withdrawal>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("withdrawers is not a map: {value}"))?
        .iter()
        .map(|(key, entry)| {
            let numbers = entry
                .as_array()
                .and_then(|items| items.iter().map(Value::as_i64).collect::<Option<Vec<_>>>());
            match numbers.as_deref() {
                Some(&[stake, quarantine_end]) => Ok((
                    key.to_ascii_lowercase(),
                    Withdrawal {
                        stake,
                        quarantine_end,
                    },
                )),
                _ => Err(format!(
                    "withdrawers[{key}] is not a (stake, quarantineEnd) pair: {entry}"
                )),
            }
        })
        .collect()
}

/// Deploy term that bonds the signing key with `stake`.
///
/// The PoS verdict goes to the deploy's `deployId` channel; read it with
/// [`parse_pos_call_result`].
pub fn build_bond_rholang(stake: u64) -> String {
    format!(
        r#"new deployId(`rho:system:deployId`), deployerId(`rho:system:deployerId`), rl(`rho:registry:lookup`), poSCh, resultCh in {{
 rl!(`rho:system:pos`, *poSCh) |
 for (@(_, PoS) <- poSCh) {{
 @PoS!("bond", *deployerId, {stake}, *resultCh)
 }} |
 for (@result <- resultCh) {{
 deployId!(result)
 }}
}}"#
    )
}

/// Interpret the `deployId` channel data of a PoS method call.
///
/// PoS methods answer `(true, _)` on success and `(false, reason)` on rejection.
/// No answer at all is also a failure: a call is never reported successful
/// without positive evidence.
pub fn parse_pos_call_result(data: &[Par]) -> Result<(), String> {
    let par = data.first().ok_or_else(|| {
        "no result on the deployId channel; the PoS contract did not answer".to_string()
    })?;
    let tuple = par
        .exprs
        .iter()
        .find_map(|expr| match &expr.expr_instance {
            Some(ExprInstance::ETupleBody(tuple)) => Some(tuple),
            _ => None,
        })
        .ok_or_else(|| format!("unexpected PoS result shape: {par:?}"))?;
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
            Err(if reason.is_empty() {
                format!("PoS rejected the call: {tuple:?}")
            } else {
                reason.join(": ")
            })
        }
        None => Err(format!("unexpected PoS result shape: {par:?}")),
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
        json!({
            "expr": [{"ExprTuple": {"data": [bonds, active, pending, withdrawers]}}],
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
    fn parses_bonded_and_active_validators() {
        let snapshot = parse_pos_snapshot(&response(
            bonds(&[(V1, 1000), (V4, 1000)]),
            active(&[V1, V4]),
            empty_map(),
            empty_map(),
        ))
        .unwrap();

        assert_eq!(snapshot.stake(V1), Some(1000));
        assert!(snapshot.is_active(V4));
        assert_eq!(snapshot.pending_activation().count(), 0);
        assert!(snapshot.pending_withdrawals.is_empty());
        assert!(snapshot.withdrawals.is_empty());
    }

    #[test]
    fn bonded_key_outside_the_active_set_is_pending_activation() {
        let snapshot = parse_pos_snapshot(&response(
            bonds(&[(V1, 1000), (V4, 1000)]),
            active(&[V1]),
            empty_map(),
            empty_map(),
        ))
        .unwrap();

        assert_eq!(snapshot.stake(V4), Some(1000));
        assert!(!snapshot.is_active(V4));
        assert_eq!(
            snapshot.pending_activation().collect::<Vec<_>>(),
            vec![(V4, 1000)]
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
                stake: 1000,
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

        assert_eq!(snapshot.stake(&V1.to_ascii_uppercase()), Some(1000));
        assert!(snapshot.is_active(&V1.to_ascii_uppercase()));
    }

    #[test]
    fn rejects_a_response_without_pos_state() {
        let err = parse_pos_snapshot(&json!({"expr": [], "block": {}})).unwrap_err();
        assert!(err.contains("no PoS state"), "{err}");
    }

    #[test]
    fn rejects_a_result_of_the_wrong_arity() {
        let bad = json!({"expr": [{"ExprTuple": {"data": [bonds(&[(V1, 1000)]), active(&[V1])]}}]});
        let err = parse_pos_snapshot(&bad).unwrap_err();
        assert!(err.contains("expected (bonds"), "{err}");
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
        assert!(err.contains("is not an integer"), "{err}");
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
        assert_eq!(parse_pos_call_result(&data), Ok(()));
    }

    #[test]
    fn reports_the_pos_rejection_reason() {
        let data = tuple(vec![
            single(ExprInstance::GBool(false)),
            single(ExprInstance::GString(
                "Public key is already bonded.".into(),
            )),
        ]);
        assert_eq!(
            parse_pos_call_result(&data),
            Err("Public key is already bonded.".to_string())
        );
    }

    #[test]
    fn treats_a_missing_answer_as_failure() {
        let err = parse_pos_call_result(&[]).unwrap_err();
        assert!(err.contains("did not answer"), "{err}");
    }

    #[test]
    fn rejects_a_result_that_is_not_a_verdict_tuple() {
        let err = parse_pos_call_result(&[single(ExprInstance::GInt(1))]).unwrap_err();
        assert!(err.contains("unexpected PoS result shape"), "{err}");
    }

    #[test]
    fn bond_term_passes_the_stake_to_pos() {
        let term = build_bond_rholang(1000);
        assert!(
            term.contains(r#"@PoS!("bond", *deployerId, 1000, *resultCh)"#),
            "{term}"
        );
        assert!(term.contains("deployId!(result)"), "{term}");
    }
}
