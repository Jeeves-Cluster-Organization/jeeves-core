//! # agent-core
//!
//! Minimal policy-free agent runtime. Two harnesses consume it:
//!
//! - **harness-pi** — pi-style coding-agent CLI; no hooks installed by default.
//! - **harness-jeeves** — production shape with confirmation hook + Python bindings.
//!
//! See [`Agent`] for the entry point, and [`hooks`] for the policy seams.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod agent;
pub mod error;
pub mod events;
pub mod hooks;
pub mod llm;
pub mod observability;
pub mod session;
pub mod state;
pub mod tools;

pub use agent::{Agent, AgentBuilder};
pub use error::{Error, Result};
pub use events::{Event, EventReceiver, EventSender};
pub use hooks::{AfterToolCall, BeforeToolCall, HookDecision, TransformContext};
pub use llm::{ChatMessage, ChatRequest, ChatResponse, GenaiProvider, LlmProvider};
pub use session::{Entry, Session};
pub use state::{AgentState, Budget};
pub use tools::{ContentPart, ContentResolver, DynTool, Tool, ToolCall, ToolOutput};
