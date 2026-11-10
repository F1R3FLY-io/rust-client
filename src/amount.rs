//! Canonical uint256 monetary values at node RPC and signing boundaries.
use f1r3fly_shared::rust::token_amount::TokenAmount;
use num_bigint::BigUint;
use serde::{Serialize, Serializer};

use crate::error::{Error, Result};

pub(crate) fn token(value: &BigUint) -> Result<TokenAmount> {
    TokenAmount::new(value.clone().into()).map_err(|e| Error::invalid("uint256 amount", e))
}

pub(crate) fn parse(value: &str) -> Result<BigUint> {
    TokenAmount::parse(value)
        .map(|amount| {
            amount
                .into_bigint()
                .to_biguint()
                .expect("unsigned TokenAmount")
        })
        .map_err(|e| Error::invalid("uint256 decimal string", e))
}

#[cfg(feature = "cli")]
pub(crate) fn parse_price(value: &str) -> Result<BigUint> {
    let price = parse(value)?;
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
