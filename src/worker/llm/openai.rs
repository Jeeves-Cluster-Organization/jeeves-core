//! OpenAI-compatible LLM provider (works with any /v1/chat/completions endpoint).

use async_trait::async_trait;
use serde::Deserialize;

use super::{ChatRequest, ChatResponse, LlmProvider, TokenUsage, ToolCall};
use crate::types::{Error, Result};

/// OpenAI-compatible HTTP provider.
#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    default_model: String,
}

impl OpenAiProvider {
    pub fn new(api_key: impl Into<String>, default_model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.openai.com".to_string(),
            api_key: api_key.into(),
            default_model: default_model.into(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

// --- API response types ---

#[derive(Debug, Deserialize)]
struct ApiResponse {
    choices: Vec<ApiChoice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct ApiChoice {
    message: ApiMessage,
}

#[derive(Debug, Deserialize)]
struct ApiMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ApiToolCall {
    id: String,
    function: ApiFunction,
}

#[derive(Debug, Deserialize)]
struct ApiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    error: ApiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    message: String,
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let model = req
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());

        let mut body = serde_json::json!({
            "model": model,
            "messages": req.messages.iter().map(|m| {
                serde_json::json!({ "role": &m.role, "content": &m.content })
            }).collect::<Vec<_>>(),
        });

        if let Some(temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max) = req.max_tokens {
            body["max_tokens"] = serde_json::json!(max);
        }
        if let Some(ref tools) = req.tools {
            body["tools"] = serde_json::json!(tools);
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::internal(format!("LLM HTTP request failed: {}", e)))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .map_err(|e| Error::internal(format!("Failed to read LLM response: {}", e)))?;

        if !status.is_success() {
            let error_msg = serde_json::from_str::<ApiError>(&response_text)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| response_text.clone());
            return Err(Error::internal(format!(
                "LLM API error ({}): {}",
                status, error_msg
            )));
        }

        let api_resp: ApiResponse = serde_json::from_str(&response_text)
            .map_err(|e| Error::internal(format!("Failed to parse LLM response: {}", e)))?;

        let choice = api_resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| Error::internal("LLM returned no choices"))?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| ToolCall {
                id: tc.id,
                name: tc.function.name,
                arguments: tc.function.arguments,
            })
            .collect();

        let usage = api_resp
            .usage
            .map(|u| TokenUsage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            })
            .unwrap_or_default();

        Ok(ChatResponse {
            content: choice.message.content,
            tool_calls,
            usage,
            model: api_resp.model,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_api_response() {
        let json = r#"{
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content.as_deref(), Some("Hello!"));
        assert_eq!(resp.usage.unwrap().total_tokens, 15);
    }

    #[test]
    fn test_parse_tool_call_response() {
        let json = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "{\"query\": \"weather\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 10,
                "total_tokens": 30
            }
        }"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        let tcs = resp.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].function.name, "search");
    }
}
