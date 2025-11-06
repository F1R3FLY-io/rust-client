/// Helper functions for parsing Rholang responses from F1r3fly
///
/// Rholang expressions are returned in a structured format (ExprMap, ExprString, etc.)
/// These helpers convert them to plain JSON for easier consumption.

use serde_json;

/// Convert a Rholang expression (from explore-deploy) to plain JSON
///
/// This recursively unwraps ExprMap, ExprString, ExprInt, ExprBool, etc.
/// into standard JSON types for easier parsing.
///
/// # Arguments
///
/// * `value` - The Rholang expression as JSON (from F1r3flyApi response)
///
/// # Returns
///
/// Plain JSON value with Rholang wrappers removed
///
/// # Example
///
/// ```ignore
/// // Rholang response:
/// // {"ExprMap": {"data": {"name": {"ExprString": {"data": "Alice"}}}}}
/// 
/// // After conversion:
/// // {"name": "Alice"}
/// ```
pub fn convert_rholang_to_json(
    value: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    // Handle ExprMap - recursively unwrap all fields
    if let Some(expr_map) = value.get("ExprMap").and_then(|v| v.get("data")) {
        let mut result = serde_json::Map::new();
        if let Some(map_obj) = expr_map.as_object() {
            for (key, val) in map_obj {
                result.insert(key.clone(), convert_rholang_to_json(val)?);
            }
        }
        return Ok(serde_json::Value::Object(result));
    }

    // Handle ExprString - extract the string value
    if let Some(expr_str) = value.get("ExprString").and_then(|v| v.get("data")) {
        return Ok(expr_str.clone());
    }

    // Handle ExprInt - extract the integer value
    if let Some(expr_int) = value.get("ExprInt").and_then(|v| v.get("data")) {
        return Ok(expr_int.clone());
    }

    // Handle ExprBool - extract the boolean value
    if let Some(expr_bool) = value.get("ExprBool").and_then(|v| v.get("data")) {
        return Ok(expr_bool.clone());
    }

    // Handle arrays - recursively convert each element
    if let Some(arr) = value.as_array() {
        let mut result = Vec::new();
        for item in arr {
            result.push(convert_rholang_to_json(item)?);
        }
        return Ok(serde_json::Value::Array(result));
    }

    // If we can't convert, return as-is
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert_eq!(
            result,
            json!({
                "name": "Alice",
                "age": 30,
                "active": true
            })
        );
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
        assert_eq!(
            result,
            json!({
                "user": {
                    "name": "Bob"
                }
            })
        );
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

