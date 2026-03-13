//! # Jeeves Core - Multi-Agent Orchestration Kernel
//!
//! Single-process Rust runtime providing:
//! - Process lifecycle management with Unix-like state transitions
//! - Resource quota enforcement (LLM calls, tokens, hops, iterations)
//! - Rate limiting with configurable windows
//! - Flow interrupts for human-in-the-loop patterns
//! - Embedded agent execution with LLM HTTP calls
//! - HTTP gateway (axum) for external clients
//! - Message bus for pub/sub and request/response patterns
//!
//! ## Architecture
//!
//! Single-actor kernel behind a typed mpsc channel. Agent tasks run as
//! concurrent tokio tasks, communicating with the kernel via `KernelHandle`.
//! ```text
//!   HTTP (axum) → KernelHandle → mpsc → Kernel actor (single &mut)
//!                                           ↕
//!                                   Agent tasks (concurrent)
//!                                           ↓
//!                                   LLM calls (reqwest)
//! ```

// Enforce strict safety at compile time
#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![warn(rust_2018_idioms)]

// Re-export public API
pub mod commbus;
pub mod envelope;
pub mod kernel;
pub mod tools;
pub mod types;
pub mod worker;

// Internal utilities
pub mod observability;

pub use types::{Config, Error, Result};
