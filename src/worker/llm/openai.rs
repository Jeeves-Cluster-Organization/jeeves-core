//! OpenAI-compatible LLM provider (works with any /v1/chat/completions endpoint).

use async_trait::async_trait;
use futures::stream::Stream;
use serde::Deserialize;
use std::pin::Pin;

use super::{ChatRequest, ChatResponse, LlmProvider, StreamChunk, TokenUsage, ToolCall};
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

    fn build_request_body(&self, req: &ChatRequest, stream: bool) -> serde_json::Value {
        let model = req
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());

        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|m| {
                let mut msg = serde_json::json!({ "role": &m.role, "content": &m.content });
                if let Some(ref tc_id) = m.tool_call_id {
                    msg["tool_call_id"] = serde_json::json!(tc_id);
                }
                if let Some(ref tcs) = m.tool_calls {
                    let tc_vals: Vec<serde_json::Value> = tcs
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments,
                                }
                            })
                        })
                        .collect();
                    msg["tool_calls"] = serde_json::json!(tc_vals);
                }
                msg
            })
            .collect();

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
        });

        if stream {
            body["stream"] = serde_json::json!(true);
            // Request usage in stream (OpenAI extension)
            body["stream_options"] = serde_json::json!({"include_usage": true});
        }
        if let Some(temp) = req.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max) = req.max_tokens {
            body["max_tokens"] = serde_json::json!(max);
        }
        if let Some(ref tools) = req.tools {
            body["tools"] = serde_json::json!(tools);
        }

        body
    }

    fn url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }
}

// --- Non-streaming API response types ---

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

// --- Streaming API response types ---

#[derive(Debug, Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[serde(default)]
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<StreamToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCallDelta {
    #[allow(dead_code)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct StreamFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let body = self.build_request_body(req, false);

        let response = self
            .client
            .post(self.url())
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

    async fn chat_stream(
        &self,
        req: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let body = self.build_request_body(req, true);

        let response = self
            .client
            .post(self.url())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::internal(format!("LLM HTTP request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let response_text = response
                .text()
                .await
                .map_err(|e| Error::internal(format!("Failed to read LLM error response: {}", e)))?;
            let error_msg = serde_json::from_str::<ApiError>(&response_text)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| response_text);
            return Err(Error::internal(format!("LLM API error ({}): {}", status, error_msg)));
        }

        let byte_stream = response.bytes_stream();
        Ok(Box::pin(parse_sse_stream(byte_stream)))
    }
}

/// Parse an OpenAI SSE byte stream into StreamChunks.
fn parse_sse_stream(
    byte_stream: impl Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = Result<StreamChunk>> + Send {
    use futures::StreamExt;

    // Pin the byte stream so we can call .next() on it inside unfold
    let pinned: Pin<
        Box<
            dyn Stream<Item = std::result::Result<bytes::Bytes, reqwest::Error>> + Send,
        >,
    > = Box::pin(byte_stream);

    let state = (pinned, String::new());

    futures::stream::unfold(state, |(mut byte_stream, mut buffer)| async move {
        loop {
            // Try to extract a complete SSE data line from buffer
            if let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    let data = data.trim();
                    if data == "[DONE]" {
                        return None;
                    }

                    match serde_json::from_str::<StreamResponse>(data) {
                        Ok(resp) => {
                            let chunk = stream_response_to_chunk(resp);
                            return Some((Ok(chunk), (byte_stream, buffer)));
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse SSE chunk: {}", e);
                            continue;
                        }
                    }
                }
                continue;
            }

            // Need more data from the byte stream
            match byte_stream.next().await {
                Some(Ok(bytes)) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                }
                Some(Err(e)) => {
                    return Some((
                        Err(Error::internal(format!("SSE stream error: {}", e))),
                        (byte_stream, buffer),
                    ));
                }
                None => return None,
            }
        }
    })
}

fn stream_response_to_chunk(resp: StreamResponse) -> StreamChunk {
    let choice = resp.choices.into_iter().next();
    let (delta_content, tool_calls) = match choice {
        Some(c) => {
            let tcs: Vec<ToolCall> = c
                .delta
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .map(|tc| ToolCall {
                    id: tc.id.unwrap_or_default(),
                    name: tc.function.as_ref().and_then(|f| f.name.clone()).unwrap_or_default(),
                    arguments: tc.function.as_ref().and_then(|f| f.arguments.clone()).unwrap_or_default(),
                })
                .collect();
            (c.delta.content, tcs)
        }
        None => (None, vec![]),
    };

    let usage = resp.usage.map(|u| TokenUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    });

    StreamChunk {
        delta_content,
        tool_calls,
        done: false,
        usage,
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

    #[test]
    fn test_parse_stream_response() {
        let json = r#"{
            "choices": [{
                "index": 0,
                "delta": {
                    "content": "Hello"
                },
                "finish_reason": null
            }]
        }"#;
        let resp: StreamResponse = serde_json::from_str(json).unwrap();
        let chunk = stream_response_to_chunk(resp);
        assert_eq!(chunk.delta_content.as_deref(), Some("Hello"));
        assert!(chunk.tool_calls.is_empty());
    }

    #[test]
    fn test_parse_stream_tool_call_delta() {
        let json = r#"{
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_abc",
                        "function": {
                            "name": "search",
                            "arguments": "{\"q\":"
                        }
                    }]
                },
                "finish_reason": null
            }]
        }"#;
        let resp: StreamResponse = serde_json::from_str(json).unwrap();
        let chunk = stream_response_to_chunk(resp);
        assert!(chunk.delta_content.is_none());
        assert_eq!(chunk.tool_calls.len(), 1);
        assert_eq!(chunk.tool_calls[0].name, "search");
        assert_eq!(chunk.tool_calls[0].arguments, "{\"q\":");
    }

    #[test]
    fn test_build_request_body_with_tool_messages() {
        use super::super::ChatMessage;

        let provider = OpenAiProvider::new("test-key", "gpt-4o");
        let req = ChatRequest {
            messages: vec![
                ChatMessage::system("You are helpful."),
                ChatMessage::user("Search for weather."),
                ChatMessage::assistant(
                    "",
                    Some(vec![ToolCall {
                        id: "call_1".into(),
                        name: "search".into(),
                        arguments: r#"{"q":"weather"}"#.into(),
                    }]),
                ),
                ChatMessage::tool_result("call_1", r#"{"temp": 72}"#),
            ],
            temperature: None,
            max_tokens: None,
            model: None,
            tools: None,
        };

        let body = provider.build_request_body(&req, false);
        let messages = body["messages"].as_array().unwrap();

        // Assistant message should have tool_calls
        let asst = &messages[2];
        assert!(asst.get("tool_calls").is_some());
        assert_eq!(asst["tool_calls"][0]["function"]["name"], "search");

        // Tool result should have tool_call_id
        let tool = &messages[3];
        assert_eq!(tool["role"], "tool");
        assert_eq!(tool["tool_call_id"], "call_1");
    }
}
