//! Tool infrastructure — runtime dispatch, access control, typed param
//! validation, and health tracking.
//!
//! Layout:
//! - [`executor`] — the [`ToolExecutor`] trait, value types ([`ToolOutput`],
//!   [`ContentPart`], …), and the [`AclToolExecutor`] / [`NoopToolExecutor`]
//!   wrappers consumers compose against.
//! - [`registry`] — [`ToolRegistry`] / [`ToolRegistryBuilder`], which dispatch
//!   through the optional policy → catalog → health chain.
//! - [`access`], [`catalog`], [`health`] — the three opt-in gates.

pub mod access;
pub mod catalog;
pub mod executor;
pub mod health;
pub mod registry;

pub use access::ToolAccessPolicy;
pub use catalog::{ParamDef, ParamType, RiskSemantic, RiskSeverity, ToolCatalog, ToolCategory, ToolEntry};
pub use executor::{
    AclToolExecutor, ConfirmationRequest, ContentPart, ContentResolver, NoopToolExecutor,
    ToolExecutor, ToolInfo, ToolOutput,
};
pub use health::{HealthConfig, HealthStatus, ToolHealthTracker};
pub use registry::{ToolRegistry, ToolRegistryBuilder};
