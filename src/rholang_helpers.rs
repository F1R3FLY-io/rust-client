//! Helper functions for parsing Rholang responses from F1r3fly
//!
//! Rholang expressions are returned in a structured format (ExprMap, ExprString, etc.)
//! These helpers convert them to plain JSON for easier consumption.

use crate::error::{Error, Result};

/// Convert a Rholang expression (from explore-deploy) to plain JSON
///
/// Recursively unwraps ExprMap, ExprString, ExprInt, ExprBool, etc.
/// into standard JSON types.
pub fn convert_rholang_to_json(value: &serde_json::Value) -> Result<serde_json::Value> {
    if let Some(expr_map) = value.get("ExprMap").and_then(|v| v.get("data")) {
        let mut result = serde_json::Map::new();
        if let Some(map_obj) = expr_map.as_object() {
            for (key, val) in map_obj {
                result.insert(key.clone(), convert_rholang_to_json(val)?);
            }
        }
        return Ok(serde_json::Value::Object(result));
    }

    if let Some(expr_str) = value.get("ExprString").and_then(|v| v.get("data")) {
        return Ok(expr_str.clone());
    }

    if let Some(expr_int) = value.get("ExprInt").and_then(|v| v.get("data")) {
        return Ok(expr_int.clone());
    }

    if let Some(data) = value.get("ExprBigInt").and_then(|v| v.get("data")) {
        return Ok(serde_json::Value::String(
            canonical_bigint(data)?.to_string(),
        ));
    }

    if let Some(expr_bool) = value.get("ExprBool").and_then(|v| v.get("data")) {
        return Ok(expr_bool.clone());
    }

    // Byte arrays arrive hex-encoded.
    if let Some(expr_bytes) = value.get("ExprBytes").and_then(|v| v.get("data")) {
        return Ok(expr_bytes.clone());
    }

    if let Some(items) = ["ExprTuple", "ExprList", "ExprSet"]
        .iter()
        .find_map(|kind| value.get(*kind))
        .and_then(|v| v.get("data"))
    {
        return convert_rholang_to_json(items);
    }

    if let Some(arr) = value.as_array() {
        let mut result = Vec::new();
        for item in arr {
            result.push(convert_rholang_to_json(item)?);
        }
        return Ok(serde_json::Value::Array(result));
    }

    Ok(value.clone())
}

/// An unsigned uint256 amount the node reported as `ExprInt` or `ExprBigInt`.
pub fn rholang_amount(value: &serde_json::Value) -> Result<num_bigint::BigUint> {
    let amount = if let Some(data) = value.get("ExprInt").and_then(|v| v.get("data")) {
        data.as_u64()
            .map(num_bigint::BigUint::from)
            .ok_or_else(|| Error::decode("amount", format!("{data} is not unsigned")))?
    } else if let Some(data) = value.get("ExprBigInt").and_then(|v| v.get("data")) {
        canonical_bigint(data)?
            .to_biguint()
            .ok_or_else(|| Error::decode("amount", format!("{data} is not unsigned")))?
    } else {
        return Err(Error::decode(
            "amount",
            format!("{value} is not an integer expression"),
        ));
    };
    if amount.bits() > 256 {
        return Err(Error::decode("amount", format!("{amount} exceeds uint256")));
    }
    Ok(amount)
}

/// The node writes a BigInt as its canonical decimal string; anything else is malformed.
fn canonical_bigint(data: &serde_json::Value) -> Result<num_bigint::BigInt> {
    let text = data
        .as_str()
        .ok_or_else(|| Error::decode("BigInt", format!("{data} is not a decimal string")))?;
    let value = text
        .parse::<num_bigint::BigInt>()
        .map_err(|e| Error::decode("BigInt", format!("{text}: {e}")))?;
    if value.to_string() != text {
        return Err(Error::decode("BigInt", format!("{text} is not canonical")));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bigint_json_preserves_exact_signed_decimal_strings() {
        for text in [
            "0",
            "1606938044258990275541962092341162602522202993782792835301377",
            "-1606938044258990275541962092341162602522202993782792835301377",
        ] {
            let value = json!({"ExprList": {"data": [{"ExprBigInt": {"data": text}}]}});
            assert_eq!(convert_rholang_to_json(&value).unwrap(), json!([text]));
        }
        for value in [
            json!(42),
            json!("01"),
            json!("-0"),
            json!("1.5"),
            json!("1e20"),
        ] {
            assert!(convert_rholang_to_json(&json!({"ExprBigInt": {"data": value}})).is_err());
        }
    }

    #[test]
    fn monetary_expressions_reject_negative_and_wrong_types() {
        for value in [
            json!({"ExprInt": {"data": -1}}),
            json!({"ExprBigInt": {"data": "-1"}}),
            json!({"ExprString": {"data": "1000"}}),
        ] {
            assert!(rholang_amount(&value).is_err());
        }
    }

    #[test]
    fn test_convert_expr_string() {
        let input = json!({"ExprString": {"data": "hello"}});
        let result = convert_rholang_to_json(&input).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn test_convert_expr_int() {
        let input = json!({"ExprInt": {"data": 42}});
        let result = convert_rholang_to_json(&input).unwrap();
        assert_eq!(result, json!(42));
    }

    #[test]
    fn test_convert_expr_bool() {
        let input = json!({"ExprBool": {"data": true}});
        let result = convert_rholang_to_json(&input).unwrap();
        assert_eq!(result, json!(true));
    }

    #[test]
    fn test_convert_expr_map() {
        let input = json!({
        "ExprMap": {
        "data": {
        "name": {"ExprString": {"data": "Alice"}},
        "age": {"ExprInt": {"data": 30}},
        "active": {"ExprBool": {"data": true}}
        }
        }
        });
        let result = convert_rholang_to_json(&input).unwrap();
        assert_eq!(result, json!({"name": "Alice", "age": 30, "active": true}));
    }

    #[test]
    fn test_convert_nested_map() {
        let input = json!({
        "ExprMap": {
        "data": {
        "user": {
        "ExprMap": {
        "data": {
        "name": {"ExprString": {"data": "Bob"}}
        }
        }
        }
        }
        }
        });
        let result = convert_rholang_to_json(&input).unwrap();
        assert_eq!(result, json!({"user": {"name": "Bob"}}));
    }

    #[test]
    fn test_convert_expr_bytes() {
        let input = json!({"ExprBytes": {"data": "04d26c61"}});
        let result = convert_rholang_to_json(&input).unwrap();
        assert_eq!(result, json!("04d26c61"));
    }

    #[test]
    fn test_convert_collections() {
        let elements = json!([{"ExprInt": {"data": 1}}, {"ExprBytes": {"data": "ab"}}]);
        for kind in ["ExprTuple", "ExprList", "ExprSet"] {
            let mut input = serde_json::Map::new();
            input.insert(kind.to_string(), json!({"data": elements}));
            let result = convert_rholang_to_json(&serde_json::Value::Object(input)).unwrap();
            assert_eq!(result, json!([1, "ab"]), "{kind}");
        }
    }

    #[test]
    fn test_convert_array() {
        let input = json!([
        {"ExprString": {"data": "a"}},
        {"ExprString": {"data": "b"}},
        {"ExprInt": {"data": 1}}
        ]);
        let result = convert_rholang_to_json(&input).unwrap();
        assert_eq!(result, json!(["a", "b", 1]));
    }
}
