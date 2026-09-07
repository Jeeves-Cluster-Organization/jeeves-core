//! Jeeves Core: a small in-process workflow runtime for deterministic and
//! LLM-backed actions.
//!
//! Workflows are ordinary Rust values built from validated stages. Each run is
//! owned by one Tokio task and a [`RunHandle`]; every attempt is retained in
//! ordered history, and each run produces exactly one terminal outcome.
//!
//! # Highlights
//!
//! - Explicit stage kinds: deterministic, LLM, direct tool, or route-only.
//! - Synchronous, fallible, read-only routing between stages.
//! - Hard per-run bounds (stage executions, model calls, tool calls,
//!   deadline) checked before resources are consumed.
//! - Streaming model output with structured-output validation and retry.
//! - In-place human approval that pauses and resumes the live tool loop.
//! - Panic isolation around actions, routing, reducers, and whole runs.
//!
//! # Quick start
//!
//! A deterministic pipeline whose second stage doubles the first stage's
//! output:
//!
//! ```
//! use jeeves_core::prelude::*;
//! use serde_json::json;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let workflow = Workflow::builder("double")
//!     .stage(
//!         Stage::deterministic_fn("read", |run: &RunView<'_>| {
//!             let value = run.input["value"]
//!                 .as_i64()
//!                 .ok_or_else(|| Error::invalid_input("value must be an integer"))?;
//!             Ok(json!(value))
//!         })
//!         .next("double"),
//!     )
//!     .stage(Stage::deterministic_fn("double", |run: &RunView<'_>| {
//!         let value = run
//!             .latest_output("read")
//!             .and_then(serde_json::Value::as_i64)
//!             .ok_or_else(|| Error::internal("read output missing"))?;
//!         Ok(json!(value * 2))
//!     }))
//!     .build()?;
//!
//! let engine = Engine::builder().workflow(workflow)?.build()?;
//! let outcome = engine.run("double", json!({"value": 21})).await?;
//!
//! assert_eq!(outcome.result().latest_outputs["double"], json!(42));
//! # Ok(())
//! # }
//! ```
//!
//! The first declared stage is the entry point. A stage completes the workflow
//! unless [`Stage::next`](crate::workflow::Stage::next) or
//! [`Stage::route`](crate::workflow::Stage::route) selects another stage;
//! [`Stage::on_error`](crate::workflow::Stage::on_error) provides recovery
//! after configured retries are exhausted.
//!
//! # LLM stages
//!
//! Model calls stream through a single provider trait. The offline mock below
//! streams two deltas that consumers observe as [`RunEvent::TextDelta`] while
//! the final text becomes the stage output:
//!
//! ```
//! use jeeves_core::prelude::*;
//! use std::sync::Arc;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//! let llm = Arc::new(MockLlmProvider::new([vec![
//!     ModelStreamEvent::Text("Hello".into()),
//!     ModelStreamEvent::Text(", world!".into()),
//! ]]));
//! let workflow = Workflow::builder("greet")
//!     .stage(Stage::llm(
//!         "speak",
//!         LlmAction::text(Prompt::text("Greet warmly.")),
//!     ))
//!     .build()?;
//! let engine = Engine::builder().llm(llm).workflow(workflow)?.build()?;
//!
//! let outcome = engine.run("greet", "input").await?;
//! assert_eq!(outcome.result().latest_outputs["speak"], "Hello, world!");
//! # Ok(())
//! # }
//! ```
//!
//! Structured stages buffer privately, parse JSON, and pass the value through
//! a consumer-supplied validator before success; invalid output is a normal
//! stage failure that workflow retries can cover.
//!
//! # Constraints by design
//!
//! - Runs, history, events, and approvals are memory-only and process-local.
//! - Dropping the final handle cancels the run; detached runs do not exist.
//! - Event receivers are single-consumer; drain them, discard them, or use
//!   [`Engine::run`](crate::engine::Engine::run).
//! - Response schemas are provider hints only; the supplied validator is the
//!   authoritative compliance check.
//!
//! See the repository README for ecosystem positioning, and `tests/runner.rs`
//! for end-to-end behavior coverage including tools, approvals, cancellation,
//! and limits.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![warn(rust_2018_idioms)]

pub mod engine;
pub mod events;
pub mod llm;
pub mod tools;
pub mod types;
pub mod workflow;

pub use engine::{Engine, EngineBuilder, RunHandle};
pub use events::RunEvent;
pub use types::{Error, ErrorKind, Result, RunId, RunInput, RunOutcome, RunResult};

/// Common consumer imports.
pub mod prelude {
    pub use crate::engine::{Engine, EngineBuilder, RunHandle};
    pub use crate::events::{RoutingReason, RunEvent};
    pub use crate::llm::genai::GenaiProvider;
    pub use crate::llm::{
        LlmAction, LlmLoopHook, LlmOutput, LlmProvider, MockLlmProvider, ModelRequest,
        ModelStopReason, ModelStreamEvent, Prompt, ToolDecision,
    };
    pub use crate::tools::{
        ApprovalPrompt, ApprovalRequest, ApprovalResponse, DenialBehavior, ReplaySafety, Tool,
        ToolContext, ToolDefinition, ToolSpec,
    };
    pub use crate::types::{
        Error, ErrorKind, LimitKind, Result, RunId, RunInput, RunOutcome, RunResult, RunView,
        StageAttempt, StageFailure, StageFailurePhase, StageRecord, Usage,
    };
    pub use crate::workflow::{
        DeterministicAction, RetryOn, RetryPolicy, Route, RunLimits, Stage, StateReducer,
        ToolAction, ToolArguments, Workflow, WorkflowBuilder,
    };
}
