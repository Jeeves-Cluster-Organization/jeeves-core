//! `Stage` (one position in the workflow) and `AgentConfig`
//! (LLM/agent-side settings, flattened into the stage on the wire).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::policy::{ContextOverflow, RetryPolicy};

/// Pipeline stage. Routing per stage evaluates in this order:
/// 1. agent failed + `error_next` set → `error_next`.
/// 2. `routing_fn` registered → call it.
/// 3. `default_next` → that stage.
/// 4. otherwise → terminate `Completed`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Stage {
    /// Unique within the pipeline.
    pub name: String,
    /// Agent name to dispatch.
    pub agent: String,
    /// Name of a registered routing function. Called after agent completion
    /// to determine the next stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_fn: Option<String>,
    /// Fallback target when no routing function is set or it returns Terminate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_next: Option<String>,
    /// Target stage when the agent fails (checked before `routing_fn`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_next: Option<String>,
    /// Per-stage visit limit. Terminates with `MaxStageVisitsExceeded` when reached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_visits: Option<i32>,
    /// Verbatim hint forwarded to the LLM provider for grammar-constrained
    /// generation. The kernel does not interpret it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
    /// State field key for this stage's output (defaults to stage name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_key: Option<String>,
    /// Maximum estimated tokens allowed in LLM context for this stage.
    /// Uses chars/4 heuristic. When exceeded, applies `context_overflow`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<i64>,
    #[serde(default)]
    pub context_overflow: ContextOverflow,
    /// Per-stage wall-clock timeout in seconds. Agent execution is cancelled
    /// if it exceeds this deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Retry policy for transient agent failures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    /// Agent execution config — transparent to kernel, consumed by worker.
    #[serde(flatten)]
    pub agent_config: AgentConfig,
}

/// LLM / agent-side settings attached to a stage. Flattened into the stage on
/// the wire so a pipeline JSON looks like one flat record per stage.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentConfig {
    /// Prompt template key for this agent. None = deterministic (no LLM call).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_key: Option<String>,
    /// Whether this agent makes LLM calls (default: false — explicit opt-in).
    #[serde(default = "default_has_llm")]
    pub has_llm: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    /// Model role (e.g. "fast", "reasoning") — resolved by the LLM provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_role: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            has_llm: false,
            prompt_key: None,
            temperature: None,
            max_tokens: None,
            model_role: None,
        }
    }
}

fn default_has_llm() -> bool {
    false
}
