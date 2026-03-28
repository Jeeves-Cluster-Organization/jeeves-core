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
use crate::worker::tools::ContentPart;

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

/// Message content — either plain text or multimodal content parts.
///
/// Uses `#[serde(untagged)]` so `Text("hello")` serializes as `"hello"` (bare string),
/// preserving backward compatibility with existing checkpoints and wire formats.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text content (backward-compatible path).
    Text(String),
    /// Multimodal content parts (text + images + refs).
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    /// True if the content is empty (no text, no parts).
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(s) => s.is_empty(),
            Self::Parts(parts) => parts.is_empty(),
        }
    }

    /// Approximate character length (used for token estimation: chars/4).
    pub fn len(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Parts(parts) => parts.iter().map(|p| match p {
                ContentPart::Text { text } => text.len(),
                ContentPart::Ref { ref_id, .. } => ref_id.len() + 30,
                ContentPart::Blob { data, .. } => data.len(),
            }).sum(),
        }
    }

    /// Get the text content if this is a Text variant.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Extract text content, joining Text parts or returning the full string.
    /// Non-text parts are skipped.
    pub fn as_text_lossy(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Parts(parts) => parts.iter().filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            }).collect::<Vec<_>>().join(""),
        }
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self { Self::Text(s) }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self { Self::Text(s.to_string()) }
}

/// A single message in a chat conversation (OpenAI multi-turn tool protocol).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    pub fn system(content: impl Into<MessageContent>) -> Self {
        Self { role: "system".into(), content: content.into(), tool_call_id: None, tool_calls: None }
    }

    pub fn user(content: impl Into<MessageContent>) -> Self {
        Self { role: "user".into(), content: content.into(), tool_call_id: None, tool_calls: None }
    }

    pub fn assistant(content: impl Into<MessageContent>, tool_calls: Option<Vec<ToolCall>>) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_call_id: None, tool_calls }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<MessageContent>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_content_text_serde_round_trip() {
        // Text variant serializes as bare string (backward compat)
        let content = MessageContent::Text("hello world".into());
        let json = serde_json::to_string(&content).unwrap();
        assert_eq!(json, r#""hello world""#);

        let deserialized: MessageContent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.as_text(), Some("hello world"));
    }

    #[test]
    fn message_content_parts_serde_round_trip() {
        let content = MessageContent::Parts(vec![
            ContentPart::Text { text: "description".into() },
            ContentPart::Ref { content_type: "image/jpeg".into(), ref_id: "frame_1".into() },
        ]);
        let json = serde_json::to_string(&content).unwrap();
        let deserialized: MessageContent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.len(), "description".len() + "frame_1".len() + 30);
    }

    #[test]
    fn message_content_helpers() {
        let text = MessageContent::Text("hello".into());
        assert!(!text.is_empty());
        assert_eq!(text.len(), 5);
        assert_eq!(text.as_text(), Some("hello"));
        assert_eq!(text.as_text_lossy(), "hello");

        let empty = MessageContent::Text(String::new());
        assert!(empty.is_empty());

        let parts = MessageContent::Parts(vec![
            ContentPart::Text { text: "a".into() },
            ContentPart::Ref { content_type: "image/png".into(), ref_id: "x".into() },
            ContentPart::Text { text: "b".into() },
        ]);
        assert_eq!(parts.as_text(), None);
        assert_eq!(parts.as_text_lossy(), "ab"); // non-text parts skipped
    }

    #[test]
    fn chat_message_backward_compat_serde() {
        // Old format: {"role":"user","content":"hello"}
        // MessageContent::Text("hello") serializes as "hello" (untagged)
        let msg = ChatMessage::user("hello");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""content":"hello""#));

        // Deserializing old-format JSON still works
        let old_json = r#"{"role":"user","content":"hello"}"#;
        let deserialized: ChatMessage = serde_json::from_str(old_json).unwrap();
        assert_eq!(deserialized.content.as_text(), Some("hello"));
    }
}

