//! Mock LLM providers for testing.

use async_trait::async_trait;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Mutex;

use super::{ChatRequest, ChatResponse, LlmProvider, StreamChunk, TokenUsage};
use crate::types::Result;

/// Mock provider that returns a fixed response.
#[derive(Debug, Clone)]
pub struct MockLlmProvider {
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

    async fn chat_stream(
        &self,
        req: &ChatRequest,
    ) -> Result<Pin<Box<dyn futures::stream::Stream<Item = Result<StreamChunk>> + Send>>> {
        let resp = self.chat(req).await?;
        let chunk = StreamChunk {
            delta_content: resp.content,
            tool_calls: resp.tool_calls,
            done: true,
            usage: Some(resp.usage),
        };
        Ok(Box::pin(futures::stream::once(async { Ok(chunk) })))
    }
}

/// Mock provider that returns responses in sequence. Each call to chat()/chat_stream()
/// pops the next response from the queue. Useful for testing multi-turn tool loops.
#[derive(Debug)]
pub struct SequentialMockLlmProvider {
    responses: Mutex<VecDeque<ChatResponse>>,
}

impl SequentialMockLlmProvider {
    pub fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
        }
    }

    fn next_response(&self) -> Result<ChatResponse> {
        self.responses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .ok_or_else(|| crate::types::Error::internal("SequentialMockLlmProvider: no more responses"))
    }
}

#[async_trait]
impl LlmProvider for SequentialMockLlmProvider {
    async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse> {
        self.next_response()
    }

    async fn chat_stream(
        &self,
        _req: &ChatRequest,
    ) -> Result<Pin<Box<dyn futures::stream::Stream<Item = Result<StreamChunk>> + Send>>> {
        let resp = self.next_response()?;
        let chunk = StreamChunk {
            delta_content: resp.content,
            tool_calls: resp.tool_calls,
            done: true,
            usage: Some(resp.usage),
        };
        Ok(Box::pin(futures::stream::once(async { Ok(chunk) })))
    }
}
