//! # Jeeves Core - Multi-Agent Orchestration Kernel
//!
//! Rust library providing multi-agent orchestration:
//! - Process lifecycle management with Unix-like state transitions
//! - Resource quota enforcement (LLM calls, tokens, hops, iterations)
//! - Rate limiting with configurable windows
//! - Flow interrupts for human-in-the-loop patterns
//! - Embedded agent execution with LLM HTTP calls
//! - Message bus for pub/sub and request/response patterns
//!
//! ## Consumption modes
//!
//! - **PyO3 module** (`py-bindings` feature): `from jeeves_core import PipelineRunner`
//! - **MCP stdio** (`mcp-stdio` feature): `jeeves-kernel` binary, JSON-RPC over stdin/stdout
//!
//! ## Architecture
//!
//! Single-actor kernel behind a typed mpsc channel. Agent tasks run as
//! concurrent tokio tasks, communicating with the kernel via `KernelHandle`.
//! ```text
//!   PyO3 / MCP stdio → KernelHandle → mpsc → Kernel actor (single &mut)
//!                                                 ↕
//!                                         Agent tasks (concurrent)
//!                                                 ↓
//!                                         LLM calls (reqwest)
//! ```

// Enforce strict safety at compile time
#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![warn(rust_2018_idioms)]

// Re-export public API
pub mod commbus;
pub mod envelope;
pub mod kernel;
#[cfg(feature = "py-bindings")]
#[allow(unsafe_code, clippy::useless_conversion)]
pub mod python;
#[cfg(any(test, feature = "test-harness"))]
pub mod testing;
pub mod tools;
pub mod types;
pub mod worker;

// Internal utilities
pub(crate) mod observability;

// Re-export observability functions needed by the MCP stdio binary
pub use observability::{init_tracing, init_tracing_from_config};
#[cfg(feature = "otel")]
pub use observability::otel_tracing_layer;

pub use types::{Config, Error, Result};

/// Prelude — re-exports consumer-facing types for convenient imports.
///
/// ```ignore
/// use jeeves_core::prelude::*;
/// ```
pub mod prelude {
    pub use crate::envelope::Envelope;
    pub use crate::kernel::orchestrator_types::PipelineConfig;
    pub use crate::kernel::Kernel;
    pub use crate::types::ProcessId;
    pub use crate::worker::actor::spawn_kernel;
    pub use crate::worker::agent::AgentRegistry;
    pub use crate::worker::agent_factory::AgentFactoryBuilder;
    pub use crate::worker::handle::KernelHandle;
    pub use crate::worker::llm::{LlmProvider, PipelineEvent};
    pub use crate::worker::prompts::PromptRegistry;
    pub use crate::worker::tools::{ToolExecutor, ToolInfo, ToolRegistry, ToolRegistryBuilder};
    pub use crate::worker::{run_pipeline_streaming, run_pipeline_with_envelope, WorkerResult};
}
