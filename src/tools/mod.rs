//! Tool infrastructure — catalog, validation, access control, health tracking.
//!
//! Rust owns tool metadata, parameter validation, access policies, prompt
//! generation, and health metrics. Python keeps async callables and SQLite
//! persistence; all typed logic lives here.

pub mod access;
pub mod catalog;
pub mod health;

pub use access::ToolAccessPolicy;
pub use catalog::{ParamDef, ParamType, ToolCatalog, ToolEntry};
pub use health::{HealthConfig, ToolHealthTracker};
