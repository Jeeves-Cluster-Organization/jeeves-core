//! Application error types.
//!
//! All errors use `thiserror` for automatic Error trait derivation and provide
//! clear error messages with context.

use thiserror::Error;

/// Application result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Main error enum for the Jeeves kernel.
#[derive(Error, Debug)]
pub enum Error {
    /// Validation errors.
    #[error("validation error: {0}")]
    Validation(String),

    /// Resource not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Quota or resource exhaustion.
    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),

    /// Invalid state transition.
    #[error("state transition error: {0}")]
    StateTransition(String),

    /// Internal errors.
    #[error("internal error: {0}")]
    Internal(String),

    /// Cancellation.
    #[error("operation cancelled: {0}")]
    Cancelled(String),

    /// Timeout.
    #[error("timeout: {0}")]
    Timeout(String),

    /// Serialization/deserialization errors.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// I/O errors.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Convert to IPC error code string for wire protocol.
    pub fn to_ipc_error_code(&self) -> &str {
        match self {
            Error::Validation(_) => "INVALID_ARGUMENT",
            Error::NotFound(_) => "NOT_FOUND",
            Error::QuotaExceeded(_) => "RESOURCE_EXHAUSTED",
            Error::StateTransition(_) => "FAILED_PRECONDITION",
            Error::Cancelled(_) => "CANCELLED",
            Error::Timeout(_) => "TIMEOUT",
            Error::Internal(_) | Error::Serialization(_) | Error::Io(_) => "INTERNAL",
        }
    }
}

// Convenience constructors
impl Error {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn quota_exceeded(msg: impl Into<String>) -> Self {
        Self::QuotaExceeded(msg.into())
    }

    pub fn state_transition(msg: impl Into<String>) -> Self {
        Self::StateTransition(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn cancelled(msg: impl Into<String>) -> Self {
        Self::Cancelled(msg.into())
    }

    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::Timeout(msg.into())
    }
}
