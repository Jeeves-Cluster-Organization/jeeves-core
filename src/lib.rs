//! # Jeeves Core - Multi-Agent Orchestration Kernel
//!
//! Rust library for multi-agent orchestration: pipeline routing, resource
//! quotas, embedded agent execution, and tool confirmation gating.
//!
//! ## Architecture
//!
//! Single-actor kernel behind a typed mpsc channel. Agent tasks run as
//! concurrent tokio tasks, communicating with the kernel via `KernelHandle`.
//! ```text
//!   Consumer → KernelHandle → mpsc → Kernel actor (single &mut)
//!                                          ↕
//!                                  Agent tasks (concurrent)
//!                                          ↓
//!                                  LLM calls (reqwest)
//! ```

// Enforce strict safety at compile time
#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![warn(rust_2018_idioms)]

pub mod agent;
pub mod kernel;
pub mod run;
#[cfg(any(test, feature = "test-harness"))]
pub mod testing;
pub mod tools;
pub mod types;
pub mod workflow;

// Internal utilities
pub(crate) mod observability;

pub use observability::{init_tracing, init_tracing_from_config};
#[cfg(feature = "otel")]
pub use observability::otel_tracing_layer;

pub use types::{Config, Error, Result};

/// Prelude — re-exports consumer-facing types for convenient imports.
///
/// Usage: `use jeeves_core::prelude::*;`
pub mod prelude {
    pub use crate::agent::factory::AgentFactoryBuilder;
    pub use crate::agent::hooks::{DynHook, HookDecision, LlmAgentHook};
    pub use crate::agent::llm::{LlmProvider, MessageContent, RunEvent};
    pub use crate::agent::prompts::PromptRegistry;
    pub use crate::agent::AgentRegistry;
    pub use crate::run::Run;
    pub use crate::kernel::actor::spawn;
    pub use crate::kernel::handle::KernelHandle;
    pub use crate::workflow::Workflow;
    pub use crate::kernel::routing::{RoutingContext, RoutingFn, RoutingResult};
    pub use crate::kernel::runner::{run, run_streaming, WorkerResult};
    pub use crate::kernel::Kernel;
    pub use crate::tools::{
        ConfirmationRequest, ContentPart, ContentResolver, ToolExecutor, ToolInfo, ToolOutput,
        ToolRegistry, ToolRegistryBuilder,
    };
    pub use crate::types::RunId;
}
