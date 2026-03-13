//! Mock LLM provider for testing.

use async_trait::async_trait;

use super::{ChatRequest, ChatResponse, LlmProvider, TokenUsage};
use crate::types::Result;

/// Mock provider that returns canned responses.
#[derive(Debug, Clone)]
pub struct MockLlmProvider {
    /// Fixed text content to return.
    pub response_content: String,
}

impl MockLlmProvider {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            response_content: content.into(),
        }
    }
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self::new("{}")
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse> {
        Ok(ChatResponse {
            content: Some(self.response_content.clone()),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            model: "mock".to_string(),
        })
    }
}
