//! Integers at the node boundary: canonical decimal text, uint256 monetary
//! values and their 32-byte wire form, and the JSON numbers older nodes sent.
use f1r3fly_shared::rust::decimal;
use f1r3fly_shared::rust::token_amount::TokenAmount;
use num_bigint::{BigInt, BigUint};
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::error::{Error, Result};

pub(crate) fn token(value: &BigUint) -> Result<TokenAmount> {
    TokenAmount::new(value.clone().into()).map_err(|e| Error::invalid("uint256 amount", e))
}

/// A uint256 written as a canonical decimal.
pub(crate) fn uint256(text: &str) -> std::result::Result<BigUint, String> {
    TokenAmount::parse(text).map(unsigned)
}

/// A signed integer of any size written as a canonical decimal.
pub(crate) fn signed(text: &str) -> std::result::Result<BigInt, String> {
    decimal::parse_signed(text)
}

/// An integer as a node writes it in JSON: a canonical decimal string, or the
/// exact JSON number older nodes sent, which is only exact within 64 bits.
pub(crate) fn json_integer(value: &Value) -> std::result::Result<BigInt, String> {
    match value {
        Value::String(text) => signed(text),
        Value::Number(number) => number
            .as_i64()
            .map(BigInt::from)
            .or_else(|| number.as_u64().map(BigInt::from))
            .ok_or_else(|| format!("{number} is not a 64-bit integer")),
        _ => Err(format!("{value} is not an integer")),
    }
}

/// A uint256 as a node writes it in JSON; see [`json_integer`].
pub(crate) fn json_uint256(value: &Value) -> std::result::Result<BigUint, String> {
    TokenAmount::new(json_integer(value)?).map(unsigned)
}

fn unsigned(amount: TokenAmount) -> BigUint {
    amount
        .into_bigint()
        .to_biguint()
        .expect("a TokenAmount is never negative")
}

#[cfg(feature = "cli")]
pub(crate) fn parse_price(value: &str) -> Result<BigUint> {
    let price = uint256(value).map_err(|e| Error::invalid("phlo price", e))?;
    if price == BigUint::default() {
        return Err(Error::invalid("phlo price", "must be positive"));
    }
    Ok(price)
}

pub(crate) fn serialize<S: Serializer>(value: &BigUint, s: S) -> Result<S::Ok, S::Error> {
    token(value)
        .map_err(serde::ser::Error::custom)?
        .serialize(s)
}

pub(crate) mod option {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        value: &Option<BigUint>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .as_ref()
            .map(token)
            .transpose()
            .map_err(serde::ser::Error::custom)?
            .serialize(s)
    }
}
