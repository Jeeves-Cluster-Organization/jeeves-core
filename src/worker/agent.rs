//! Agent execution — the bridge between kernel instructions and LLM calls.
//!
//! LlmAgent implements a ReAct-style tool loop: LLM → tool → LLM → ... → final text.
//! Streaming is opt-in via AgentContext.event_tx (None = fully buffered).

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

use tracing::instrument;

use crate::kernel::orchestrator_types::{AgentExecutionMetrics, ContextOverflow, ToolCallResult};
use crate::worker::llm::{
    collect_stream, ChatMessage, ChatRequest, LlmProvider, MessageContent, PipelineEvent, ToolCall,
};
use crate::worker::prompts::PromptRegistry;
use crate::worker::tools::{ContentPart, ContentResolver, ToolRegistry};

/// Output from agent execution.
#[must_use]
#[derive(Debug, Clone)]
pub struct AgentOutput {
    pub output: serde_json::Value,
    pub metrics: AgentExecutionMetrics,
    pub success: bool,
    pub error_message: String,
    /// If set, the agent is requesting a flow interrupt (e.g., tool confirmation).
    /// The worker loop sets this on the envelope and suspends the stage.
    pub interrupt_request: Option<crate::envelope::FlowInterrupt>,
}

/// Context provided to an agent for execution.
///
/// Built by the worker loop from the current envelope state and pipeline stage config.
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// Original user input for this pipeline run.
    pub raw_input: String,
    /// Accumulated outputs from prior stages, keyed by `output_key` then field name.
    pub outputs: HashMap<String, HashMap<String, serde_json::Value>>,
    /// Pipeline state fields (persisted across loop-backs via `state_schema`).
    pub state: HashMap<String, serde_json::Value>,
    /// Envelope metadata (user_id, session_id, custom fields).
    pub metadata: HashMap<String, serde_json::Value>,
    /// Channel for streaming `PipelineEvent`s to the consumer (None = buffered mode).
    pub event_tx: Option<mpsc::Sender<PipelineEvent>>,
    /// Pipeline stage name this agent is executing within (None for ad-hoc execution).
    pub stage_name: Option<String>,
    /// Pipeline name for event attribution (dotted notation for child pipelines).
    pub pipeline_name: Arc<str>,
    /// Maximum estimated tokens allowed in LLM context (chars/4 heuristic).
    pub max_context_tokens: Option<i64>,
    /// Strategy when context exceeds max_context_tokens.
    pub context_overflow: Option<ContextOverflow>,
    /// Response from a resolved interrupt (e.g., user confirmed a destructive tool).
    /// Populated by the worker when resuming from an interrupt.
    pub interrupt_response: Option<serde_json::Value>,
}

/// Agent trait — implementations provide custom agent behavior.
#[async_trait]
pub trait Agent: Send + Sync + std::fmt::Debug + std::any::Any {
    /// Execute the agent's logic given the pipeline context.
    ///
    /// Returns `AgentOutput` containing the output value, execution metrics,
    /// and success/failure status. Errors in the Result indicate infrastructure
    /// failures; agent-level failures use `AgentOutput.success = false`.
    async fn process(&self, ctx: &AgentContext) -> crate::types::Result<AgentOutput>;
}

/// Default max tool-call rounds in the ReAct loop.
pub const DEFAULT_MAX_TOOL_ROUNDS: u32 = 10;

/// Default LLM-backed agent with ReAct tool loop and streaming support.
#[derive(Debug)]
pub struct LlmAgent {
    pub llm: Arc<dyn LlmProvider>,
    pub prompts: Arc<PromptRegistry>,
    pub tools: Arc<ToolRegistry>,
    pub prompt_key: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i32>,
    pub model: Option<String>,
    pub max_tool_rounds: u32,
    /// Optional content resolver for lazy `ContentPart::Ref` resolution.
    pub content_resolver: Option<Arc<dyn ContentResolver>>,
}

impl Default for LlmAgent {
    fn default() -> Self {
        Self {
            llm: Arc::new(crate::worker::llm::mock::MockLlmProvider::default()),
            prompts: Arc::new(PromptRegistry::empty()),
            tools: Arc::new(ToolRegistry::new()),
            prompt_key: String::new(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 10,
            content_resolver: None,
        }
    }
}

#[async_trait]
impl Agent for LlmAgent {
    #[instrument(skip(self, ctx), fields(prompt_key = %self.prompt_key))]
    async fn process(&self, ctx: &AgentContext) -> crate::types::Result<AgentOutput> {
        let start = Instant::now();

        // Build template vars from context
        let mut vars = HashMap::new();
        vars.insert("raw_input".to_string(), ctx.raw_input.clone());
        for (agent_name, output) in &ctx.outputs {
            for (key, value) in output {
                vars.insert(format!("{}_{}", agent_name, key), value_to_string(value));
            }
        }
        for (key, value) in &ctx.state {
            vars.insert(key.clone(), value_to_string(value));
        }
        for (key, value) in &ctx.metadata {
            vars.insert(key.clone(), value_to_string(value));
        }

        // Render prompt
        let prompt_text = self
            .prompts
            .render(&self.prompt_key, &vars)
            .unwrap_or_else(|| format!("No prompt found for key: {}", self.prompt_key));

        // Build initial messages
        let mut messages = vec![
            ChatMessage::system(prompt_text),
            ChatMessage::user(ctx.raw_input.clone()),
        ];

        // Build tool definitions (already ACL-filtered at ToolRegistry layer)
        let tool_defs = Arc::new(build_tool_defs(&self.tools));

        // Accumulated metrics across all rounds
        let mut total_llm_calls = 0i32;
        let mut total_tool_calls = 0i32;
        let mut total_tokens_in = 0i64;
        let mut total_tokens_out = 0i64;
        let mut last_response = None;
        let mut tool_results: Vec<ToolCallResult> = Vec::new();

        // ReAct tool loop
        for _round in 0..self.max_tool_rounds {
            // Context window safety check
            if let Some(max_tokens) = ctx.max_context_tokens {
                let estimated_tokens: i64 = messages.iter()
                    .map(|m| (m.content.len() / 4) as i64)
                    .sum();

                if estimated_tokens > max_tokens {
                    let overflow_strategy = ctx.context_overflow.unwrap_or_default();

                    match overflow_strategy {
                        ContextOverflow::TruncateOldest => {
                            // Keep system prompt (index 0) and latest user message (last).
                            // Drop intermediate messages oldest-first until under limit.
                            let mut running_est = estimated_tokens;
                            while messages.len() > 2 && running_est > max_tokens {
                                let dropped_tokens = (messages[1].content.len() / 4) as i64;
                                messages.remove(1);
                                running_est -= dropped_tokens;
                            }
                        }
                        ContextOverflow::Fail => {
                            return Ok(AgentOutput {
                                output: serde_json::json!({
                                    "error": "context_overflow",
                                    "estimated_tokens": estimated_tokens,
                                    "max_context_tokens": max_tokens,
                                }),
                                metrics: AgentExecutionMetrics {
                                    llm_calls: total_llm_calls,
                                    tool_calls: total_tool_calls,
                                    tokens_in: Some(total_tokens_in),
                                    tokens_out: Some(total_tokens_out),
                                    duration_ms: start.elapsed().as_millis() as i64,
                                    tool_results: tool_results.clone(),
                                },
                                success: false,
                                error_message: format!(
                                    "Context overflow: estimated {} tokens exceeds limit of {}",
                                    estimated_tokens, max_tokens
                                ),
                                interrupt_request: None,
                            });
                        }
                    }
                }
            }

            let req = ChatRequest {
                messages: messages.clone(),
                temperature: self.temperature,
                max_tokens: self.max_tokens,
                model: self.model.clone(),
                tools: if tool_defs.is_empty() { None } else { Some(tool_defs.clone()) },
            };

            // Use streaming path — collect_stream forwards deltas to event_tx if present
            let resp = match self.llm.chat_stream(&req).await {
                Ok(stream) => match collect_stream(stream, ctx.event_tx.as_ref(), ctx.stage_name.as_deref(), ctx.pipeline_name.clone()).await {
                    Ok(resp) => resp,
                    Err(e) => return Ok(make_error_output(e, start, total_llm_calls + 1)),
                },
                Err(e) => return Ok(make_error_output(e, start, total_llm_calls + 1)),
            };

            total_llm_calls += 1;
            total_tokens_in += resp.usage.prompt_tokens as i64;
            total_tokens_out += resp.usage.completion_tokens as i64;

            if resp.tool_calls.is_empty() {
                // No tool calls — this is the final response
                last_response = Some(resp);
                break;
            }

            tracing::debug!(round = _round, tool_count = resp.tool_calls.len(), "tool_loop_round");

            // Tool calls: push assistant message, execute tools, push results
            let content = resp.content.clone().unwrap_or_default();
            messages.push(ChatMessage::assistant(content, Some(resp.tool_calls.clone())));

            for tc in &resp.tool_calls {
                total_tool_calls += 1;
                tracing::debug!(tool_name = %tc.name, "tool_execute");

                // Tool confirmation gate: check before executing
                if ctx.interrupt_response.is_none() {
                    if let Some(confirmation) = self.tools.requires_confirmation(&tc.name, &tc.arguments) {
                        let mut interrupt = crate::envelope::FlowInterrupt::new(
                            crate::envelope::InterruptKind::Confirmation,
                        ).with_message(confirmation.message.clone());
                        if let Some(data) = confirmation.action_data {
                            interrupt = interrupt.with_data(HashMap::from([("action_data".to_string(), data)]));
                        }
                        return Ok(AgentOutput {
                            output: serde_json::json!({
                                "status": "awaiting_confirmation",
                                "message": &confirmation.message,
                                "tool": &tc.name,
                            }),
                            interrupt_request: Some(interrupt),
                            metrics: AgentExecutionMetrics {
                                llm_calls: total_llm_calls,
                                tool_calls: total_tool_calls,
                                tokens_in: Some(total_tokens_in),
                                tokens_out: Some(total_tokens_out),
                                duration_ms: start.elapsed().as_millis() as i64,
                                tool_results: tool_results.clone(),
                            },
                            success: true,
                            error_message: String::new(),
                        });
                    }
                }

                if let Some(ref tx) = ctx.event_tx {
                    let _ = tx
                        .send(PipelineEvent::ToolCallStart {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            stage: ctx.stage_name.clone(),
                            pipeline: ctx.pipeline_name.clone(),
                        })
                        .await;
                }

                let params = tc.arguments.clone();
                let tool_start = Instant::now();
                let (result_text, result_content, tool_success, tool_error) = match self.tools.execute(&tc.name, params).await {
                    Ok(tool_output) => {
                        let text = tool_output.data.to_string();
                        let content = if tool_output.content.is_empty() {
                            None
                        } else {
                            Some(self.resolve_content_parts(&tool_output.content))
                        };
                        (text, content, true, None)
                    }
                    Err(e) => (
                        serde_json::json!({"error": e.to_string()}).to_string(),
                        None,
                        false,
                        Some(e.to_string()),
                    ),
                };
                tool_results.push(ToolCallResult {
                    name: tc.name.clone(),
                    success: tool_success,
                    latency_ms: tool_start.elapsed().as_millis() as u64,
                    error_type: tool_error,
                });

                if let Some(ref tx) = ctx.event_tx {
                    let _ = tx
                        .send(PipelineEvent::ToolResult {
                            id: tc.id.clone(),
                            content: result_text.clone(),
                            stage: ctx.stage_name.clone(),
                            pipeline: ctx.pipeline_name.clone(),
                        })
                        .await;
                }

                // Use multimodal content if available, otherwise text fallback
                match result_content {
                    Some(parts) => {
                        let mut all_parts = vec![ContentPart::Text { text: result_text }];
                        all_parts.extend(parts);
                        messages.push(ChatMessage::tool_result(
                            &tc.id,
                            MessageContent::Parts(all_parts),
                        ));
                    }
                    None => {
                        messages.push(ChatMessage::tool_result(&tc.id, result_text));
                    }
                }
            }

            last_response = Some(resp);
        }

        let duration = start.elapsed();
        let metrics = AgentExecutionMetrics {
            llm_calls: total_llm_calls,
            tool_calls: total_tool_calls,
            tokens_in: Some(total_tokens_in),
            tokens_out: Some(total_tokens_out),
            duration_ms: duration.as_millis() as i64,
            tool_results,
        };

        // Build output from last response
        let output = match last_response {
            Some(resp) => build_output_value(&resp.content, &resp.tool_calls),
            None => serde_json::json!({}),
        };

        Ok(AgentOutput {
            output,
            metrics,
            success: true,
            error_message: String::new(),
            interrupt_request: None,
        })
    }
}

impl LlmAgent {
    /// Resolve `ContentPart::Ref` entries using the registered content resolver.
    /// Refs that can't be resolved degrade to text placeholders.
    fn resolve_content_parts(&self, parts: &[ContentPart]) -> Vec<ContentPart> {
        parts.iter().map(|part| match part {
            ContentPart::Ref { content_type, ref_id } => {
                if let Some(ref resolver) = self.content_resolver {
                    if let Some(data) = resolver.resolve(ref_id, content_type) {
                        return ContentPart::Blob {
                            content_type: content_type.clone(),
                            data,
                        };
                    }
                }
                // Graceful degradation: unresolved ref becomes text placeholder
                ContentPart::Text {
                    text: format!("[content ref: {} ({})]", ref_id, content_type),
                }
            }
            other => other.clone(),
        }).collect()
    }
}

/// MCP-delegating agent — forwards to an MCP tool, returns its output.
///
/// Bridges deterministic Python agents: kernel dispatches to this Agent impl,
/// which calls an MCP tool with the full agent context as params, returns
/// the tool output as agent output.
#[derive(Debug)]
pub struct McpDelegatingAgent {
    pub tool_name: String,
    pub tools: Arc<ToolRegistry>,
}

#[async_trait]
impl Agent for McpDelegatingAgent {
    #[instrument(skip(self, ctx), fields(tool = %self.tool_name))]
    async fn process(&self, ctx: &AgentContext) -> crate::types::Result<AgentOutput> {
        let call_id = format!("mcp_{}", self.tool_name);

        if let Some(ref tx) = ctx.event_tx {
            let _ = tx
                .send(PipelineEvent::ToolCallStart {
                    id: call_id.clone(),
                    name: self.tool_name.clone(),
                    stage: ctx.stage_name.clone(),
                    pipeline: ctx.pipeline_name.clone(),
                })
                .await;
        }

        let params = serde_json::json!({
            "raw_input": ctx.raw_input,
            "outputs": ctx.outputs,
            "state": ctx.state,
            "metadata": ctx.metadata,
        });

        // Tool confirmation gate: check before executing
        if ctx.interrupt_response.is_none() {
            if let Some(confirmation) = self.tools.requires_confirmation(&self.tool_name, &params) {
                let mut interrupt = crate::envelope::FlowInterrupt::new(
                    crate::envelope::InterruptKind::Confirmation,
                ).with_message(confirmation.message.clone());
                if let Some(data) = confirmation.action_data {
                    interrupt = interrupt.with_data(HashMap::from([("action_data".to_string(), data)]));
                }
                return Ok(AgentOutput {
                    output: serde_json::json!({
                        "status": "awaiting_confirmation",
                        "message": &confirmation.message,
                    }),
                    interrupt_request: Some(interrupt),
                    metrics: AgentExecutionMetrics {
                        llm_calls: 0,
                        tool_calls: 0,
                        tokens_in: None,
                        tokens_out: None,
                        duration_ms: 0,
                        tool_results: vec![],
                    },
                    success: true,
                    error_message: String::new(),
                });
            }
        }

        let start = std::time::Instant::now();
        let (result, success, error_message) = match self.tools.execute(&self.tool_name, params).await {
            Ok(tool_output) => (tool_output.data, true, String::new()),
            Err(e) => {
                let err_str = e.to_string();
                if let Some(ref tx) = ctx.event_tx {
                    let _ = tx
                        .send(PipelineEvent::Error {
                            message: err_str.clone(),
                            stage: ctx.stage_name.clone(),
                            pipeline: ctx.pipeline_name.clone(),
                        })
                        .await;
                }
                (serde_json::json!({"error": err_str}), false, err_str)
            }
        };
        let duration_ms = start.elapsed().as_millis() as i64;

        if let Some(ref tx) = ctx.event_tx {
            let _ = tx
                .send(PipelineEvent::ToolResult {
                    id: call_id.clone(),
                    content: result.to_string(),
                    stage: ctx.stage_name.clone(),
                    pipeline: ctx.pipeline_name.clone(),
                })
                .await;
        }

        Ok(AgentOutput {
            output: result,
            metrics: AgentExecutionMetrics {
                llm_calls: 0,
                tool_calls: 1,
                tokens_in: None,
                tokens_out: None,
                duration_ms,
                tool_results: vec![ToolCallResult {
                    name: self.tool_name.clone(),
                    success,
                    latency_ms: duration_ms as u64,
                    error_type: if error_message.is_empty() { None } else { Some(error_message.clone()) },
                }],
            },
            success,
            error_message,
            interrupt_request: None,
        })
    }
}

/// Deterministic (no-LLM) agent — passthrough with empty output.
#[derive(Debug)]
pub struct DeterministicAgent;

#[async_trait]
impl Agent for DeterministicAgent {
    async fn process(&self, _ctx: &AgentContext) -> crate::types::Result<AgentOutput> {
        Ok(AgentOutput {
            output: serde_json::json!({}),
            metrics: AgentExecutionMetrics {
                llm_calls: 0,
                tool_calls: 0,
                tokens_in: None,
                tokens_out: None,
                duration_ms: 0,
                tool_results: vec![],
            },
            success: true,
            error_message: String::new(),
            interrupt_request: None,
        })
    }
}

/// Registry of named agents for pipeline dispatch.
#[derive(Debug, Default)]
pub struct AgentRegistry {
    agents: HashMap<String, Arc<dyn Agent>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: impl Into<String>, agent: Arc<dyn Agent>) {
        self.agents.insert(name.into(), agent);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Agent>> {
        self.agents.get(name)
    }

    /// List all registered agent names.
    pub fn list_names(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Build OpenAI function-calling tool definitions from the registry.
///
/// No filtering needed — the ToolRegistry is already ACL-filtered by AclToolExecutor
/// when `allowed_tools` is set on the pipeline stage.
fn build_tool_defs(tools: &ToolRegistry) -> Vec<serde_json::Value> {
    tools
        .list_all_tools()
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect()
}

/// Build the output JSON value from LLM response content and tool calls.
fn build_output_value(content: &Option<String>, tool_calls: &[ToolCall]) -> serde_json::Value {
    let mut output_map = serde_json::Map::new();

    if let Some(ref content) = content {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(content) {
            for (k, v) in map {
                output_map.insert(k, v);
            }
        } else {
            output_map.insert(
                "response".to_string(),
                serde_json::Value::String(content.clone()),
            );
        }
    }

    if !tool_calls.is_empty() {
        let tc_json: Vec<serde_json::Value> = tool_calls
            .iter()
            .map(|tc| {
                serde_json::json!({
                    "id": tc.id,
                    "name": tc.name,
                    "arguments": tc.arguments,
                })
            })
            .collect();
        output_map.insert("tool_calls".to_string(), serde_json::json!(tc_json));
    }

    serde_json::Value::Object(output_map)
}

fn make_error_output(
    e: crate::types::Error,
    start: Instant,
    llm_calls: i32,
) -> AgentOutput {
    AgentOutput {
        output: serde_json::json!({"error": e.to_string()}),
        metrics: AgentExecutionMetrics {
            llm_calls,
            tool_calls: 0,
            tokens_in: None,
            tokens_out: None,
            duration_ms: start.elapsed().as_millis() as i64,
            tool_results: vec![],
        },
        success: false,
        error_message: e.to_string(),
        interrupt_request: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::llm::mock::MockLlmProvider;

    /// Helper: build an AgentContext with context overflow settings.
    fn ctx_with_overflow(
        raw_input: &str,
        max_tokens: i64,
        overflow: ContextOverflow,
    ) -> AgentContext {
        AgentContext {
            raw_input: raw_input.to_string(),
            outputs: HashMap::new(),
            state: HashMap::new(),
            metadata: HashMap::new(),
            event_tx: None,
            stage_name: Some("test_stage".to_string()),
            pipeline_name: Arc::from("test"),
            max_context_tokens: Some(max_tokens),
            context_overflow: Some(overflow),
            interrupt_response: None,
        }
    }

    // =========================================================================
    // 9a: TruncateOldest context overflow tests
    // =========================================================================

    #[tokio::test]
    async fn test_truncate_oldest_keeps_system_and_drops_intermediate() {
        // max_context_tokens=100, so the char budget is 400 chars.
        // We use a MockLlmProvider that returns a fixed response — this means
        // the ReAct loop runs once. We construct an agent where:
        //   - system prompt is ~200 chars (50 tokens)
        //   - user input is ~200 chars (50 tokens)
        // Total: ~100 tokens — exactly at the limit, no truncation on first call.
        //
        // The MockLlmProvider returns tool calls on the first call, which adds
        // assistant + tool_result messages. On the second iteration of the loop,
        // the messages exceed the token limit and truncation kicks in.
        //
        // Instead, we test with a higher char count: system + user > 400 chars.
        // The mock will return text on first call, so the loop runs once.
        // We verify the agent completes without error (truncation removed
        // intermediate messages until under limit, but with only 2 messages
        // — system + user — and still over limit, it stops dropping).

        // Use a very low token limit: 20 tokens = 80 chars budget.
        // System prompt (from PromptRegistry): "No prompt found for key: test" = 30 chars
        // User input: 200 chars = 50 tokens
        // Total estimated: ~57 tokens > 20 limit → truncation triggered.
        //
        // With TruncateOldest and only 2 messages (system + user), the while
        // loop condition `messages.len() > 2` is false, so it stops. The LLM
        // call proceeds with the over-limit messages (best effort).

        let llm = Arc::new(MockLlmProvider::new(r#"{"response":"ok"}"#));
        let agent = LlmAgent {
            llm,
            prompts: Arc::new(crate::worker::prompts::PromptRegistry::empty()),
            tools: Arc::new(ToolRegistry::new()),
            prompt_key: "test".to_string(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 10,
            content_resolver: None,
        };

        let long_input = "x".repeat(200); // 200 chars = ~50 tokens
        let ctx = ctx_with_overflow(&long_input, 20, ContextOverflow::TruncateOldest);
        let result = agent.process(&ctx).await.unwrap();
        // With TruncateOldest, even if over limit after dropping all intermediates,
        // the agent proceeds (best effort). It should succeed.
        assert!(result.success, "TruncateOldest should allow agent to proceed");
        assert_eq!(result.metrics.llm_calls, 1);
    }

    #[tokio::test]
    async fn test_truncate_oldest_drops_intermediate_messages_in_tool_loop() {
        // This test exercises the truncation logic within the ReAct tool loop.
        // We use SequentialMockLlmProvider to simulate:
        //   1. First LLM call returns a tool call
        //   2. After tool execution, messages accumulate (system + user + assistant + tool_result)
        //   3. On second iteration, total chars exceed budget → truncation drops intermediate
        //   4. Second LLM call returns final text
        use crate::worker::llm::mock::SequentialMockLlmProvider;
        use crate::worker::llm::{ChatResponse, TokenUsage, ToolCall};

        // Tool call response: LLM asks to call "echo" tool
        let tool_response = ChatResponse {
            content: Some("thinking".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "echo".to_string(),
                arguments: serde_json::json!({}),
            }],
            usage: TokenUsage { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 },
            model: "mock".to_string(),
        };
        // Final text response
        let final_response = ChatResponse {
            content: Some(r#"{"response":"done"}"#.to_string()),
            tool_calls: vec![],
            usage: TokenUsage { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 },
            model: "mock".to_string(),
        };

        let llm = Arc::new(SequentialMockLlmProvider::new(vec![tool_response, final_response]));

        // Echo tool executor
        #[derive(Debug)]
        struct EchoExecutor;
        #[async_trait::async_trait]
        impl crate::worker::tools::ToolExecutor for EchoExecutor {
            async fn execute(&self, _name: &str, _params: serde_json::Value) -> crate::types::Result<crate::worker::tools::ToolOutput> {
                // Return a large result to inflate context
                Ok(crate::worker::tools::ToolOutput::json(serde_json::json!({"result": "a]".repeat(100)})))
            }
            fn list_tools(&self) -> Vec<crate::worker::tools::ToolInfo> {
                vec![crate::worker::tools::ToolInfo {
                    name: "echo".to_string(),
                    description: "Echo tool".to_string(),
                    parameters: serde_json::json!({"type": "object"}),
                }]
            }
        }

        let mut tool_reg = ToolRegistry::new();
        tool_reg.register("echo", Arc::new(EchoExecutor));

        let agent = LlmAgent {
            llm,
            prompts: Arc::new(crate::worker::prompts::PromptRegistry::empty()),
            tools: Arc::new(tool_reg),
            prompt_key: "test".to_string(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 5,
            content_resolver: None,
        };

        // Set a low token limit: 50 tokens = 200 chars.
        // After first tool call, messages will be:
        //   system (~30 chars) + user (10 chars) + assistant (~8 chars) + tool_result (~200 chars)
        //   = ~248 chars = ~62 tokens > 50
        // Truncation should kick in on second iteration, dropping index 1 (user message).
        let ctx = ctx_with_overflow("short input", 50, ContextOverflow::TruncateOldest);
        let result = agent.process(&ctx).await.unwrap();

        // The agent should complete successfully (truncation allowed it to proceed)
        assert!(result.success, "Agent should succeed with TruncateOldest truncation");
        // Should have made 2 LLM calls (tool call + final)
        assert_eq!(result.metrics.llm_calls, 2);
        assert_eq!(result.metrics.tool_calls, 1);
    }

    #[tokio::test]
    async fn test_context_overflow_fail_strategy_returns_error() {
        // With Fail strategy and context exceeding limit, agent should return
        // an error output without making the LLM call.
        let llm = Arc::new(MockLlmProvider::new("should not be called"));
        let agent = LlmAgent {
            llm,
            prompts: Arc::new(crate::worker::prompts::PromptRegistry::empty()),
            tools: Arc::new(ToolRegistry::new()),
            prompt_key: "test".to_string(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 10,
            content_resolver: None,
        };

        let long_input = "y".repeat(500); // 500 chars = ~125 tokens
        let ctx = ctx_with_overflow(&long_input, 20, ContextOverflow::Fail);
        let result = agent.process(&ctx).await.unwrap();

        assert!(!result.success, "Fail strategy should report failure");
        assert!(result.error_message.contains("Context overflow"));
        // Should NOT have made any LLM calls
        assert_eq!(result.metrics.llm_calls, 0);
        // Output should contain error details
        let output = &result.output;
        assert_eq!(output["error"], "context_overflow");
    }

    #[tokio::test]
    async fn test_no_context_limit_proceeds_normally() {
        // When max_context_tokens is None, no truncation or failure occurs.
        let llm = Arc::new(MockLlmProvider::new(r#"{"response":"ok"}"#));
        let agent = LlmAgent {
            llm,
            prompts: Arc::new(crate::worker::prompts::PromptRegistry::empty()),
            tools: Arc::new(ToolRegistry::new()),
            prompt_key: "test".to_string(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 10,
            content_resolver: None,
        };

        let ctx = AgentContext {
            raw_input: "x".repeat(10000),
            outputs: HashMap::new(),
            state: HashMap::new(),
            metadata: HashMap::new(),
            event_tx: None,
            stage_name: Some("test".to_string()),
            pipeline_name: Arc::from("test"),
            max_context_tokens: None,
            context_overflow: None,
            interrupt_response: None,
        };

        let result = agent.process(&ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.metrics.llm_calls, 1);
    }
}
