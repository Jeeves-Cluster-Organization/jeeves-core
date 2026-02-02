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
    /// Validation errors (map to gRPC INVALID_ARGUMENT).
    #[error("validation error: {0}")]
    Validation(String),

    /// Resource not found (map to gRPC NOT_FOUND).
    #[error("not found: {0}")]
    NotFound(String),

    /// Quota or resource exhaustion (map to gRPC RESOURCE_EXHAUSTED).
    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),

    /// Invalid state transition (map to gRPC FAILED_PRECONDITION).
    #[error("state transition error: {0}")]
    StateTransition(String),

    /// Internal errors (map to gRPC INTERNAL).
    #[error("internal error: {0}")]
    Internal(String),

    /// Cancellation/timeout (map to gRPC CANCELLED or DEADLINE_EXCEEDED).
    #[error("operation cancelled: {0}")]
    Cancelled(String),

    /// Timeout (map to gRPC DEADLINE_EXCEEDED).
    #[error("timeout: {0}")]
    Timeout(String),

    /// Serialization/deserialization errors.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// gRPC transport errors (boxed to reduce Result size).
    #[error("grpc error: {0}")]
    Grpc(#[from] Box<tonic::Status>),

    /// I/O errors.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Convert to gRPC status code.
    pub fn to_grpc_status(&self) -> tonic::Status {
        match self {
            Error::Validation(msg) => {
                tonic::Status::invalid_argument(msg)
            }
            Error::NotFound(msg) => {
                tonic::Status::not_found(msg)
            }
            Error::QuotaExceeded(msg) => {
                tonic::Status::resource_exhausted(msg)
            }
            Error::StateTransition(msg) => {
                tonic::Status::failed_precondition(msg)
            }
            Error::Cancelled(msg) => {
                tonic::Status::cancelled(msg)
            }
            Error::Timeout(msg) => {
                tonic::Status::deadline_exceeded(msg)
            }
            Error::Internal(msg) => {
                tonic::Status::internal(msg)
            }
            Error::Serialization(e) => {
                tonic::Status::internal(format!("serialization error: {}", e))
            }
            Error::Grpc(status) => (**status).clone(),
            Error::Io(e) => {
                tonic::Status::internal(format!("io error: {}", e))
            }
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

// Implement From<Error> for Status to enable ? operator in gRPC handlers
impl From<Error> for tonic::Status {
    fn from(err: Error) -> Self {
        err.to_grpc_status()
    }
}
