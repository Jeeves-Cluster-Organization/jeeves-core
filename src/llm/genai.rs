//! Standard multi-provider adapter backed by the `genai` crate.

use async_trait::async_trait;
use futures::StreamExt;
use genai::chat::{
    ChatMessage as GenaiMessage, ChatOptions, ChatRequest as GenaiRequest, ChatStreamEvent,
    Tool as GenaiTool,
};
use std::collections::HashMap;
use std::sync::Arc;

use super::{
    LlmProvider, ModelRequest, ModelStopReason, ModelStream, ModelStreamEvent, Role, TokenUsage,
    ToolCall,
};
use crate::types::{Error, Result};

/// Multi-provider LLM adapter. Model roles are explicit mappings, while API
/// credentials continue to be resolved by `genai`.
#[derive(Debug, Clone)]
pub struct GenaiProvider {
    client: genai::Client,
    default_model: Arc<str>,
    model_roles: HashMap<Arc<str>, Arc<str>>,
}

impl GenaiProvider {
    /// Construct using `genai`'s default credential resolution. When
    /// `LLM_API_BASE` is set, all models use that OpenAI-compatible endpoint.
    pub fn new(default_model: impl Into<Arc<str>>) -> Self {
        let client = if let Ok(base_url) = std::env::var("LLM_API_BASE") {
            let endpoint_url = if base_url.ends_with('/') {
                base_url
            } else {
                format!("{base_url}/")
            };
            let resolver = genai::resolver::ServiceTargetResolver::from_resolver_fn(
                move |service_target: genai::ServiceTarget| {
                    let genai::ServiceTarget { model, .. } = service_target;
                    let endpoint = genai::resolver::Endpoint::from_owned(endpoint_url.clone());
                    let auth = genai::resolver::AuthData::from_single("not-needed");
                    let model = genai::ModelIden::new(
                        genai::adapter::AdapterKind::OpenAI,
                        model.model_name,
                    );
                    Ok(genai::ServiceTarget {
                        endpoint,
                        auth,
                        model,
                    })
                },
            );
            genai::Client::builder()
                .with_service_target_resolver(resolver)
                .build()
        } else {
            genai::Client::default()
        };

        Self {
            client,
            default_model: default_model.into(),
            model_roles: HashMap::new(),
        }
    }

    pub fn with_client(client: genai::Client, default_model: impl Into<Arc<str>>) -> Self {
        Self {
            client,
            default_model: default_model.into(),
            model_roles: HashMap::new(),
        }
    }

    pub fn with_model_role(
        mut self,
        role: impl Into<Arc<str>>,
        model: impl Into<Arc<str>>,
    ) -> Self {
        self.model_roles.insert(role.into(), model.into());
        self
    }

    fn model(&self, request: &ModelRequest) -> Arc<str> {
        request
            .model
            .as_ref()
            .and_then(|role| self.model_roles.get(role.as_ref()))
            .cloned()
            .or_else(|| request.model.clone())
            .unwrap_or_else(|| self.default_model.clone())
    }

    fn options(request: &ModelRequest) -> ChatOptions {
        let mut options = ChatOptions::default()
            .with_capture_usage(true)
            .with_capture_content(true)
            .with_capture_tool_calls(true);
        if let Some(temperature) = request.temperature {
            options = options.with_temperature(temperature);
        }
        if let Some(max_tokens) = request.max_tokens {
            options = options.with_max_tokens(max_tokens);
        }
        if let Some(schema) = &request.response_schema {
            use genai::chat::{ChatResponseFormat, JsonSpec};
            options = options.with_response_format(ChatResponseFormat::JsonSpec(JsonSpec::new(
                "stage_output",
                schema.clone(),
            )));
        }
        if let Some(extra_body) = &request.extra_body {
            options = options.with_extra_body(extra_body.clone());
        }
        options
    }

    fn request(request: &ModelRequest) -> GenaiRequest {
        let mut system = None;
        let mut messages = Vec::new();
        for message in &request.messages {
            match message.role {
                Role::System => system = Some(message.content.clone()),
                Role::User => messages.push(GenaiMessage::user(message.content.clone())),
                Role::Assistant if !message.tool_calls.is_empty() => {
                    let calls = message
                        .tool_calls
                        .iter()
                        .map(|call| genai::chat::ToolCall {
                            call_id: call.id.clone(),
                            fn_name: call.name.clone(),
                            fn_arguments: call.arguments.clone(),
                            thought_signatures: None,
                        })
                        .collect();
                    let mut converted =
                        GenaiMessage::assistant_tool_calls_with_thoughts(calls, Vec::new());
                    if !message.content.is_empty() {
                        converted
                            .content
                            .prepend(genai::chat::ContentPart::from_text(message.content.clone()));
                    }
                    messages.push(converted);
                }
                Role::Assistant => {
                    messages.push(GenaiMessage::assistant(message.content.clone()));
                }
                Role::Tool => {
                    let response = genai::chat::ToolResponse::new(
                        message.tool_call_id.clone().unwrap_or_default(),
                        message.content.clone(),
                    );
                    messages.push(response.into());
                }
            }
        }

        let mut converted = GenaiRequest::from_messages(messages);
        if let Some(system) = system {
            converted = converted.with_system(system);
        }
        if !request.tools.is_empty() {
            let tools: Vec<GenaiTool> = request
                .tools
                .iter()
                .map(|spec| {
                    GenaiTool::new(spec.name.as_ref())
                        .with_description(spec.description.as_ref())
                        .with_schema(spec.parameters.clone())
                })
                .collect();
            converted = converted.with_tools(tools);
        }
        converted
    }
}

fn is_transient(error: &genai::Error) -> bool {
    match error {
        genai::Error::HttpError { status, .. } => {
            status.as_u16() == 429 || status.is_server_error()
        }
        genai::Error::WebAdapterCall { .. }
        | genai::Error::WebModelCall { .. }
        | genai::Error::WebStream { .. } => true,
        _ => false,
    }
}

fn provider_error(prefix: &str, error: genai::Error) -> Error {
    let message = format!("{prefix}: {error}");
    if is_transient(&error) {
        Error::transient(message)
    } else {
        Error::permanent(message)
    }
}

fn tool_call(call: genai::chat::ToolCall) -> ToolCall {
    let arguments = match call.fn_arguments {
        serde_json::Value::String(value) => {
            serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value))
        }
        value => value,
    };
    ToolCall {
        id: call.call_id,
        name: call.fn_name,
        arguments,
    }
}

fn token_usage(usage: &genai::chat::Usage) -> TokenUsage {
    TokenUsage {
        input_tokens: usage.prompt_tokens.unwrap_or(0) as u64,
        output_tokens: usage.completion_tokens.unwrap_or(0) as u64,
    }
}

fn stop_reason(reason: genai::chat::StopReason) -> ModelStopReason {
    use genai::chat::StopReason;
    match reason {
        StopReason::Completed(reason) => ModelStopReason::Completed(reason),
        StopReason::MaxTokens(reason) => ModelStopReason::MaxTokens(reason),
        StopReason::ToolCall(reason) => ModelStopReason::ToolCall(reason),
        StopReason::ContentFilter(reason) => ModelStopReason::ContentFilter(reason),
        StopReason::StopSequence(reason) => ModelStopReason::StopSequence(reason),
        StopReason::Other(reason) => ModelStopReason::Other(reason),
    }
}

#[async_trait]
impl LlmProvider for GenaiProvider {
    async fn stream(&self, request: &ModelRequest) -> Result<ModelStream> {
        let model = self.model(request);
        let response = self
            .client
            .exec_chat_stream(
                model.as_ref(),
                Self::request(request),
                Some(&Self::options(request)),
            )
            .await
            .map_err(|error| provider_error("LLM stream setup failed", error))?;

        let stream = response
            .stream
            .map(|event| match event {
                Ok(ChatStreamEvent::Chunk(chunk)) => {
                    vec![Ok(ModelStreamEvent::Text(chunk.content))]
                }
                Ok(ChatStreamEvent::ToolCallChunk(chunk)) => {
                    vec![Ok(ModelStreamEvent::ToolCall(tool_call(chunk.tool_call)))]
                }
                Ok(ChatStreamEvent::End(end)) => {
                    let mut events = Vec::with_capacity(2);
                    if let Some(usage) = end.captured_usage.as_ref() {
                        events.push(Ok(ModelStreamEvent::Usage(token_usage(usage))));
                    }
                    if let Some(reason) = end.captured_stop_reason {
                        events.push(Ok(ModelStreamEvent::StopReason(stop_reason(reason))));
                    }
                    events
                }
                Ok(_) => Vec::new(),
                Err(error) => vec![Err(provider_error("LLM stream failed", error))],
            })
            .flat_map(futures::stream::iter);
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn sends_max_tokens_and_extra_body_to_openai_compatible_http() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test server address");
        let (body_tx, body_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept provider request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set request read timeout");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            let header_end = loop {
                let read = stream.read(&mut buffer).expect("read request headers");
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
                if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then(|| {
                        value
                            .trim()
                            .parse::<usize>()
                            .expect("numeric content length")
                    })
                })
                .expect("content length header");
            while request.len() < header_end + content_length {
                let read = stream.read(&mut buffer).expect("read request body");
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
            }
            body_tx
                .send(request[header_end..header_end + content_length].to_vec())
                .expect("capture request body");
            let response_body = "data: [DONE]\n\n";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .expect("write SSE response");
        });

        let endpoint_url = format!("http://{address}/v1/");
        let resolver = genai::resolver::ServiceTargetResolver::from_resolver_fn(
            move |service_target: genai::ServiceTarget| {
                let genai::ServiceTarget { model, .. } = service_target;
                Ok(genai::ServiceTarget {
                    endpoint: genai::resolver::Endpoint::from_owned(endpoint_url.clone()),
                    auth: genai::resolver::AuthData::from_single("not-needed"),
                    model: genai::ModelIden::new(
                        genai::adapter::AdapterKind::OpenAI,
                        model.model_name,
                    ),
                })
            },
        );
        let client = genai::Client::builder()
            .with_service_target_resolver(resolver)
            .build();
        let provider: Arc<dyn LlmProvider> =
            Arc::new(GenaiProvider::with_client(client, "test-model"));
        let action = crate::llm::LlmAction::text(crate::llm::Prompt::text("answer briefly"))
            .with_max_tokens(128)
            .with_extra_body(serde_json::json!({
                "chat_template_kwargs": {"enable_thinking": false}
            }));
        let workflow = crate::workflow::Workflow::builder("transport")
            .stage(crate::workflow::Stage::llm("request", action))
            .build()
            .expect("build transport workflow");
        let engine = crate::engine::Engine::builder()
            .llm(provider)
            .workflow(workflow)
            .expect("register transport workflow")
            .build()
            .expect("build transport engine");
        let outcome = engine
            .run("transport", "hello")
            .await
            .expect("run transport");
        assert!(matches!(
            outcome.as_ref(),
            crate::types::RunOutcome::Completed(_)
        ));
        let body: serde_json::Value = serde_json::from_slice(
            &body_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("receive captured body"),
        )
        .expect("parse captured JSON");
        server.join().expect("test server exits");

        assert_eq!(body["max_tokens"], 128);
        assert_eq!(
            body.pointer("/chat_template_kwargs/enable_thinking"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn parses_completed_streamed_tool_arguments() {
        let call = tool_call(genai::chat::ToolCall {
            call_id: "call-1".into(),
            fn_name: "lookup".into(),
            fn_arguments: serde_json::Value::String(r#"{"query":"value"}"#.into()),
            thought_signatures: None,
        });

        assert_eq!(call.arguments, serde_json::json!({"query": "value"}));
    }
}
