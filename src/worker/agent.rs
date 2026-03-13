//! Agent execution — the bridge between kernel instructions and LLM calls.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::kernel::orchestrator_types::AgentExecutionMetrics;
use crate::worker::llm::{ChatMessage, ChatRequest, LlmProvider};
use crate::worker::prompts::PromptRegistry;
use crate::worker::tools::ToolRegistry;

/// Output from agent execution.
#[derive(Debug, Clone)]
pub struct AgentOutput {
    /// JSON output from the agent (stored in envelope.outputs[agent_name]).
    pub output: serde_json::Value,
    /// Execution metrics.
    pub metrics: AgentExecutionMetrics,
    /// Whether execution succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error_message: String,
}

/// Context provided to an agent for execution.
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// Raw user input.
    pub raw_input: String,
    /// Outputs from all prior agents in this pipeline.
    pub outputs: HashMap<String, HashMap<String, serde_json::Value>>,
    /// Merged state accumulator.
    pub state: HashMap<String, serde_json::Value>,
    /// Metadata from the envelope.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Agent trait — implementations provide custom agent behavior.
#[async_trait]
pub trait Agent: Send + Sync + std::fmt::Debug {
    async fn process(&self, ctx: &AgentContext) -> crate::types::Result<AgentOutput>;
}

/// Default LLM-backed agent: load prompt → render → call LLM → return.
#[derive(Debug)]
pub struct LlmAgent {
    pub llm: Arc<dyn LlmProvider>,
    pub prompts: Arc<PromptRegistry>,
    pub tools: Arc<ToolRegistry>,
    pub prompt_key: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i32>,
    pub model: Option<String>,
}

#[async_trait]
impl Agent for LlmAgent {
    async fn process(&self, ctx: &AgentContext) -> crate::types::Result<AgentOutput> {
        let start = Instant::now();

        // Build template vars from context
        let mut vars = HashMap::new();
        vars.insert("raw_input".to_string(), ctx.raw_input.clone());

        // Flatten prior outputs into vars: "{agent}_{key}" = value
        for (agent_name, output) in &ctx.outputs {
            for (key, value) in output {
                let var_key = format!("{}_{}", agent_name, key);
                vars.insert(var_key, value_to_string(value));
            }
        }

        // Flatten state into vars
        for (key, value) in &ctx.state {
            vars.insert(key.clone(), value_to_string(value));
        }

        // Flatten metadata into vars
        for (key, value) in &ctx.metadata {
            vars.insert(key.clone(), value_to_string(value));
        }

        // Render prompt
        let prompt_text = self
            .prompts
            .render(&self.prompt_key, &vars)
            .unwrap_or_else(|| format!("No prompt found for key: {}", self.prompt_key));

        // Build LLM request
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: prompt_text,
            },
            ChatMessage {
                role: "user".to_string(),
                content: ctx.raw_input.clone(),
            },
        ];

        // Add tool definitions if available
        let tool_defs = if !self.tools.list_all_tools().is_empty() {
            Some(
                self.tools
                    .list_all_tools()
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
                    .collect(),
            )
        } else {
            None
        };

        let req = ChatRequest {
            messages,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            model: self.model.clone(),
            tools: tool_defs,
        };

        // Call LLM
        match self.llm.chat(&req).await {
            Ok(resp) => {
                let duration = start.elapsed();

                // Build output
                let mut output_map = serde_json::Map::new();
                if let Some(ref content) = resp.content {
                    // Try to parse as JSON, fall back to string
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content) {
                        if let serde_json::Value::Object(map) = parsed {
                            for (k, v) in map {
                                output_map.insert(k, v);
                            }
                        } else {
                            output_map
                                .insert("response".to_string(), serde_json::Value::String(content.clone()));
                        }
                    } else {
                        output_map
                            .insert("response".to_string(), serde_json::Value::String(content.clone()));
                    }
                }

                if !resp.tool_calls.is_empty() {
                    let tc_json: Vec<serde_json::Value> = resp
                        .tool_calls
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

                Ok(AgentOutput {
                    output: serde_json::Value::Object(output_map),
                    metrics: AgentExecutionMetrics {
                        llm_calls: 1,
                        tool_calls: resp.tool_calls.len() as i32,
                        tokens_in: Some(resp.usage.prompt_tokens),
                        tokens_out: Some(resp.usage.completion_tokens),
                        duration_ms: duration.as_millis() as i64,
                    },
                    success: true,
                    error_message: String::new(),
                })
            }
            Err(e) => {
                let duration = start.elapsed();
                Ok(AgentOutput {
                    output: serde_json::json!({"error": e.to_string()}),
                    metrics: AgentExecutionMetrics {
                        llm_calls: 1,
                        tool_calls: 0,
                        tokens_in: None,
                        tokens_out: None,
                        duration_ms: duration.as_millis() as i64,
                    },
                    success: false,
                    error_message: e.to_string(),
                })
            }
        }
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

    /// Register a named agent.
    pub fn register(&mut self, name: impl Into<String>, agent: Arc<dyn Agent>) {
        self.agents.insert(name.into(), agent);
    }

    /// Get an agent by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Agent>> {
        self.agents.get(name)
    }
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
