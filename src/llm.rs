//! Provider-neutral LLM streaming types and the built-in ReAct configuration.

use async_trait::async_trait;
use futures::Stream;
use serde_json::Value;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;

use crate::tools::{DenialBehavior, ToolDefinition, ToolSpec};
use crate::types::{Error, Result, RunView};

pub mod genai;

/// Chat role used by the provider adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// One model conversation message.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    pub fn assistant(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls,
        }
    }

    pub fn tool(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(call_id.into()),
            tool_calls: Vec::new(),
        }
    }

    fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }
}

/// Tool call emitted by a model.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Token counts captured by a provider.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Model request. Both text and structured calls use the same stream method.
#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub model: Option<Arc<str>>,
    pub response_schema: Option<Value>,
    /// Provider-specific top-level request fields.
    pub extra_body: Option<Value>,
}

/// Why a model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStopReason {
    Completed(String),
    MaxTokens(String),
    ToolCall(String),
    ContentFilter(String),
    StopSequence(String),
    Other(String),
}

/// Events yielded by a provider stream.
#[derive(Debug, Clone)]
pub enum ModelStreamEvent {
    Text(String),
    ToolCall(ToolCall),
    Usage(TokenUsage),
    StopReason(ModelStopReason),
}

pub type ModelStream = Pin<Box<dyn Stream<Item = Result<ModelStreamEvent>> + Send>>;

/// LLM provider. A non-streaming backend may yield one `Text` event.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn stream(&self, request: &ModelRequest) -> Result<ModelStream>;
}

/// Prompt attached directly to an LLM stage.
#[derive(Clone)]
pub enum Prompt {
    Static(Arc<str>),
    Dynamic(Arc<dyn PromptBuilder>),
}

impl Prompt {
    pub fn text(value: impl Into<Arc<str>>) -> Self {
        Self::Static(value.into())
    }

    pub fn dynamic(builder: impl PromptBuilder + 'static) -> Self {
        Self::Dynamic(Arc::new(builder))
    }

    pub fn build(&self, run: &RunView<'_>) -> Result<String> {
        match self {
            Self::Static(value) => Ok(value.to_string()),
            Self::Dynamic(builder) => builder.build(run),
        }
    }
}

impl fmt::Debug for Prompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static(_) => f.write_str("Prompt::Static(..)"),
            Self::Dynamic(_) => f.write_str("Prompt::Dynamic(..)"),
        }
    }
}

pub trait PromptBuilder: Send + Sync {
    fn build(&self, run: &RunView<'_>) -> Result<String>;
}

impl<F> PromptBuilder for F
where
    F: Fn(&RunView<'_>) -> Result<String> + Send + Sync,
{
    fn build(&self, run: &RunView<'_>) -> Result<String> {
        self(run)
    }
}

/// Completed response passed through hooks and then interpreted by the action.
#[derive(Debug, Clone, Default)]
pub struct ModelResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub stop_reason: Option<ModelStopReason>,
}

/// Decision made before an LLM-selected tool call.
#[derive(Debug, Clone)]
pub enum ToolDecision {
    Continue,
    Reject { reason: Arc<str> },
    Replace(Value),
}

/// Result exposed to the post-tool hook.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub value: Value,
    pub succeeded: bool,
}

/// Narrow hook for behavior that needs the model/tool-loop boundary.
#[async_trait]
pub trait LlmLoopHook: Send + Sync {
    /// Adjust conversation context before a model call. Tool capabilities and
    /// model settings remain owned by the stage.
    async fn before_model(&self, _messages: &mut Vec<Message>) -> Result<()> {
        Ok(())
    }

    async fn after_model(&self, _response: &mut ModelResponse) -> Result<()> {
        Ok(())
    }

    async fn before_tool(&self, _call: &ToolCall) -> Result<ToolDecision> {
        Ok(ToolDecision::Continue)
    }

    async fn after_tool(&self, _call: &ToolCall, _result: &mut ToolResult) -> Result<()> {
        Ok(())
    }
}

pub type OutputValidator = Arc<dyn Fn(&Value) -> Result<()> + Send + Sync>;

/// Final LLM output interpretation.
#[derive(Clone)]
pub enum LlmOutput {
    Text,
    Structured {
        schema: Value,
        validate: OutputValidator,
    },
}

impl fmt::Debug for LlmOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => f.write_str("LlmOutput::Text"),
            Self::Structured { schema, .. } => f
                .debug_struct("LlmOutput::Structured")
                .field("schema", schema)
                .finish_non_exhaustive(),
        }
    }
}

/// Built-in LLM action configuration.
#[derive(Clone)]
pub struct LlmAction {
    pub prompt: Prompt,
    pub output: LlmOutput,
    pub tools: Vec<Arc<ToolDefinition>>,
    pub hooks: Vec<Arc<dyn LlmLoopHook>>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub model: Option<Arc<str>>,
    pub extra_body: Option<Value>,
    pub max_tool_rounds: u32,
    pub on_denied: DenialBehavior,
}

impl LlmAction {
    pub fn text(prompt: Prompt) -> Self {
        Self {
            prompt,
            output: LlmOutput::Text,
            tools: Vec::new(),
            hooks: Vec::new(),
            temperature: None,
            max_tokens: None,
            model: None,
            extra_body: None,
            max_tool_rounds: 10,
            on_denied: DenialBehavior::Continue,
        }
    }

    pub fn structured(
        prompt: Prompt,
        schema: Value,
        validate: impl Fn(&Value) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            output: LlmOutput::Structured {
                schema,
                validate: Arc::new(validate),
            },
            ..Self::text(prompt)
        }
    }

    pub fn with_tools(mut self, tools: impl IntoIterator<Item = Arc<ToolDefinition>>) -> Self {
        self.tools = tools.into_iter().collect();
        self
    }

    pub fn with_hook(mut self, hook: Arc<dyn LlmLoopHook>) -> Self {
        self.hooks.push(hook);
        self
    }

    pub fn with_model(mut self, model: impl Into<Arc<str>>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Add provider-specific top-level request fields.
    pub fn with_extra_body(mut self, extra_body: Value) -> Self {
        self.extra_body = Some(extra_body);
        self
    }

    pub fn with_max_tool_rounds(mut self, max_tool_rounds: u32) -> Self {
        self.max_tool_rounds = max_tool_rounds;
        self
    }

    pub fn on_denied(mut self, behavior: DenialBehavior) -> Self {
        self.on_denied = behavior;
        self
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.temperature.is_some_and(|value| !value.is_finite()) {
            return Err(Error::configuration("LLM temperature must be finite"));
        }
        if self.max_tokens == Some(0) {
            return Err(Error::configuration("LLM max_tokens must be positive"));
        }
        if self
            .extra_body
            .as_ref()
            .is_some_and(|value| !value.is_object())
        {
            return Err(Error::configuration("LLM extra_body must be an object"));
        }
        let mut names = std::collections::HashSet::new();
        for tool in &self.tools {
            if !names.insert(tool.spec.name.as_ref()) {
                return Err(Error::configuration(format!(
                    "duplicate LLM tool '{}'",
                    tool.spec.name
                )));
            }
        }
        Ok(())
    }
}

impl fmt::Debug for LlmAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlmAction")
            .field("prompt", &self.prompt)
            .field("output", &self.output)
            .field(
                "tools",
                &self
                    .tools
                    .iter()
                    .map(|tool| tool.spec.name.as_ref())
                    .collect::<Vec<_>>(),
            )
            .field("hooks", &self.hooks.len())
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("model", &self.model)
            .field("extra_body", &self.extra_body)
            .field("max_tool_rounds", &self.max_tool_rounds)
            .field("on_denied", &self.on_denied)
            .finish()
    }
}

/// Convenient deterministic provider for unit and consumer tests.
#[derive(Debug)]
pub struct MockLlmProvider {
    responses: tokio::sync::Mutex<std::collections::VecDeque<Vec<ModelStreamEvent>>>,
}

impl MockLlmProvider {
    pub fn new(responses: impl IntoIterator<Item = Vec<ModelStreamEvent>>) -> Self {
        Self {
            responses: tokio::sync::Mutex::new(responses.into_iter().collect()),
        }
    }

    pub fn text(response: impl Into<String>) -> Self {
        Self::new([vec![ModelStreamEvent::Text(response.into())]])
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn stream(&self, _request: &ModelRequest) -> Result<ModelStream> {
        let events = self
            .responses
            .lock()
            .await
            .pop_front()
            .ok_or_else(|| Error::permanent("mock LLM has no response remaining"))?;
        Ok(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
    }
}
