//! Envelope import from JSON (Go compatibility layer).
//!
//! This module handles deserialization of envelope state from Go-produced JSON,
//! including normalization of legacy artifacts (null → {}, float → int, etc.).

use serde::Deserialize;

/// Import envelope from JSON bytes (checkpoint 2 - stub for now).
pub fn from_json(_bytes: &[u8]) -> crate::types::Result<super::Envelope> {
    // TODO: Implement in checkpoint 2
    unimplemented!("from_json will be implemented in checkpoint 2")
}

/// Custom deserializer for int fields encoded as floats in JSON.
pub fn deserialize_int_from_float<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let f: f64 = f64::deserialize(deserializer)?;
    if f.fract() != 0.0 {
        return Err(serde::de::Error::custom("non-integer float"));
    }
    Ok(f as u32)
}
