//! Agent execution layer. `LlmAgent` runs a ReAct tool loop; streaming is
//! opt-in via `AgentContext.event_tx`.

pub mod factory;
pub mod hooks;
pub mod llm;
pub mod prompts;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

use tracing::instrument;

use crate::agent::llm::{
    collect_stream, ChatMessage, ChatRequest, LlmProvider, MessageContent, RunEvent, ToolCall,
};
use crate::agent::prompts::PromptRegistry;
use crate::workflow::{ContextOverflow};
use crate::kernel::protocol::{AgentExecutionMetrics, ToolCallResult};
use crate::tools::{ContentPart, ContentResolver, ToolRegistry};

#[must_use]
#[derive(Debug, Clone)]
pub struct AgentOutput {
    pub output: serde_json::Value,
    pub metrics: AgentExecutionMetrics,
    pub success: bool,
    pub error_message: String,
    /// Set when the agent wants the worker to suspend on a flow interrupt
    /// (e.g. tool confirmation). The worker stores it on the run.
    pub interrupt_request: Option<crate::run::FlowInterrupt>,
}

#[derive(Debug, Clone)]
pub struct AgentContext {
    pub raw_input: String,
    /// Outputs from prior stages: `output_key → field → value`.
    pub outputs: HashMap<String, HashMap<String, serde_json::Value>>,
    /// State fields preserved across pipeline loop-backs (`state_schema`).
    pub state: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, serde_json::Value>,
    /// `None` = buffered execution; `Some` = stream events back to consumer.
    pub event_tx: Option<mpsc::Sender<RunEvent>>,
    pub stage_name: Option<String>,
    pub workflow_name: Arc<str>,
    /// chars/4 heuristic; checked at the top of every ReAct round.
    pub max_context_tokens: Option<i64>,
    pub context_overflow: Option<ContextOverflow>,
    /// Set by the worker on resume after a tool-confirmation interrupt.
    pub interrupt_response: Option<serde_json::Value>,
    /// Verbatim LLM-provider hint forwarded as-is; kernel does not parse it.
    pub response_format: Option<serde_json::Value>,
}

#[async_trait]
pub trait Agent: Send + Sync + std::fmt::Debug + std::any::Any {
    /// `Err` is infrastructure failure; agent-level failure is `success: false`.
    async fn process(&self, ctx: &AgentContext) -> crate::types::Result<AgentOutput>;
}

pub const DEFAULT_MAX_TOOL_ROUNDS: u32 = 10;

#[derive(Debug)]
pub struct LlmAgent {
    pub llm: Arc<dyn LlmProvider>,
    pub prompts: Arc<PromptRegistry>,
    pub tools: Arc<ToolRegistry>,
    /// Identity used by `ToolRegistry::execute_for` to consult `ToolAccessPolicy`.
    pub agent_name: crate::types::AgentName,
    pub prompt_key: crate::types::PromptKey,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i32>,
    pub model: Option<String>,
    pub max_tool_rounds: u32,
    pub content_resolver: Option<Arc<dyn ContentResolver>>,
    /// Hooks run in registration order; the first non-`Continue` decision wins.
    pub hooks: Vec<crate::agent::hooks::DynHook>,
}

impl Default for LlmAgent {
    fn default() -> Self {
        Self {
            llm: Arc::new(crate::agent::llm::mock::MockLlmProvider::default()),
            prompts: Arc::new(PromptRegistry::empty()),
            tools: Arc::new(ToolRegistry::new()),
            agent_name: crate::types::AgentName::default(),
            prompt_key: crate::types::PromptKey::default(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 10,
            content_resolver: None,
            hooks: Vec::new(),
        }
    }
}

#[async_trait]
impl Agent for LlmAgent {
    #[instrument(skip(self, ctx), fields(prompt_key = %self.prompt_key))]
    async fn process(&self, ctx: &AgentContext) -> crate::types::Result<AgentOutput> {
        let start = Instant::now();

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

        let prompt_text = self
            .prompts
            .render(self.prompt_key.as_str(), &vars)
            .unwrap_or_else(|| format!("No prompt found for key: {}", self.prompt_key));

        let mut messages = vec![
            ChatMessage::system(prompt_text),
            ChatMessage::user(ctx.raw_input.clone()),
        ];

        let tool_defs = Arc::new(build_tool_defs(&self.tools));

        let mut total_llm_calls = 0i32;
        let mut total_tool_calls = 0i32;
        let mut total_tokens_in = 0i64;
        let mut total_tokens_out = 0i64;
        let mut last_response = None;
        let mut tool_results: Vec<ToolCallResult> = Vec::new();

        for _round in 0..self.max_tool_rounds {
            if let Some(max_tokens) = ctx.max_context_tokens {
                let estimated_tokens: i64 = messages.iter()
                    .map(|m| (m.content.len() / 4) as i64)
                    .sum();

                if estimated_tokens > max_tokens {
                    let overflow_strategy = ctx.context_overflow.unwrap_or_default();

                    match overflow_strategy {
                        ContextOverflow::TruncateOldest => {
                            // Preserve system prompt (idx 0) and the trailing user/tool
                            // message; drop oldest intermediates until under the limit.
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

            for hook in &self.hooks {
                hook.before_llm_call(&mut messages).await;
            }

            let req = ChatRequest {
                messages: messages.clone(),
                temperature: self.temperature,
                max_tokens: self.max_tokens,
                model: self.model.clone(),
                tools: if tool_defs.is_empty() { None } else { Some(tool_defs.clone()) },
                response_format: ctx.response_format.clone(),
            };

            let resp = match self.llm.chat_stream(&req).await {
                Ok(stream) => match collect_stream(stream, ctx.event_tx.as_ref(), ctx.stage_name.as_deref(), ctx.workflow_name.clone()).await {
                    Ok(resp) => resp,
                    Err(e) => return Ok(make_error_output(e, start, total_llm_calls + 1)),
                },
                Err(e) => return Ok(make_error_output(e, start, total_llm_calls + 1)),
            };

            total_llm_calls += 1;
            total_tokens_in += resp.usage.prompt_tokens as i64;
            total_tokens_out += resp.usage.completion_tokens as i64;

            if resp.tool_calls.is_empty() {
                last_response = Some(resp);
                break;
            }

            tracing::debug!(round = _round, tool_count = resp.tool_calls.len(), "tool_loop_round");

            let content = resp.content.clone().unwrap_or_default();
            messages.push(ChatMessage::assistant(content, Some(resp.tool_calls.clone())));

            for tc in &resp.tool_calls {
                total_tool_calls += 1;
                tracing::debug!(tool_name = %tc.name, "tool_execute");

                if ctx.interrupt_response.is_none() {
                    if let Some(confirmation) = self.tools.requires_confirmation(&tc.name, &tc.arguments) {
                        let mut interrupt = crate::run::FlowInterrupt::new()
                            .with_message(confirmation.message.clone());
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
                        .send(RunEvent::ToolCallStart {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            stage: ctx.stage_name.clone(),
                            pipeline: ctx.workflow_name.clone(),
                        })
                        .await;
                }

                let mut hook_decision: Option<crate::agent::hooks::HookDecision> = None;
                for hook in &self.hooks {
                    let d = hook.before_tool_call(tc).await;
                    if !matches!(d, crate::agent::hooks::HookDecision::Continue) {
                        hook_decision = Some(d);
                        break;
                    }
                }

                let params = tc.arguments.clone();
                let tool_start = Instant::now();
                let (mut hook_short_circuit_output, exec_result) = match hook_decision {
                    Some(crate::agent::hooks::HookDecision::Replace(out)) => (Some(out), None),
                    Some(crate::agent::hooks::HookDecision::Block { reason }) => (
                        Some(crate::tools::ToolOutput::json(serde_json::json!({
                            "error": "tool_call_blocked",
                            "reason": reason,
                        }))),
                        None,
                    ),
                    _ => (None, Some(self.tools.execute_for(self.agent_name.as_str(), &tc.name, params).await)),
                };

                let (result_text, result_content, tool_success, tool_error) = if let Some(ref mut output) = hook_short_circuit_output {
                    // after_tool_call still fires on Replace/Block — audit/redact use cases.
                    for hook in &self.hooks {
                        hook.after_tool_call(tc, output).await;
                    }
                    let text = output.data.to_string();
                    let content = if output.content.is_empty() {
                        None
                    } else {
                        Some(self.resolve_content_parts(&output.content))
                    };
                    (text, content, true, None)
                } else {
                    match exec_result.unwrap() {
                        Ok(mut tool_output) => {
                            for hook in &self.hooks {
                                hook.after_tool_call(tc, &mut tool_output).await;
                            }
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
                    }
                };
                tool_results.push(ToolCallResult {
                    name: tc.name.clone(),
                    success: tool_success,
                    latency_ms: tool_start.elapsed().as_millis() as u64,
                    error_type: tool_error,
                });

                if let Some(ref tx) = ctx.event_tx {
                    let _ = tx
                        .send(RunEvent::ToolResult {
                            id: tc.id.clone(),
                            content: result_text.clone(),
                            stage: ctx.stage_name.clone(),
                            pipeline: ctx.workflow_name.clone(),
                        })
                        .await;
                }

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
    /// Resolves `ContentPart::Ref`; unresolved refs degrade to a text placeholder
    /// so the conversation continues instead of erroring.
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
                ContentPart::Text {
                    text: format!("[content ref: {} ({})]", ref_id, content_type),
                }
            }
            other => other.clone(),
        }).collect()
    }
}

/// Non-LLM stage agent that forwards the call to a named `ToolExecutor`.
///
/// `agent_name == tool_name` is the usual convention, but the fields are kept
/// separate so the policy-chain identity is unambiguous.
#[derive(Debug)]
pub struct ToolDelegatingAgent {
    pub agent_name: crate::types::AgentName,
    pub tool_name: crate::types::ToolName,
    pub tools: Arc<ToolRegistry>,
}

#[async_trait]
impl Agent for ToolDelegatingAgent {
    #[instrument(skip(self, ctx), fields(tool = %self.tool_name))]
    async fn process(&self, ctx: &AgentContext) -> crate::types::Result<AgentOutput> {
        let call_id = format!("mcp_{}", self.tool_name);

        if let Some(ref tx) = ctx.event_tx {
            let _ = tx
                .send(RunEvent::ToolCallStart {
                    id: call_id.clone(),
                    name: self.tool_name.as_str().to_string(),
                    stage: ctx.stage_name.clone(),
                    pipeline: ctx.workflow_name.clone(),
                })
                .await;
        }

        let params = serde_json::json!({
            "raw_input": ctx.raw_input,
            "outputs": ctx.outputs,
            "state": ctx.state,
            "metadata": ctx.metadata,
        });

        if ctx.interrupt_response.is_none() {
            if let Some(confirmation) = self.tools.requires_confirmation(self.tool_name.as_str(), &params) {
                let mut interrupt = crate::run::FlowInterrupt::new()
                    .with_message(confirmation.message.clone());
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
        let (result, success, error_message) = match self.tools.execute_for(self.agent_name.as_str(), self.tool_name.as_str(), params).await {
            Ok(tool_output) => (tool_output.data, true, String::new()),
            Err(e) => {
                let err_str = e.to_string();
                if let Some(ref tx) = ctx.event_tx {
                    let _ = tx
                        .send(RunEvent::Error {
                            message: err_str.clone(),
                            stage: ctx.stage_name.clone(),
                            pipeline: ctx.workflow_name.clone(),
                        })
                        .await;
                }
                (serde_json::json!({"error": err_str}), false, err_str)
            }
        };
        let duration_ms = start.elapsed().as_millis() as i64;

        if let Some(ref tx) = ctx.event_tx {
            let _ = tx
                .send(RunEvent::ToolResult {
                    id: call_id.clone(),
                    content: result.to_string(),
                    stage: ctx.stage_name.clone(),
                    pipeline: ctx.workflow_name.clone(),
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
                    name: self.tool_name.as_str().to_string(),
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

/// Passthrough agent with empty output; used when a stage has no LLM and no
/// matching tool.
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

#[derive(Debug, Default)]
pub struct AgentRegistry {
    agents: HashMap<crate::types::AgentName, Arc<dyn Agent>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: impl Into<crate::types::AgentName>, agent: Arc<dyn Agent>) {
        self.agents.insert(name.into(), agent);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Agent>> {
        self.agents.get(name)
    }

    pub fn list_names(&self) -> Vec<crate::types::AgentName> {
        self.agents.keys().cloned().collect()
    }
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Builds OpenAI function-calling tool defs. No filtering here — the registry
/// has already been ACL-wrapped by `AgentFactoryBuilder`.
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
    use crate::agent::llm::mock::MockLlmProvider;

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
            workflow_name: Arc::from("test"),
            max_context_tokens: Some(max_tokens),
            context_overflow: Some(overflow),
            interrupt_response: None,
            response_format: None,
        }
    }

    #[tokio::test]
    async fn test_truncate_oldest_proceeds_when_only_two_messages_remain() {
        // With only system + user messages, the truncation loop has nothing to
        // drop, so it falls through to the LLM call (best-effort) — the agent
        // should still report success.
        let llm = Arc::new(MockLlmProvider::new(r#"{"response":"ok"}"#));
        let agent = LlmAgent {
            llm,
            prompts: Arc::new(crate::agent::prompts::PromptRegistry::empty()),
            tools: Arc::new(ToolRegistry::new()),
            agent_name: "test".into(),
            prompt_key: "test".into(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 10,
            content_resolver: None,
            hooks: Vec::new(),
        };

        let long_input = "x".repeat(200);
        let ctx = ctx_with_overflow(&long_input, 20, ContextOverflow::TruncateOldest);
        let result = agent.process(&ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.metrics.llm_calls, 1);
    }

    #[tokio::test]
    async fn test_truncate_oldest_drops_intermediate_messages_in_tool_loop() {
        // Tool result is large enough that the second ReAct round triggers
        // truncation; the agent must still complete both LLM calls.
        use crate::agent::llm::mock::SequentialMockLlmProvider;
        use crate::agent::llm::{ChatResponse, TokenUsage, ToolCall};

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
        let final_response = ChatResponse {
            content: Some(r#"{"response":"done"}"#.to_string()),
            tool_calls: vec![],
            usage: TokenUsage { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 },
            model: "mock".to_string(),
        };

        let llm = Arc::new(SequentialMockLlmProvider::new(vec![tool_response, final_response]));

        #[derive(Debug)]
        struct EchoExecutor;
        #[async_trait::async_trait]
        impl crate::tools::ToolExecutor for EchoExecutor {
            async fn execute(&self, _name: &str, _params: serde_json::Value) -> crate::types::Result<crate::tools::ToolOutput> {
                Ok(crate::tools::ToolOutput::json(serde_json::json!({"result": "a]".repeat(100)})))
            }
            fn list_tools(&self) -> Vec<crate::tools::ToolInfo> {
                vec![crate::tools::ToolInfo {
                    name: "echo".into(),
                    description: "Echo tool".to_string(),
                    parameters: serde_json::json!({"type": "object"}),
                }]
            }
        }

        let mut tool_reg = ToolRegistry::new();
        tool_reg.register("echo", Arc::new(EchoExecutor));

        let agent = LlmAgent {
            llm,
            prompts: Arc::new(crate::agent::prompts::PromptRegistry::empty()),
            tools: Arc::new(tool_reg),
            agent_name: "test".into(),
            prompt_key: "test".into(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 5,
            content_resolver: None,
            hooks: Vec::new(),
        };

        let ctx = ctx_with_overflow("short input", 50, ContextOverflow::TruncateOldest);
        let result = agent.process(&ctx).await.unwrap();

        assert!(result.success);
        assert_eq!(result.metrics.llm_calls, 2);
        assert_eq!(result.metrics.tool_calls, 1);
    }

    #[tokio::test]
    async fn test_context_overflow_fail_strategy_returns_error() {
        let llm = Arc::new(MockLlmProvider::new("should not be called"));
        let agent = LlmAgent {
            llm,
            prompts: Arc::new(crate::agent::prompts::PromptRegistry::empty()),
            tools: Arc::new(ToolRegistry::new()),
            agent_name: "test".into(),
            prompt_key: "test".into(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 10,
            content_resolver: None,
            hooks: Vec::new(),
        };

        let long_input = "y".repeat(500);
        let ctx = ctx_with_overflow(&long_input, 20, ContextOverflow::Fail);
        let result = agent.process(&ctx).await.unwrap();

        assert!(!result.success);
        assert!(result.error_message.contains("Context overflow"));
        assert_eq!(result.metrics.llm_calls, 0);
        assert_eq!(result.output["error"], "context_overflow");
    }

    #[tokio::test]
    async fn test_no_context_limit_proceeds_normally() {
        let llm = Arc::new(MockLlmProvider::new(r#"{"response":"ok"}"#));
        let agent = LlmAgent {
            llm,
            prompts: Arc::new(crate::agent::prompts::PromptRegistry::empty()),
            tools: Arc::new(ToolRegistry::new()),
            agent_name: "test".into(),
            prompt_key: "test".into(),
            temperature: None,
            max_tokens: None,
            model: None,
            max_tool_rounds: 10,
            content_resolver: None,
            hooks: Vec::new(),
        };

        let ctx = AgentContext {
            raw_input: "x".repeat(10000),
            outputs: HashMap::new(),
            state: HashMap::new(),
            metadata: HashMap::new(),
            event_tx: None,
            stage_name: Some("test".to_string()),
            workflow_name: Arc::from("test"),
            max_context_tokens: None,
            context_overflow: None,
            interrupt_response: None,
            response_format: None,
        };

        let result = agent.process(&ctx).await.unwrap();
        assert!(result.success);
        assert_eq!(result.metrics.llm_calls, 1);
    }
}
