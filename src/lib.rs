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
pub mod observability;

pub use types::{Config, Error, Result};
