//! Standard multi-provider adapter backed by the `genai` crate.

use async_trait::async_trait;
use futures::StreamExt;
use genai::chat::{
    ChatMessage as GenaiMessage, ChatOptions, ChatRequest as GenaiRequest, ChatStreamEvent,
    Tool as GenaiTool,
};
use std::collections::HashMap;
use std::sync::Arc;

use super::{LlmProvider, ModelRequest, ModelStream, ModelStreamEvent, Role, TokenUsage, ToolCall};
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

        let stream = response.stream.filter_map(|event| async move {
            match event {
                Ok(ChatStreamEvent::Chunk(chunk)) => {
                    Some(Ok(ModelStreamEvent::Text(chunk.content)))
                }
                Ok(ChatStreamEvent::ToolCallChunk(chunk)) => {
                    Some(Ok(ModelStreamEvent::ToolCall(tool_call(chunk.tool_call))))
                }
                Ok(ChatStreamEvent::End(end)) => end
                    .captured_usage
                    .as_ref()
                    .map(token_usage)
                    .map(ModelStreamEvent::Usage)
                    .map(Ok),
                Ok(_) => None,
                Err(error) => Some(Err(provider_error("LLM stream failed", error))),
            }
        });
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
