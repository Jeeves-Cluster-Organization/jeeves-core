//! LLM provider abstraction — HTTP-based LLM calls.

pub mod mock;
pub mod openai;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
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

/// A streaming chunk.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub delta_content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub done: bool,
    pub usage: Option<TokenUsage>,
}

/// LLM provider trait.
#[async_trait]
pub trait LlmProvider: Send + Sync + std::fmt::Debug {
    /// Non-streaming chat completion.
    async fn chat(&self, req: &ChatRequest) -> crate::types::Result<ChatResponse>;
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
