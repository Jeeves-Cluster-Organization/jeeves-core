//! Core types for the Jeeves kernel.
//!
//! This module provides foundational types used throughout the system:
//! - **IDs**: Strongly-typed identifiers (ProcessId, EnvelopeId, etc.)
//! - **Errors**: Application error types with thiserror derives
//! - **Config**: Configuration structures for kernel, pipeline, and resources

mod config;
mod errors;
mod ids;

pub use config::{Config, IpcConfig};
pub use errors::{Error, Result};
pub use ids::{EnvelopeId, ProcessId, RequestId, SessionId, UserId};
