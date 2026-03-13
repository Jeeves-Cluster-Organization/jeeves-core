//! LLM provider abstraction — HTTP-based LLM calls + streaming.

pub mod mock;
pub mod openai;

use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio::sync::mpsc;

use crate::types::Result;

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
    pub arguments: String,
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
    pub tools: Option<Vec<serde_json::Value>>,
}

/// Token usage from the API response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
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
    pub done: bool,
    pub usage: Option<TokenUsage>,
}

/// Events emitted during pipeline execution for SSE streaming.
///
/// `stage` is `Some(name)` when the event originates inside a known pipeline stage,
/// `None` for events that are topology-independent (Done, InterruptPending) or
/// emitted outside a stage boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PipelineEvent {
    StageStarted { stage: String },
    Delta {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage: Option<String>,
    },
    ToolCallStart {
        id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage: Option<String>,
    },
    ToolResult {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage: Option<String>,
    },
    StageCompleted { stage: String },
    Done {
        process_id: String,
        terminated: bool,
        terminal_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        outputs: Option<serde_json::Value>,
    },
    InterruptPending {
        process_id: String,
        interrupt_id: String,
        kind: String,
        question: Option<String>,
        message: Option<String>,
    },
    Error {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage: Option<String>,
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
pub async fn collect_stream(
    stream: Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>,
    event_tx: Option<&mpsc::Sender<PipelineEvent>>,
    stage: Option<&str>,
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
/// OpenAI streams tool calls incrementally: first chunk has id+name, subsequent chunks
/// append to arguments. Deltas are identified by index in the tool_calls array.
fn merge_tool_call_deltas(accumulated: &mut Vec<ToolCall>, deltas: &[ToolCall]) {
    for delta in deltas {
        // Use id as the lookup key — if we already have this id, append arguments
        if let Some(existing) = accumulated.iter_mut().find(|tc| tc.id == delta.id && !delta.id.is_empty()) {
            existing.arguments.push_str(&delta.arguments);
            if existing.name.is_empty() && !delta.name.is_empty() {
                existing.name.clone_from(&delta.name);
            }
        } else {
            accumulated.push(delta.clone());
        }
    }
}

/// Resolve a model role (e.g. "fast", "reasoning") to a concrete model name.
pub fn resolve_model(
    model_role: Option<&str>,
    default_model: &str,
) -> String {
    match model_role {
        Some("fast") => "gpt-4o-mini".to_string(),
        Some("reasoning") => "o3-mini".to_string(),
        Some(explicit) => explicit.to_string(),
        None => default_model.to_string(),
    }
}
