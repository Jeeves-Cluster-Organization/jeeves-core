//! Jeeves Core: a small in-process workflow runtime for deterministic and
//! LLM-backed actions.

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
        ModelStreamEvent, Prompt, ToolDecision,
    };
    pub use crate::tools::{
        ApprovalPrompt, ApprovalRequest, ApprovalResponse, CircuitBreakerConfig,
        CircuitBreakerStatus, CircuitBreakerTool, CircuitFailurePolicy, DenialBehavior,
        ReplaySafety, Tool, ToolContext, ToolDefinition, ToolSpec,
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
