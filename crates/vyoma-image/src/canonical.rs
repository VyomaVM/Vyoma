//! RFC 8785: JSON Canonicalization Scheme (JCS) implementation.
//!
//! Provides deterministic, canonical JSON serialization ensuring that two semantically
//! identical JSON values produce the exact same byte representation, independent of key
//! ordering or whitespace differences.

use serde::Serialize;
use serde_json::Value;
use std::fmt::Write as FmtWrite;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CanonicalJsonError {
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

/// Serializes any Serde-serializable type into RFC 8785 Canonical JSON bytes.
pub fn to_canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, CanonicalJsonError> {
    let json_value = serde_json::to_value(value)?;
    let mut out = String::new();
    canonicalize_value(&json_value, &mut out)?;
    Ok(out.into_bytes())
}

/// Serializes a `serde_json::Value` into RFC 8785 Canonical JSON string.
pub fn canonicalize_value(value: &Value, out: &mut String) -> Result<(), CanonicalJsonError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(num) => {
            if let Some(i) = num.as_i64() {
                write!(out, "{}", i)?;
            } else if let Some(u) = num.as_u64() {
                write!(out, "{}", u)?;
            } else if let Some(f) = num.as_f64() {
                // Check for NaN or infinity which are not valid JSON
                if !f.is_finite() {
                    out.push_str("null");
                } else {
                    // Canonical formatting for floats per RFC 8785 (ECMAScript Number.prototype.toString)
                    if f.fract() == 0.0 && f.abs() < 1e21 && f.abs() >= 1e-6 {
                        write!(out, "{:.0}", f)?;
                    } else {
                        write!(out, "{}", f)?;
                    }
                }
            } else {
                write!(out, "{}", num)?;
            }
        }
        Value::String(s) => escape_json_string(s, out),
        Value::Array(arr) => {
            out.push('[');
            for (i, elem) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonicalize_value(elem, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            // Sort keys lexicographically by UTF-8 bytes (Unicode code points)
            let mut sorted_keys: Vec<&String> = map.keys().collect();
            sorted_keys.sort();

            for (i, key) in sorted_keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                escape_json_string(key, out);
                out.push(':');
                canonicalize_value(&map[*key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// Escapes a string according to RFC 8785 Section 3.2.2.2.
fn escape_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_canonical_primitive_types() {
        assert_eq!(to_canonical_json(&Value::Null).unwrap(), b"null");
        assert_eq!(to_canonical_json(&true).unwrap(), b"true");
        assert_eq!(to_canonical_json(&false).unwrap(), b"false");
        assert_eq!(to_canonical_json(&42).unwrap(), b"42");
        assert_eq!(to_canonical_json(&-100).unwrap(), b"-100");
        assert_eq!(to_canonical_json(&0).unwrap(), b"0");
    }

    #[test]
    fn test_canonical_string_escaping() {
        assert_eq!(to_canonical_json(&"hello world").unwrap(), b"\"hello world\"");
        assert_eq!(to_canonical_json(&"quote \" and slash \\").unwrap(), b"\"quote \\\" and slash \\\\\"");
        assert_eq!(to_canonical_json(&"line1\nline2\ttab").unwrap(), b"\"line1\\nline2\\ttab\"");
        assert_eq!(to_canonical_json(&"\x01\x1f").unwrap(), b"\"\\u0001\\u001f\"");
    }

    #[test]
    fn test_canonical_array_no_whitespace() {
        let arr = vec![1, 2, 3];
        assert_eq!(to_canonical_json(&arr).unwrap(), b"[1,2,3]");

        let nested = vec![vec!["a", "b"], vec!["c"]];
        assert_eq!(to_canonical_json(&nested).unwrap(), b"[[\"a\",\"b\"],[\"c\"]]");
    }

    #[test]
    fn test_canonical_object_key_sorting() {
        // Construct JSON with unordered keys
        let val = json!({
            "zebra": 1,
            "apple": 2,
            "banana": {
                "yellow": true,
                "ripe": false
            },
            "carrot": 3
        });

        let canonical = to_canonical_json(&val).unwrap();
        let expected = b"{\"apple\":2,\"banana\":{\"ripe\":false,\"yellow\":true},\"carrot\":3,\"zebra\":1}";
        assert_eq!(canonical, expected);
    }

    #[test]
    fn test_canonical_hashmap_determinism() {
        // Run multiple iterations with randomized HashMap insertions
        let mut map1 = HashMap::new();
        map1.insert("first", "value1");
        map1.insert("second", "value2");
        map1.insert("third", "value3");

        let mut map2 = HashMap::new();
        map2.insert("third", "value3");
        map2.insert("first", "value1");
        map2.insert("second", "value2");

        let json1 = to_canonical_json(&map1).unwrap();
        let json2 = to_canonical_json(&map2).unwrap();

        assert_eq!(json1, json2);
        assert_eq!(json1, b"{\"first\":\"value1\",\"second\":\"value2\",\"third\":\"value3\"}");
    }
}
