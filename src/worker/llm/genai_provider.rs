//! GenAI-backed LLM provider — 8+ providers via the genai crate.
//!
//! Supports OpenAI, Anthropic, Gemini, Ollama, Groq, DeepSeek, Cohere, xAI
//! through a single interface. The model name prefix determines the provider
//! (e.g. `gpt-4o` → OpenAI, `claude-3-5-sonnet` → Anthropic).
//! API keys are read from environment variables automatically.

use async_trait::async_trait;
use futures::stream::Stream;
use futures::StreamExt;
use genai::chat::{
    ChatMessage as GenaiMessage, ChatOptions, ChatRequest as GenaiRequest,
    ChatStreamEvent, Tool as GenaiTool,
};
use std::collections::HashMap;
use std::pin::Pin;

use super::{ChatRequest, ChatResponse, LlmProvider, StreamChunk, TokenUsage, ToolCall};
use crate::types::{Error, Result};

/// Max retry attempts for transient LLM errors (429, 5xx, connection).
const MAX_RETRIES: u32 = 3;
/// Initial backoff delay in milliseconds (doubles each retry).
const INITIAL_BACKOFF_MS: u64 = 500;

/// Returns true for errors worth retrying (rate limits, server errors, network).
fn is_transient(err: &genai::Error) -> bool {
    match err {
        genai::Error::HttpError { status, .. } => {
            status.as_u16() == 429
                || status.is_server_error() // 500, 502, 503, 504
        }
        // Network/connection failures
        genai::Error::WebAdapterCall { .. }
        | genai::Error::WebModelCall { .. }
        | genai::Error::WebStream { .. } => true,
        _ => false,
    }
}

/// Multi-provider LLM adapter via [genai](https://crates.io/crates/genai).
///
/// API keys are auto-discovered from environment variables based on model prefix:
/// `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, etc.
///
/// Model roles (e.g. `"fast"`, `"reasoning"`) are resolved via `MODEL_*` env vars:
/// `MODEL_FAST=gpt-4o-mini`, `MODEL_REASONING=claude-3-5-sonnet`, etc.
#[derive(Debug, Clone)]
pub struct GenaiProvider {
    client: genai::Client,
    default_model: String,
    model_roles: HashMap<String, String>,
}

impl GenaiProvider {
    /// Create a provider with default client (reads API keys and model roles from env).
    ///
    /// Model roles are read from `MODEL_*` env vars (case-insensitive):
    /// `MODEL_FAST=gpt-4o-mini` makes `model_role: "fast"` resolve to `gpt-4o-mini`.
    ///
    /// If `LLM_API_BASE` is set, routes all requests to that endpoint using the
    /// OpenAI adapter (for llama.cpp, vLLM, or any OpenAI-compatible server).
    pub fn new(default_model: impl Into<String>) -> Self {
        let mut model_roles = HashMap::new();
        for (key, value) in std::env::vars() {
            if let Some(role) = key.strip_prefix("MODEL_") {
                model_roles.insert(role.to_lowercase(), value);
            }
        }

        let client = if let Ok(base_url) = std::env::var("LLM_API_BASE") {
            // Custom endpoint: route all models through OpenAI adapter to this URL.
            // Ensure trailing slash so reqwest::Url::join preserves path prefix
            // (e.g. "http://host/v1" + "chat/completions" would drop "/v1" without it).
            let endpoint_url = if base_url.ends_with('/') { base_url } else { format!("{}/", base_url) };
            tracing::info!(endpoint_url = %endpoint_url, "LLM_API_BASE set — routing all models via OpenAI adapter");
            let resolver = genai::resolver::ServiceTargetResolver::from_resolver_fn(
                move |service_target: genai::ServiceTarget| -> std::result::Result<genai::ServiceTarget, genai::resolver::Error> {
                    let genai::ServiceTarget { model, .. } = service_target;
                    let endpoint = genai::resolver::Endpoint::from_owned(endpoint_url.clone());
                    let auth = genai::resolver::AuthData::from_single("not-needed");
                    let model = genai::ModelIden::new(genai::adapter::AdapterKind::OpenAI, model.model_name);
                    Ok(genai::ServiceTarget { endpoint, auth, model })
                },
            );
            genai::Client::builder().with_service_target_resolver(resolver).build()
        } else {
            genai::Client::default()
        };

        Self {
            client,
            default_model: default_model.into(),
            model_roles,
        }
    }

    /// Create a provider with a pre-configured genai Client.
    pub fn with_client(client: genai::Client, default_model: impl Into<String>) -> Self {
        Self {
            client,
            default_model: default_model.into(),
            model_roles: HashMap::new(),
        }
    }

    /// Add a model role mapping (e.g. `"fast"` → `"gpt-4o-mini"`).
    pub fn with_model_role(mut self, role: impl Into<String>, model: impl Into<String>) -> Self {
        self.model_roles.insert(role.into().to_lowercase(), model.into());
        self
    }

    fn resolve_model(&self, req: &ChatRequest) -> String {
        match &req.model {
            Some(role) => self.model_roles.get(&role.to_lowercase())
                .cloned()
                .unwrap_or_else(|| role.clone()),
            None => self.default_model.clone(),
        }
    }

    fn build_options(&self, req: &ChatRequest) -> ChatOptions {
        let mut opts = ChatOptions::default()
            .with_capture_usage(true)
            .with_capture_content(true)
            .with_capture_tool_calls(true);
        if let Some(temp) = req.temperature {
            opts = opts.with_temperature(temp);
        }
        if let Some(max) = req.max_tokens {
            #[allow(clippy::cast_sign_loss)]
            {
                opts = opts.with_max_tokens(max as u32);
            }
        }
        if let Some(ref schema) = req.response_format {
            // Force schema-constrained output (response_format=json_schema on
            // OpenAI-compat backends, structured output on others). Names allow
            // only `-` and `_` per genai/OpenAI rules.
            use genai::chat::{ChatResponseFormat, JsonSpec};
            opts = opts.with_response_format(ChatResponseFormat::JsonSpec(
                JsonSpec::new("agent_output", schema.clone()),
            ));
        }
        opts
    }

    fn build_request(&self, req: &ChatRequest) -> GenaiRequest {
        let mut system_msg = None;
        let mut messages = Vec::new();

        for msg in &req.messages {
            match msg.role.as_str() {
                "system" => {
                    system_msg = Some(msg.content.as_text_lossy());
                }
                "user" => {
                    messages.push(GenaiMessage::user(msg.content.as_text_lossy()));
                }
                "assistant" => {
                    if let Some(ref tcs) = msg.tool_calls {
                        let genai_tcs: Vec<genai::chat::ToolCall> = tcs
                            .iter()
                            .map(|tc| genai::chat::ToolCall {
                                call_id: tc.id.clone(),
                                fn_name: tc.name.clone(),
                                fn_arguments: tc.arguments.clone(),
                                thought_signatures: None,
                            })
                            .collect();
                        // Build assistant message with tool calls + optional text
                        let thought_sigs: Vec<String> = Vec::new();
                        let mut m = GenaiMessage::assistant_tool_calls_with_thoughts(genai_tcs, thought_sigs);
                        if !msg.content.is_empty() {
                            m.content.prepend(genai::chat::ContentPart::from_text(msg.content.as_text_lossy()));
                        }
                        messages.push(m);
                    } else {
                        messages.push(GenaiMessage::assistant(msg.content.as_text_lossy()));
                    }
                }
                "tool" => {
                    let call_id = msg.tool_call_id.clone().unwrap_or_default();
                    let tool_resp = genai::chat::ToolResponse::new(call_id, msg.content.as_text_lossy());
                    messages.push(tool_resp.into());
                }
                _ => {
                    messages.push(GenaiMessage::user(msg.content.as_text_lossy()));
                }
            }
        }

        let mut genai_req = GenaiRequest::from_messages(messages);
        if let Some(sys) = system_msg {
            genai_req = genai_req.with_system(sys);
        }

        // Convert tool definitions
        if let Some(ref tools) = req.tools {
            let genai_tools: Vec<GenaiTool> = tools
                .iter()
                .filter_map(|t| {
                    let func = t.get("function")?;
                    let name = func.get("name")?.as_str()?;
                    let mut tool = GenaiTool::new(name);
                    if let Some(desc) = func.get("description").and_then(|d| d.as_str()) {
                        tool = tool.with_description(desc);
                    }
                    if let Some(params) = func.get("parameters") {
                        tool = tool.with_schema(params.clone());
                    }
                    Some(tool)
                })
                .collect();
            if !genai_tools.is_empty() {
                genai_req = genai_req.with_tools(genai_tools);
            }
        }

        genai_req
    }
}

fn convert_usage(usage: &genai::chat::Usage) -> TokenUsage {
    TokenUsage {
        prompt_tokens: usage.prompt_tokens.unwrap_or(0),
        completion_tokens: usage.completion_tokens.unwrap_or(0),
        total_tokens: usage.total_tokens.unwrap_or(0),
    }
}

fn convert_tool_call(tc: genai::chat::ToolCall) -> ToolCall {
    ToolCall {
        id: tc.call_id,
        name: tc.fn_name,
        arguments: tc.fn_arguments,
    }
}

#[async_trait]
impl LlmProvider for GenaiProvider {
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let model = self.resolve_model(req);
        let options = self.build_options(req);

        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            let genai_req = self.build_request(req);
            match self.client.exec_chat(&model, genai_req, Some(&options)).await {
                Ok(resp) => {
                    // Fall through to response conversion below
                    let usage = convert_usage(&resp.usage);
                    let tool_calls: Vec<ToolCall> = resp.content.tool_calls()
                        .into_iter()
                        .cloned()
                        .map(convert_tool_call)
                        .collect();
                    let content = resp.content.into_first_text();
                    return Ok(ChatResponse {
                        content,
                        tool_calls,
                        usage,
                        model: resp.model_iden.model_name.to_string(),
                    });
                }
                Err(e) if is_transient(&e) && attempt + 1 < MAX_RETRIES => {
                    let backoff = INITIAL_BACKOFF_MS * 2u64.pow(attempt);
                    tracing::warn!(attempt = attempt + 1, backoff_ms = backoff, error = %e, "LLM transient error, retrying");
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff)).await;
                    last_err = Some(e);
                }
                Err(e) => {
                    return Err(Error::internal(format!("LLM error: {e}")));
                }
            }
        }
        Err(Error::internal(format!("LLM error after {MAX_RETRIES} retries: {}", last_err.map(|e| e.to_string()).unwrap_or_default())))
    }

    async fn chat_stream(
        &self,
        req: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        let model = self.resolve_model(req);
        let options = self.build_options(req);

        let mut stream_resp = None;
        for attempt in 0..MAX_RETRIES {
            let genai_req = self.build_request(req);
            match self.client.exec_chat_stream(&model, genai_req, Some(&options)).await {
                Ok(resp) => { stream_resp = Some(resp); break; }
                Err(e) if is_transient(&e) && attempt + 1 < MAX_RETRIES => {
                    let backoff = INITIAL_BACKOFF_MS * 2u64.pow(attempt);
                    tracing::warn!(attempt = attempt + 1, backoff_ms = backoff, error = %e, "LLM stream transient error, retrying");
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff)).await;
                }
                Err(e) => return Err(Error::internal(format!("LLM stream error: {e}"))),
            }
        }
        let stream_resp = stream_resp
            .ok_or_else(|| Error::internal("LLM stream failed after retries".to_string()))?;

        let mapped = stream_resp.stream.filter_map(|event| async {
            match event {
                Ok(ChatStreamEvent::Chunk(chunk)) => Some(Ok(StreamChunk {
                    delta_content: Some(chunk.content),
                    tool_calls: vec![],
                    usage: None,
                })),
                Ok(ChatStreamEvent::ToolCallChunk(tc_chunk)) => Some(Ok(StreamChunk {
                    delta_content: None,
                    tool_calls: vec![convert_tool_call(tc_chunk.tool_call)],
                    usage: None,
                })),
                Ok(ChatStreamEvent::End(end)) => {
                    let usage = end.captured_usage.as_ref().map(convert_usage);
                    Some(Ok(StreamChunk {
                        delta_content: None,
                        tool_calls: vec![],
                        usage,
                    }))
                }
                Ok(_) => None, // Start, ReasoningChunk, ThoughtSignatureChunk
                Err(e) => Some(Err(Error::internal(format!("LLM stream error: {e}")))),
            }
        });

        Ok(Box::pin(mapped))
    }
}
