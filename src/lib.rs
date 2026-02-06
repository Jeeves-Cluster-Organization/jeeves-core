//! # Jeeves Core - Multi-Agent Orchestration Kernel
//!
//! Rust implementation of the Jeeves kernel providing:
//! - Process lifecycle management with Unix-like state transitions
//! - Resource quota enforcement (LLM calls, tokens, hops, iterations)
//! - Rate limiting with configurable windows
//! - Flow interrupts for human-in-the-loop patterns
//! - gRPC service layer for external clients
//! - Message bus for pub/sub and request/response patterns
//!
//! ## Architecture
//!
//! The kernel follows a single-actor model where the `Kernel` owns all mutable state:
//! ```text
//!                    ┌─────────────────────────────────┐
//!   gRPC requests →  │         Kernel Actor            │
//!                    │  ┌─────────┐ ┌─────────┐        │
//!                    │  │Resources│ │Lifecycle│        │
//!                    │  │ Tracker │ │ Manager │        │
//!                    │  └─────────┘ └─────────┘        │
//!                    │  ┌─────────┐ ┌─────────┐        │
//!                    │  │Interrupt│ │RateLimit│        │
//!                    │  │ Service │ │   er    │        │
//!                    │  └─────────┘ └─────────┘        │
//!                    └─────────────────────────────────┘
//! ```

// Enforce strict safety at compile time
#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![warn(rust_2018_idioms)]

// Re-export public API
pub mod commbus;
pub mod envelope;
pub mod grpc;
pub mod kernel;
pub mod proto;
pub mod types;

// Internal utilities
pub mod observability;

pub use types::{Config, Error, Result};
