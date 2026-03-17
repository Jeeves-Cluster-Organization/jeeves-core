//! LLM provider abstraction — multi-provider calls + streaming via genai.

pub mod genai_provider;
pub mod mock;

use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::kernel::orchestrator_types::ToolCallResult;
use crate::types::Result;

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

/// A single message in a chat conversation (OpenAI multi-turn tool protocol).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into(), tool_call_id: None, tool_calls: None }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into(), tool_call_id: None, tool_calls: None }
    }

    pub fn assistant(content: impl Into<String>, tool_calls: Option<Vec<ToolCall>>) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_call_id: None, tool_calls }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: "tool".into(), content: content.into(), tool_call_id: Some(tool_call_id.into()), tool_calls: None }
    }
}

/// Tool call returned by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Request to the LLM.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Arc<Vec<serde_json::Value>>>,
}

/// Token usage from the API response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
}

/// Response from the LLM.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub model: String,
}

/// A streaming chunk from the LLM.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub delta_content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
}

/// Events emitted during pipeline execution for SSE streaming.
///
/// Every event carries a `pipeline` field identifying the originating pipeline.
/// For composed pipelines, child events use dotted notation: `"parent.child"`.
///
/// `stage` is `Some(name)` when the event originates inside a known pipeline stage,
/// `None` for events that are topology-independent (Done, InterruptPending) or
/// emitted outside a stage boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum PipelineEvent {
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
        process_id: String,
        terminated: bool,
        terminal_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        outputs: Option<serde_json::Value>,
        pipeline: Arc<str>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aggregate_metrics: Option<AggregateMetrics>,
    },
    InterruptPending {
        process_id: String,
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

impl PipelineEvent {
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

/// LLM provider trait — non-streaming and streaming.
#[async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {
    /// Non-streaming chat completion.
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse>;

    /// Streaming chat completion. Returns a stream of chunks.
    async fn chat_stream(
        &self,
        req: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>>;
}

/// Collect a StreamChunk stream into a ChatResponse, optionally forwarding content deltas
/// as PipelineEvent::Delta through the event channel.
///
/// `stage` tags every emitted Delta with the originating pipeline stage.
/// `pipeline` identifies which pipeline produced the event.
pub async fn collect_stream(
    stream: Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>,
    event_tx: Option<&mpsc::Sender<PipelineEvent>>,
    stage: Option<&str>,
    pipeline: Arc<str>,
) -> Result<ChatResponse> {
    let mut content = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut usage = TokenUsage::default();

    futures::pin_mut!(stream);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(ref delta) = chunk.delta_content {
            content.push_str(delta);
            if let Some(tx) = event_tx {
                let _ = tx
                    .send(PipelineEvent::Delta {
                        content: delta.clone(),
                        stage: stage.map(String::from),
                        pipeline: pipeline.clone(),
                    })
                    .await;
            }
        }
        merge_tool_call_deltas(&mut tool_calls, &chunk.tool_calls);
        if let Some(u) = chunk.usage {
            usage = u;
        }
    }

    Ok(ChatResponse {
        content: if content.is_empty() { None } else { Some(content) },
        tool_calls,
        usage,
        model: String::new(),
    })
}

/// Merge streaming tool call deltas into accumulated tool calls.
/// With genai, tool calls arrive complete (genai accumulates internally).
/// This handles both complete tool calls and legacy delta merging.
fn merge_tool_call_deltas(accumulated: &mut Vec<ToolCall>, deltas: &[ToolCall]) {
    for delta in deltas {
        if let Some(existing) = accumulated.iter_mut().find(|tc| tc.id == delta.id && !delta.id.is_empty()) {
            // Merge: concatenate string arguments, replace object arguments
            match (&existing.arguments, &delta.arguments) {
                (serde_json::Value::String(a), serde_json::Value::String(b)) => {
                    existing.arguments = serde_json::Value::String(format!("{a}{b}"));
                }
                _ => {
                    existing.arguments = delta.arguments.clone();
                }
            }
            if existing.name.is_empty() && !delta.name.is_empty() {
                existing.name.clone_from(&delta.name);
            }
        } else {
            accumulated.push(delta.clone());
        }
    }
}

