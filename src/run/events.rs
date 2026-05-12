//! Streaming events emitted while a pipeline runs.
//!
//! Consumers drain a `mpsc::Receiver<RunEvent>` from
//! [`run_streaming`](crate::kernel::runner::run_streaming).
//! Events are pipeline-level (not LLM-specific) so they live with the run
//! that frames a pipeline execution.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::kernel::protocol::ToolCallResult;

/// Per-stage execution metrics attached to `StageCompleted` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageMetrics {
    pub duration_ms: i64,
    pub llm_calls: i32,
    pub tool_calls: i32,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub tool_results: Vec<ToolCallResult>,
    pub success: bool,
}

/// Aggregate metrics across all stages, attached to `Done` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateMetrics {
    pub total_duration_ms: i64,
    pub total_llm_calls: i32,
    pub total_tool_calls: i32,
    pub total_tokens_in: i64,
    pub total_tokens_out: i64,
    pub stages_executed: Vec<String>,
}

/// Events emitted during pipeline execution.
///
/// Every event carries a `pipeline` field identifying the originating pipeline.
/// `stage` is `Some(name)` when the event originates inside a known pipeline
/// stage, `None` for events that are topology-independent (`Done`,
/// `InterruptPending`) or emitted outside a stage boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RunEvent {
    StageStarted {
        stage: String,
        pipeline: Arc<str>,
    },
    Delta {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage: Option<String>,
        pipeline: Arc<str>,
    },
    ToolCallStart {
        id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage: Option<String>,
        pipeline: Arc<str>,
    },
    ToolResult {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage: Option<String>,
        pipeline: Arc<str>,
    },
    StageCompleted {
        stage: String,
        pipeline: Arc<str>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metrics: Option<StageMetrics>,
    },
    Done {
        run_id: String,
        terminated: bool,
        terminal_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        outputs: Option<serde_json::Value>,
        pipeline: Arc<str>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aggregate_metrics: Option<AggregateMetrics>,
    },
    InterruptPending {
        run_id: String,
        interrupt_id: String,
        kind: String,
        question: Option<String>,
        message: Option<String>,
        pipeline: Arc<str>,
    },
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage: Option<String>,
        pipeline: Arc<str>,
    },
    RoutingDecision {
        from_stage: String,
        to_stage: Option<String>,
        reason: crate::kernel::routing::RoutingReason,
        pipeline: Arc<str>,
    },
}

impl RunEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::StageStarted { .. } => "stage_started",
            Self::Delta { .. } => "delta",
            Self::ToolCallStart { .. } => "tool_call_start",
            Self::ToolResult { .. } => "tool_result",
            Self::StageCompleted { .. } => "stage_completed",
            Self::Done { .. } => "done",
            Self::InterruptPending { .. } => "interrupt_pending",
            Self::Error { .. } => "error",
            Self::RoutingDecision { .. } => "routing_decision",
        }
    }
}
