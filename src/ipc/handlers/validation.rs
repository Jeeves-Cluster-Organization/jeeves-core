//! Shared IPC input validation helpers.
//!
//! These enforce that client-supplied numeric fields are within valid ranges
//! before being passed to kernel internals, preventing silent integer overflow.

use crate::types::{Error, Result};
use serde_json::Value;

/// Parse a string into a serde-deserializable enum.
///
/// Uses serde_json round-trip: wraps the string in a JSON `Value::String`,
/// then deserializes. This means the string must match the enum's serde
/// rename (e.g. `"SCREAMING_SNAKE_CASE"` or `"snake_case"`).
pub fn parse_enum<T: serde::de::DeserializeOwned>(s: &str, field_name: &str) -> Result<T> {
    serde_json::from_value::<T>(Value::String(s.to_string()))
        .map_err(|_| Error::validation(format!("invalid {}: {}", field_name, s)))
}

/// Safely convert an i64 (from JSON) to i32, rejecting out-of-range values.
pub fn safe_i64_to_i32(value: i64, field: &str) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        Error::validation(format!("{field} value {value} exceeds i32 range"))
    })
}

/// Require a non-negative i64 value.
pub fn require_non_negative_i64(value: i64, field: &str) -> Result<i64> {
    if value < 0 {
        return Err(Error::validation(format!(
            "{field} must be non-negative, got {value}"
        )));
    }
    Ok(value)
}

/// Parse and validate an i32 field from JSON: must be non-negative and within i32 range.
pub fn parse_non_negative_i32(value: i64, field: &str) -> Result<i32> {
    require_non_negative_i64(value, field)?;
    safe_i64_to_i32(value, field)
}

/// Parse an optional i64 field from JSON and require non-negative values when present.
pub fn parse_optional_non_negative_i64(value: Option<&Value>, field: &str) -> Result<Option<i64>> {
    match value {
        None => Ok(None),
        Some(v) => {
            let parsed = v
                .as_i64()
                .ok_or_else(|| Error::validation(format!("{field} must be an integer")))?;
            require_non_negative_i64(parsed, field)?;
            Ok(Some(parsed))
        }
    }
}

/// Validate that an envelope update from Python doesn't violate kernel invariants.
pub fn validate_envelope_update(update: &Value) -> Result<()> {
    // Python must never set terminated=true — that's kernel's authority
    if let Some(bounds) = update.get("bounds") {
        if let Some(terminated) = bounds.get("terminated") {
            if terminated.as_bool() == Some(true) {
                return Err(Error::validation(
                    "Python worker cannot set bounds.terminated=true. Kernel is sole termination authority."
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::TerminalReason;

    // ── parse_enum ──────────────────────────────────────────────────────

    #[test]
    fn test_parse_enum_valid_terminal_reason() {
        let result: TerminalReason = parse_enum("COMPLETED", "terminal_reason").unwrap();
        assert_eq!(result, TerminalReason::Completed);
    }

    #[test]
    fn test_parse_enum_invalid_string() {
        let result = parse_enum::<TerminalReason>("NOT_A_REASON", "terminal_reason");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid terminal_reason"));
    }

    #[test]
    fn test_parse_enum_empty_string() {
        let result = parse_enum::<TerminalReason>("", "terminal_reason");
        assert!(result.is_err());
    }

    // ── safe_i64_to_i32 ────────────────────────────────────────────────

    #[test]
    fn test_safe_i64_to_i32_normal() {
        assert_eq!(safe_i64_to_i32(42, "field").unwrap(), 42);
    }

    #[test]
    fn test_safe_i64_to_i32_at_max() {
        assert_eq!(safe_i64_to_i32(i32::MAX as i64, "field").unwrap(), i32::MAX);
    }

    #[test]
    fn test_safe_i64_to_i32_overflow() {
        let result = safe_i64_to_i32(i32::MAX as i64 + 1, "field");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds i32 range"));
    }

    #[test]
    fn test_safe_i64_to_i32_i64_max() {
        let result = safe_i64_to_i32(i64::MAX, "field");
        assert!(result.is_err());
    }

    // ── require_non_negative_i64 ────────────────────────────────────────

    #[test]
    fn test_require_non_negative_i64_zero() {
        assert_eq!(require_non_negative_i64(0, "field").unwrap(), 0);
    }

    #[test]
    fn test_require_non_negative_i64_positive() {
        assert_eq!(require_non_negative_i64(1, "field").unwrap(), 1);
    }

    #[test]
    fn test_require_non_negative_i64_negative() {
        let result = require_non_negative_i64(-1, "field");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-negative"));
    }

    // ── parse_non_negative_i32 ──────────────────────────────────────────

    #[test]
    fn test_parse_non_negative_i32_zero() {
        assert_eq!(parse_non_negative_i32(0, "field").unwrap(), 0);
    }

    #[test]
    fn test_parse_non_negative_i32_valid() {
        assert_eq!(parse_non_negative_i32(5, "field").unwrap(), 5);
    }

    #[test]
    fn test_parse_non_negative_i32_negative() {
        let result = parse_non_negative_i32(-1, "field");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_non_negative_i32_overflow() {
        let result = parse_non_negative_i32(i32::MAX as i64 + 1, "field");
        assert!(result.is_err());
    }

    // ── parse_optional_non_negative_i64 ─────────────────────────────────

    #[test]
    fn test_parse_optional_non_negative_i64_none() {
        assert_eq!(parse_optional_non_negative_i64(None, "field").unwrap(), None);
    }

    #[test]
    fn test_parse_optional_non_negative_i64_valid() {
        let val = Value::from(5);
        assert_eq!(parse_optional_non_negative_i64(Some(&val), "field").unwrap(), Some(5));
    }

    #[test]
    fn test_parse_optional_non_negative_i64_negative() {
        let val = Value::from(-1);
        let result = parse_optional_non_negative_i64(Some(&val), "field");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_optional_non_negative_i64_non_integer() {
        let val = Value::String("not_a_number".to_string());
        let result = parse_optional_non_negative_i64(Some(&val), "field");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be an integer"));
    }

    // ── validate_envelope_update ────────────────────────────────────────

    #[test]
    fn test_validate_envelope_update_no_bounds() {
        let update = serde_json::json!({"raw_input": "hello"});
        assert!(validate_envelope_update(&update).is_ok());
    }

    #[test]
    fn test_validate_envelope_update_terminated_false_ok() {
        let update = serde_json::json!({"bounds": {"terminated": false}});
        assert!(validate_envelope_update(&update).is_ok());
    }

    #[test]
    fn test_validate_envelope_update_terminated_true_rejected() {
        let update = serde_json::json!({"bounds": {"terminated": true}});
        let result = validate_envelope_update(&update);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("sole termination authority"));
    }
}
