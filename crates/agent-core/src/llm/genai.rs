//! GenAI-backed LlmProvider — multi-provider chat through the `genai` crate.
//!
//! API keys are discovered from environment variables based on the model prefix:
//! `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, …
//!
//! Model roles are resolved from `MODEL_*` env vars (case-insensitive). If `LLM_API_BASE`
//! is set, every request is routed through the OpenAI adapter to that endpoint.

use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use genai::chat::{
    ChatMessage as GenaiMessage, ChatOptions, ChatRequest as GenaiRequest, ChatStreamEvent,
    Tool as GenaiTool,
};
use std::collections::HashMap;
use std::pin::Pin;

use super::{ChatRequest, ChatResponse, LlmProvider, StreamChunk, TokenUsage};
use crate::error::{Error, Result};
use crate::tools::ToolCall;

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 500;

fn is_transient(err: &genai::Error) -> bool {
    match err {
        genai::Error::HttpError { status, .. } => {
            status.as_u16() == 429 || status.is_server_error()
        }
        genai::Error::WebAdapterCall { .. }
        | genai::Error::WebModelCall { .. }
        | genai::Error::WebStream { .. } => true,
        _ => false,
    }
}

#[derive(Debug, Clone)]
pub struct GenaiProvider {
    client: genai::Client,
    default_model: String,
    model_roles: HashMap<String, String>,
}

impl GenaiProvider {
    /// Build a provider from the ambient environment.
    ///
    /// - Reads `MODEL_*` env vars as role aliases.
    /// - If `LLM_API_BASE` is set, routes every model via the OpenAI adapter to that URL.
    pub fn from_env(default_model: impl Into<String>) -> Self {
        let mut model_roles = HashMap::new();
        for (key, value) in std::env::vars() {
            if let Some(role) = key.strip_prefix("MODEL_") {
                model_roles.insert(role.to_lowercase(), value);
            }
        }

        let client = if let Ok(base_url) = std::env::var("LLM_API_BASE") {
            let endpoint_url = base_url.clone();
            let resolver = genai::resolver::ServiceTargetResolver::from_resolver_fn(
                move |service_target: genai::ServiceTarget| -> std::result::Result<genai::ServiceTarget, genai::resolver::Error> {
                    let genai::ServiceTarget { model, .. } = service_target;
                    let endpoint = genai::resolver::Endpoint::from_owned(endpoint_url.clone());
                    let auth = genai::resolver::AuthData::from_single("not-needed");
                    let model = genai::ModelIden::new(genai::adapter::AdapterKind::OpenAI, model.model_name);
                    Ok(genai::ServiceTarget { endpoint, auth, model })
                },
            );
            tracing::info!(base_url = %base_url, "LLM_API_BASE set — routing all models via OpenAI adapter");
            genai::Client::builder()
                .with_service_target_resolver(resolver)
                .build()
        } else {
            genai::Client::default()
        };

        Self {
            client,
            default_model: default_model.into(),
            model_roles,
        }
    }

    pub fn with_model_role(mut self, role: impl Into<String>, model: impl Into<String>) -> Self {
        self.model_roles
            .insert(role.into().to_lowercase(), model.into());
        self
    }

    fn resolve_model(&self, req: &ChatRequest) -> String {
        match &req.model {
            Some(role) => self
                .model_roles
                .get(&role.to_lowercase())
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
        if let Some(t) = req.temperature {
            opts = opts.with_temperature(t);
        }
        if let Some(m) = req.max_tokens {
            #[allow(clippy::cast_sign_loss)]
            {
                opts = opts.with_max_tokens(m as u32);
            }
        }
        opts
    }

    fn build_request(&self, req: &ChatRequest) -> GenaiRequest {
        let mut system_msg: Option<String> = None;
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
                        let thought_sigs: Vec<String> = Vec::new();
                        let mut m = GenaiMessage::assistant_tool_calls_with_thoughts(
                            genai_tcs,
                            thought_sigs,
                        );
                        if !msg.content.is_empty() {
                            m.content.prepend(genai::chat::ContentPart::from_text(
                                msg.content.as_text_lossy(),
                            ));
                        }
                        messages.push(m);
                    } else {
                        messages.push(GenaiMessage::assistant(msg.content.as_text_lossy()));
                    }
                }
                "tool" => {
                    let call_id = msg.tool_call_id.clone().unwrap_or_default();
                    let resp = genai::chat::ToolResponse::new(call_id, msg.content.as_text_lossy());
                    messages.push(resp.into());
                }
                _ => messages.push(GenaiMessage::user(msg.content.as_text_lossy())),
            }
        }

        let mut out = GenaiRequest::from_messages(messages);
        if let Some(sys) = system_msg {
            out = out.with_system(sys);
        }

        if let Some(ref tools) = req.tools {
            let gen_tools: Vec<GenaiTool> = tools
                .iter()
                .filter_map(|t| {
                    let name = t.get("name").and_then(|n| n.as_str())?;
                    let mut tool = GenaiTool::new(name);
                    if let Some(d) = t.get("description").and_then(|d| d.as_str()) {
                        tool = tool.with_description(d);
                    }
                    if let Some(p) = t.get("parameters") {
                        tool = tool.with_schema(p.clone());
                    }
                    Some(tool)
                })
                .collect();
            if !gen_tools.is_empty() {
                out = out.with_tools(gen_tools);
            }
        }

        out
    }
}

fn convert_usage(u: &genai::chat::Usage) -> TokenUsage {
    TokenUsage {
        prompt_tokens: u.prompt_tokens.unwrap_or(0),
        completion_tokens: u.completion_tokens.unwrap_or(0),
        total_tokens: u.total_tokens.unwrap_or(0),
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
            match self
                .client
                .exec_chat(&model, genai_req, Some(&options))
                .await
            {
                Ok(resp) => {
                    let usage = convert_usage(&resp.usage);
                    let tool_calls: Vec<ToolCall> = resp
                        .content
                        .tool_calls()
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
                    tracing::warn!(attempt = attempt + 1, backoff_ms = backoff, error = %e, "transient LLM error; retrying");
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff)).await;
                    last_err = Some(e);
                }
                Err(e) => return Err(Error::Llm(e.to_string())),
            }
        }
        Err(Error::Llm(format!(
            "exhausted {MAX_RETRIES} retries: {}",
            last_err.map(|e| e.to_string()).unwrap_or_default()
        )))
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
            match self
                .client
                .exec_chat_stream(&model, genai_req, Some(&options))
                .await
            {
                Ok(r) => {
                    stream_resp = Some(r);
                    break;
                }
                Err(e) if is_transient(&e) && attempt + 1 < MAX_RETRIES => {
                    let backoff = INITIAL_BACKOFF_MS * 2u64.pow(attempt);
                    tracing::warn!(attempt = attempt + 1, backoff_ms = backoff, error = %e, "transient LLM stream error; retrying");
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff)).await;
                }
                Err(e) => return Err(Error::Llm(e.to_string())),
            }
        }
        let stream_resp =
            stream_resp.ok_or_else(|| Error::Llm("stream setup failed after retries".into()))?;

        let mapped = stream_resp.stream.filter_map(|event| async {
            match event {
                Ok(ChatStreamEvent::Chunk(chunk)) => Some(Ok(StreamChunk {
                    delta_content: Some(chunk.content),
                    tool_calls: vec![],
                    usage: None,
                })),
                Ok(ChatStreamEvent::ToolCallChunk(tc)) => Some(Ok(StreamChunk {
                    delta_content: None,
                    tool_calls: vec![convert_tool_call(tc.tool_call)],
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
                Ok(_) => None,
                Err(e) => Some(Err(Error::Llm(e.to_string()))),
            }
        });

        Ok(Box::pin(mapped))
    }
}
