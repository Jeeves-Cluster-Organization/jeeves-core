//! Envelope export to JSON.
//!
//! Infallible serialization using serde.

/// Export envelope to JSON bytes (checkpoint 2 - stub for now).
pub fn to_json(_envelope: &super::Envelope) -> crate::types::Result<Vec<u8>> {
    Err(crate::types::Error::internal(
        "to_json not yet implemented (checkpoint 2)",
    ))
}
