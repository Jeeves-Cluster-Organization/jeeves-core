//! Shared IPC input validation helpers.
//!
//! These enforce that client-supplied numeric fields are within valid ranges
//! before being passed to kernel internals, preventing silent integer overflow.

use crate::types::{Error, Result};
use serde_json::Value;

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
