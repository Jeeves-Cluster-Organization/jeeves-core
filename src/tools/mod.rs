//! Tool infrastructure — catalog, validation, access control, health tracking.
//!
//! Owns tool metadata, parameter validation, access policies, prompt
//! generation, and health metrics. Tool implementations live in the worker layer.

pub mod access;
pub mod catalog;
pub mod health;

pub use access::ToolAccessPolicy;
pub use catalog::{ParamDef, ParamType, ToolCatalog, ToolEntry};
pub use health::{HealthConfig, ToolHealthTracker};
