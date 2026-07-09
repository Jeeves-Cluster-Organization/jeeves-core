//! Typed in-process run events.

use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::tools::ApprovalRequest;
use crate::types::{Error, RunOutcome, StageRecord};

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
        stage: Arc<str>,
        visit: u32,
        attempt: u32,
    },
    TextDelta {
        stage: Arc<str>,
        content: String,
    },
    ToolCallStarted {
        stage: Arc<str>,
        call_id: Arc<str>,
        tool: Arc<str>,
        arguments: Value,
    },
    ToolCallFinished {
        stage: Arc<str>,
        call_id: Arc<str>,
        tool: Arc<str>,
        result: std::result::Result<Value, Error>,
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
