//! LLM-callable tool model: specification, invocation context, approval, and
//! denial policy.

use async_trait::async_trait;
use serde_json::{Map, Value};
use std::fmt;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::types::{Error, Result, RunId, StageAttempt};

/// Model-visible metadata for one tool.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: Arc<str>,
    pub description: Arc<str>,
    pub parameters: Value,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<Arc<str>>,
        description: impl Into<Arc<str>>,
        parameters: Value,
    ) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(Error::configuration("tool name cannot be empty"));
        }
        Ok(Self {
            name,
            description: description.into(),
            parameters,
        })
    }
}

/// Human approval requested before a tool executes.
#[derive(Debug, Clone)]
pub struct ApprovalPrompt {
    pub message: Arc<str>,
    pub data: Option<Value>,
}

impl ApprovalPrompt {
    pub fn new(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Trusted invocation data supplied by the runtime, never by the model.
#[derive(Debug, Clone)]
pub struct ToolContext<'a> {
    pub run_id: &'a RunId,
    pub workflow: &'a str,
    pub attempt: &'a StageAttempt,
    pub call_id: &'a str,
    pub metadata: &'a Map<String, Value>,
    pub cancellation: CancellationToken,
}

/// A single callable tool implementation.
#[async_trait]
pub trait Tool: Send + Sync {
    async fn call(&self, context: &ToolContext<'_>, arguments: Value) -> Result<Value>;

    /// Return an approval prompt for this invocation, or `None` to execute it.
    async fn approval(
        &self,
        _context: &ToolContext<'_>,
        _arguments: &Value,
    ) -> Result<Option<ApprovalPrompt>> {
        Ok(None)
    }
}

/// Whether a tool may be invoked again after an indeterminate stage failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaySafety {
    Unsafe,
    Idempotent,
}

/// One authoritative tool record: specification and matching handler.
#[derive(Clone)]
pub struct ToolDefinition {
    pub spec: ToolSpec,
    pub handler: Arc<dyn Tool>,
    pub replay_safety: ReplaySafety,
}

impl ToolDefinition {
    pub fn new(spec: ToolSpec, handler: Arc<dyn Tool>) -> Self {
        Self {
            spec,
            handler,
            replay_safety: ReplaySafety::Unsafe,
        }
    }

    pub fn with_replay_safety(mut self, replay_safety: ReplaySafety) -> Self {
        self.replay_safety = replay_safety;
        self
    }

    pub fn idempotent(self) -> Self {
        self.with_replay_safety(ReplaySafety::Idempotent)
    }
}

impl fmt::Debug for ToolDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolDefinition")
            .field("spec", &self.spec)
            .field("replay_safety", &self.replay_safety)
            .finish_non_exhaustive()
    }
}

/// Consumer response to an in-memory approval request.
#[derive(Debug, Clone)]
pub struct ApprovalResponse {
    pub request_id: Arc<str>,
    pub approved: bool,
}

impl ApprovalResponse {
    pub fn approve(request_id: impl Into<Arc<str>>) -> Self {
        Self {
            request_id: request_id.into(),
            approved: true,
        }
    }

    pub fn deny(request_id: impl Into<Arc<str>>) -> Self {
        Self {
            request_id: request_id.into(),
            approved: false,
        }
    }
}

/// Approval request emitted by a running workflow.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub request_id: Arc<str>,
    pub attempt: StageAttempt,
    pub call_id: Arc<str>,
    pub tool: Arc<str>,
    pub arguments: Value,
    pub prompt: ApprovalPrompt,
}

/// Behavior applied when a consumer denies an invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialBehavior {
    Continue,
    FailStage,
    CancelRun,
}
