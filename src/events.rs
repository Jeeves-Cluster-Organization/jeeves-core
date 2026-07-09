//! Typed in-process run events.

use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::tools::ApprovalRequest;
use crate::types::{Error, RunOutcome, StageAttempt, StageRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingReason {
    Success,
    Dynamic,
    ErrorRecovery,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RunEvent {
    StageStarted {
        attempt: StageAttempt,
    },
    TextDelta {
        attempt: StageAttempt,
        model_call: u32,
        content: String,
    },
    ToolCallStarted {
        attempt: StageAttempt,
        call_id: Arc<str>,
        tool: Arc<str>,
        arguments: Value,
    },
    ToolCallFinished {
        attempt: StageAttempt,
        call_id: Arc<str>,
        tool: Arc<str>,
        result: std::result::Result<Value, Error>,
        duration: Duration,
    },
    ToolCallAborted {
        attempt: StageAttempt,
        call_id: Arc<str>,
        tool: Arc<str>,
        duration: Duration,
    },
    ApprovalRequested(ApprovalRequest),
    StageAttemptFinished(Arc<StageRecord>),
    Routed {
        from: Arc<str>,
        to: Option<Arc<str>>,
        reason: RoutingReason,
    },
    Finished(Arc<RunOutcome>),
}
