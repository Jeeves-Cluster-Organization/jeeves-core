//! LLM provider abstraction.

pub mod genai;

use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;

use crate::error::Result;
use crate::tools::{ContentPart, ToolCall};

pub use genai::GenaiProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(s) => s.is_empty(),
            Self::Parts(p) => p.is_empty(),
        }
    }

    pub fn as_text_lossy(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Parts(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}
impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        Self::Text(s.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRef>>,
}

/// Sidecar tool-call record (mirrors tools::ToolCall but serde-friendly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRef {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

impl From<&ToolCall> for ToolCallRef {
    fn from(tc: &ToolCall) -> Self {
        Self {
            id: tc.id.clone(),
            name: tc.name.clone(),
            arguments: tc.arguments.clone(),
        }
    }
}

impl ChatMessage {
    pub fn system(content: impl Into<MessageContent>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }
    pub fn user(content: impl Into<MessageContent>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }
    pub fn assistant(
        content: impl Into<MessageContent>,
        tool_calls: Option<Vec<ToolCallRef>>,
    ) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls,
        }
    }
    pub fn tool_result(id: impl Into<String>, content: impl Into<MessageContent>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_call_id: Some(id.into()),
            tool_calls: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i32>,
    pub model: Option<String>,
    pub tools: Option<Arc<Vec<serde_json::Value>>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub delta_content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
}

#[async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse>;

    async fn chat_stream(
        &self,
        req: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>>;
}
