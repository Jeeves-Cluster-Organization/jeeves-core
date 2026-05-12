//! Core types for the Jeeves kernel.
//!
//! This module provides foundational types used throughout the system:
//! - **IDs**: Strongly-typed identifiers (RunId, EnvelopeId, etc.)
//! - **Errors**: Application error types with thiserror derives
//! - **Config**: Configuration structures for kernel, pipeline, and resources

pub mod config;
mod errors;
mod ids;

pub use config::{AgentDefinition, Config, ObservabilityConfig};
pub use errors::{Error, Result};
pub use ids::{
    AgentName, EnvelopeId, InterruptId, OutputKey, PromptKey, RequestId, RoutingFnName, RunId,
    SessionId, StageName, ToolName, UserId,
};
