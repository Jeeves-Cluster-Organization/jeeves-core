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

use crate::kernel::orchestrator_types::{AgentExecutionMetrics, ToolCallResult};
use crate::worker::llm::{
    collect_stream, ChatMessage, ChatRequest, LlmProvider, PipelineEvent, ToolCall,
};
use crate::worker::prompts::PromptRegistry;
use crate::worker::tools::{ToolInfo, ToolRegistry};

/// Output from agent execution.
#[derive(Debug, Clone)]
pub struct AgentOutput {
    pub output: serde_json::Value,
    pub metrics: AgentExecutionMetrics,
    pub success: bool,
    pub error_message: String,
}

/// Context provided to an agent for execution.
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub raw_input: String,
    pub outputs: HashMap<String, HashMap<String, serde_json::Value>>,
    pub state: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub allowed_tools: Vec<String>,
    pub event_tx: Option<mpsc::Sender<PipelineEvent>>,
}

/// Agent trait — implementations provide custom agent behavior.
#[async_trait]
pub trait Agent: Send + Sync + std::fmt::Debug {
    async fn process(&self, ctx: &AgentContext) -> crate::types::Result<AgentOutput>;
}

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

        // Build tool definitions filtered by allowed_tools
        let tool_defs = build_tool_defs(&self.tools, &ctx.allowed_tools);

        // Accumulated metrics across all rounds
        let mut total_llm_calls = 0i32;
        let mut total_tool_calls = 0i32;
        let mut total_tokens_in = 0i64;
        let mut total_tokens_out = 0i64;
        let mut last_response = None;
        let mut tool_results: Vec<ToolCallResult> = Vec::new();

        // ReAct tool loop
        for _round in 0..self.max_tool_rounds {
            let req = ChatRequest {
                messages: messages.clone(),
                temperature: self.temperature,
                max_tokens: self.max_tokens,
                model: self.model.clone(),
                tools: if tool_defs.is_empty() { None } else { Some(tool_defs.clone()) },
            };

            // Use streaming path — collect_stream forwards deltas to event_tx if present
            let resp = match self.llm.chat_stream(&req).await {
                Ok(stream) => match collect_stream(stream, ctx.event_tx.as_ref()).await {
                    Ok(resp) => resp,
                    Err(e) => return Ok(make_error_output(e, start, total_llm_calls + 1)),
                },
                Err(e) => return Ok(make_error_output(e, start, total_llm_calls + 1)),
            };

            total_llm_calls += 1;
            total_tokens_in += resp.usage.prompt_tokens;
            total_tokens_out += resp.usage.completion_tokens;

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

                if let Some(ref tx) = ctx.event_tx {
                    let _ = tx
                        .send(PipelineEvent::ToolCallStart {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                        })
                        .await;
                }

                let params: serde_json::Value =
                    serde_json::from_str(&tc.arguments).unwrap_or(serde_json::json!({}));
                let tool_start = Instant::now();
                let (result, tool_success, tool_error) = match self.tools.execute(&tc.name, params).await {
                    Ok(v) => (v.to_string(), true, None),
                    Err(e) => (
                        serde_json::json!({"error": e.to_string()}).to_string(),
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
                            content: result.clone(),
                        })
                        .await;
                }

                messages.push(ChatMessage::tool_result(&tc.id, &result));
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

/// Build OpenAI function-calling tool definitions, filtered by allowed_tools.
fn build_tool_defs(tools: &ToolRegistry, allowed_tools: &[String]) -> Vec<serde_json::Value> {
    let all_tools: Vec<ToolInfo> = tools.list_all_tools();
    let filtered: Vec<&ToolInfo> = if allowed_tools.is_empty() {
        all_tools.iter().collect()
    } else {
        all_tools
            .iter()
            .filter(|t| allowed_tools.contains(&t.name))
            .collect()
    };

    filtered
        .into_iter()
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
    }
}
