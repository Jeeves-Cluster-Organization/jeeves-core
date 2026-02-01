//! Request validation utilities.

// Allow dead code - these will be used in Phase 4 when implementing gRPC services
#![allow(dead_code)]

/// Validate that a string is not empty.
pub fn validate_non_empty(s: &str, field: &str) -> crate::types::Result<()> {
    if s.is_empty() {
        return Err(crate::types::Error::validation(format!(
            "{} cannot be empty",
            field
        )));
    }
    Ok(())
}

/// Validate that a value is positive.
pub fn validate_positive(n: u32, field: &str) -> crate::types::Result<()> {
    if n == 0 {
        return Err(crate::types::Error::validation(format!(
            "{} must be positive",
            field
        )));
    }
    Ok(())
}
