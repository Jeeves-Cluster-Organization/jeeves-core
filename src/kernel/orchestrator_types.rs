//! Pipeline orchestration types — config, instructions, metrics, session state.
//!
//! These types are shared between the Orchestrator and Worker modules.

use crate::envelope::{FlowInterrupt, TerminalReason};
use crate::types::{Error, ProcessId, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};


// =============================================================================
// Graph Primitive Types
// =============================================================================

/// Node kind in the pipeline graph.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, JsonSchema)]
pub enum NodeKind {
    /// Standard agent execution node
    #[default]
    Agent,
    /// Pure routing node — evaluates routing without running an agent
    Gate,
    /// Parallel fan-out node — routing function returns Fan(targets)
    Fork,
}

/// Merge strategy for state fields.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, JsonSchema)]
pub enum MergeStrategy {
    /// Overwrite the existing value
    #[default]
    Replace,
    /// Append to an array (creates array if not present)
    Append,
    /// Shallow-merge object keys
    MergeDict,
}

/// Strategy when LLM context exceeds max_context_tokens.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, JsonSchema)]
pub enum ContextOverflow {
    /// Terminate with error (safest default)
    #[default]
    Fail,
    /// Drop oldest non-system messages until under limit
    TruncateOldest,
}

/// Schema entry for a state field.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StateField {
    pub key: String,
    pub merge: MergeStrategy,
}

// =============================================================================
// Instruction Types
// =============================================================================

/// Context attached by the kernel enrichment layer (post-orchestrator).
/// Starts as Default (all None), populated in kernel_orchestration.rs get_next_instruction().
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentDispatchContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_context: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_overflow: Option<ContextOverflow>,
    /// Response from a resolved interrupt, if this dispatch is a resume after confirmation.
    /// Extracted from envelope metadata by `get_next_instruction()` (not leaked to agents via metadata).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt_response: Option<serde_json::Value>,
    /// Per-stage wall-clock timeout (passed from PipelineStage config).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Retry policy for transient failures (passed from PipelineStage config).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    /// Routing decision that led to this instruction (for audit trail events).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_routing_decision: Option<super::routing::RoutingDecision>,
}

/// Instruction from kernel to worker — determines what happens next.
///
/// Returned by `KernelHandle::get_next_instruction()` after each agent completes.
#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Instruction {
    /// Dispatch a single agent for execution.
    RunAgent {
        agent: String,
        #[serde(flatten)]
        context: AgentDispatchContext,
    },
    /// Dispatch multiple agents in parallel (emitted by Fork nodes).
    RunAgents {
        agents: Vec<String>,
        #[serde(flatten)]
        context: AgentDispatchContext,
    },
    /// Stop the pipeline with the given reason.
    Terminate {
        reason: TerminalReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(flatten)]
        context: AgentDispatchContext,
    },
    /// Wait for all parallel agents to complete (Fork join point).
    WaitParallel,
    WaitInterrupt {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interrupt: Option<FlowInterrupt>,
    },
}

impl Instruction {
    /// Create a Terminate instruction.
    pub fn terminate(reason: TerminalReason, message: impl Into<String>) -> Self {
        Self::Terminate { reason, message: Some(message.into()), context: Default::default() }
    }

    /// Create a Terminate instruction with reason only (no message).
    pub fn terminate_completed() -> Self {
        Self::Terminate { reason: TerminalReason::Completed, message: None, context: Default::default() }
    }

    /// Create a RunAgent instruction for a single agent.
    pub fn run_agent(name: impl Into<String>) -> Self {
        Self::RunAgent { agent: name.into(), context: Default::default() }
    }

    /// Create a RunAgents instruction for parallel fan-out.
    pub fn run_agents(names: Vec<String>) -> Self {
        Self::RunAgents { agents: names, context: Default::default() }
    }

    /// Create a WaitParallel instruction.
    pub fn wait_parallel() -> Self { Self::WaitParallel }

    /// Create a WaitInterrupt instruction.
    pub fn wait_interrupt(interrupt: FlowInterrupt) -> Self {
        Self::WaitInterrupt { interrupt: Some(interrupt) }
    }
}

/// Per-tool-call result reported by the agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub name: String,
    pub success: bool,
    pub latency_ms: u64,
    pub error_type: Option<String>,
}

/// AgentExecutionMetrics contains metrics from agent execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentExecutionMetrics {
    pub llm_calls: i32,
    pub tool_calls: i32,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub duration_ms: i64,
    #[serde(default)]
    pub tool_results: Vec<ToolCallResult>,
}

// =============================================================================
// Pipeline Config
// =============================================================================

/// Pipeline configuration for orchestration.
///
/// The kernel dispatches registered routing functions to determine transitions.
/// All pipeline shape (linear, branching, cycles) is defined by the capability
/// layer via routing functions and default_next on each stage.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PipelineConfig {
    /// Unique pipeline name (used for event attribution and child pipeline references).
    pub name: String,
    /// Ordered list of pipeline stages. First stage is the entry point.
    pub stages: Vec<PipelineStage>,
    /// Global iteration bound. Terminates with `MaxIterationsExceeded` when reached.
    pub max_iterations: i32,
    /// Global LLM call bound across all stages. Terminates with `MaxLlmCallsExceeded`.
    pub max_llm_calls: i32,
    /// Global agent hop bound (transitions between stages). Terminates with `MaxAgentHopsExceeded`.
    pub max_agent_hops: i32,
    /// Per-edge transition limits (e.g. max 3 trips from stage A→B).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_limits: Vec<EdgeLimit>,
    /// Maximum routing steps per `get_next_instruction` call (default: 100).
    /// Prevents infinite Gate/Fork chains from blocking the orchestration loop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_limit: Option<i32>,
    /// Typed state fields with merge strategies for loop-back accumulation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_schema: Vec<StateField>,
    /// Event types this pipeline subscribes to via CommBus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscriptions: Vec<String>,
    /// Event types this pipeline publishes via CommBus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publishes: Vec<String>,
}

impl PipelineConfig {
    /// Get stage order from pipeline.
    pub fn get_stage_order(&self) -> Vec<String> {
        self.stages.iter().map(|s| s.name.clone()).collect()
    }

    /// Validate pipeline configuration.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(Error::validation("Pipeline name is required"));
        }
        if self.stages.is_empty() {
            return Err(Error::validation("Pipeline must have at least one stage"));
        }

        // Validate bounds are positive (fail fast on pipeline-level config)
        if self.max_iterations <= 0 {
            return Err(Error::validation(format!(
                "max_iterations must be > 0, got {}", self.max_iterations
            )));
        }
        if self.max_llm_calls <= 0 {
            return Err(Error::validation(format!(
                "max_llm_calls must be > 0, got {}", self.max_llm_calls
            )));
        }
        if self.max_agent_hops <= 0 {
            return Err(Error::validation(format!(
                "max_agent_hops must be > 0, got {}", self.max_agent_hops
            )));
        }
        if let Some(sl) = self.step_limit {
            if sl <= 0 {
                return Err(Error::validation(format!(
                    "step_limit must be > 0, got {}", sl
                )));
            }
        }

        // Validate stage names: non-empty and unique
        let mut stage_names: HashSet<&str> = HashSet::new();
        let mut output_keys: HashSet<&str> = HashSet::new();
        for stage in &self.stages {
            if stage.name.is_empty() {
                return Err(Error::validation("Stage name must not be empty"));
            }
            if !stage_names.insert(stage.name.as_str()) {
                return Err(Error::validation(format!(
                    "Duplicate stage name '{}'", stage.name
                )));
            }
            // Track explicit output_keys for uniqueness
            if let Some(ref ok) = stage.output_key {
                if !output_keys.insert(ok.as_str()) {
                    return Err(Error::validation(format!(
                        "Duplicate output_key '{}' on stage '{}'", ok, stage.name
                    )));
                }
            }
        }

        for stage in &self.stages {
            // Validate node-kind constraints
            match stage.node_kind {
                NodeKind::Agent => {
                    if stage.agent.is_empty() {
                        return Err(Error::validation(format!(
                            "Agent stage '{}' must have a non-empty agent field", stage.name
                        )));
                    }
                }
                NodeKind::Gate => {
                    // Gate stages route without running an agent.
                    // Must have routing_fn or default_next to be useful.
                }
                NodeKind::Fork => {
                    if stage.routing_fn.is_none() {
                        return Err(Error::validation(format!(
                            "Fork stage '{}' must have a routing_fn", stage.name
                        )));
                    }
                }
            }

            // Detect unbounded self-loops
            if let Some(ref dn) = stage.default_next {
                if dn == &stage.name && stage.max_visits.is_none() {
                    return Err(Error::validation(format!(
                        "Stage '{}' has default_next pointing to itself without max_visits (infinite loop)",
                        stage.name
                    )));
                }
            }

            // Validate default_next references existing stage
            if let Some(ref dn) = stage.default_next {
                if !stage_names.contains(dn.as_str()) {
                    return Err(Error::validation(format!(
                        "Stage '{}' has default_next '{}' which does not exist in pipeline",
                        stage.name, dn
                    )));
                }
            }

            // Validate error_next references existing stage
            if let Some(ref en) = stage.error_next {
                if !stage_names.contains(en.as_str()) {
                    return Err(Error::validation(format!(
                        "Stage '{}' has error_next '{}' which does not exist in pipeline",
                        stage.name, en
                    )));
                }
            }

            // Validate max_visits is positive
            if let Some(mv) = stage.max_visits {
                if mv <= 0 {
                    return Err(Error::validation(format!(
                        "Stage '{}' has max_visits {} which must be positive",
                        stage.name, mv
                    )));
                }
            }
            // Validate max_context_tokens is positive
            if let Some(mct) = stage.max_context_tokens {
                if mct <= 0 {
                    return Err(Error::validation(format!(
                        "Stage '{}' has max_context_tokens {} which must be positive",
                        stage.name, mct
                    )));
                }
            }
        }

        // Validate edge limits reference existing stages
        for limit in &self.edge_limits {
            if !stage_names.contains(limit.from_stage.as_str()) {
                return Err(Error::validation(format!(
                    "Edge limit from_stage '{}' does not exist", limit.from_stage
                )));
            }
            if !stage_names.contains(limit.to_stage.as_str()) {
                return Err(Error::validation(format!(
                    "Edge limit to_stage '{}' does not exist", limit.to_stage
                )));
            }
            if limit.max_count <= 0 {
                return Err(Error::validation(format!(
                    "Edge limit {}->{} has max_count {} which must be > 0",
                    limit.from_stage, limit.to_stage, limit.max_count
                )));
            }
        }

        // Validate state_schema: no duplicate keys
        let mut state_keys: HashSet<&str> = HashSet::new();
        for field in &self.state_schema {
            if !state_keys.insert(field.key.as_str()) {
                return Err(Error::validation(format!(
                    "Duplicate state_schema key '{}'", field.key
                )));
            }
        }

        Ok(())
    }

    /// Test-only minimal config constructor. Avoids 7-field boilerplate.
    #[cfg(test)]
    pub fn test_default(name: &str, stages: Vec<PipelineStage>) -> Self {
        Self {
            name: name.to_string(),
            stages,
            max_iterations: 10,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
            subscriptions: vec![],
            publishes: vec![],
        }
    }
}

/// Agent execution configuration — transparent to kernel, consumed by worker.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentConfig {
    /// Prompt template key for this agent. None = deterministic (no LLM call).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_key: Option<String>,
    /// Whether this agent makes LLM calls (default: false — require explicit opt-in).
    #[serde(default = "default_has_llm")]
    pub has_llm: bool,
    /// LLM temperature override for this stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// LLM max_tokens override for this stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    /// Model role (e.g. "fast", "reasoning") — resolved by LLM provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_role: Option<String>,
    /// Maximum tool call rounds in the ReAct loop (default: 10).
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            has_llm: false,
            prompt_key: None,
            temperature: None,
            max_tokens: None,
            model_role: None,
            max_tool_rounds: 10,
        }
    }
}

/// Retry policy for transient agent failures (Temporal activity retry pattern).
///
/// When an agent fails with `success: false`, the worker retries with exponential
/// backoff before routing to `error_next`. No retry on interrupt requests.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RetryPolicy {
    /// Maximum retry attempts (0 = no retry, default).
    #[serde(default)]
    pub max_retries: u32,
    /// Initial backoff in milliseconds (default: 1000).
    #[serde(default = "default_initial_backoff_ms")]
    pub initial_backoff_ms: u64,
    /// Maximum backoff in milliseconds (default: 30000).
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,
    /// Backoff multiplier (default: 2.0).
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 0,
            initial_backoff_ms: 1000,
            max_backoff_ms: 30000,
            backoff_multiplier: 2.0,
        }
    }
}

fn default_initial_backoff_ms() -> u64 { 1000 }
fn default_max_backoff_ms() -> u64 { 30000 }
fn default_backoff_multiplier() -> f64 { 2.0 }

/// Pipeline stage with routing, fallbacks, and parallel config.
///
/// Routing evaluation order:
/// 1. If agent failed AND error_next set → route to error_next
/// 2. If routing_fn registered → call it, use result
/// 3. If no routing_fn → fall through to default_next
/// 4. If no default_next → terminate (COMPLETED)
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PipelineStage {
    /// Stage identifier (must be unique within the pipeline).
    pub name: String,
    /// Agent name to dispatch (ignored for Gate nodes).
    pub agent: String,
    /// Name of a registered routing function. Called after agent completion
    /// (or directly for Gate nodes) to determine the next stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_fn: Option<String>,
    /// Fallback target when no routing function is set or it returns Terminate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_next: Option<String>,
    /// Target stage when the agent fails (checked before routing_fn).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_next: Option<String>,
    /// Per-stage visit limit. Terminates with `MaxStageVisitsExceeded` when reached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_visits: Option<i32>,
    /// How to join parallel branches from a Fork (WaitAll or WaitFirst).
    #[serde(default)]
    pub join_strategy: JoinStrategy,
    /// JSON Schema for validating agent output before writing to state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Tool whitelist — only these tools are available to this stage's agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    /// Node kind: Agent (run agent + route), Gate (route only), Fork (parallel fan-out).
    #[serde(default)]
    pub node_kind: NodeKind,
    /// State field key for this stage's output (defaults to stage name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_key: Option<String>,
    /// Maximum estimated tokens allowed in LLM context for this stage.
    /// Uses chars/4 heuristic. When exceeded, applies context_overflow strategy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<i64>,
    /// Strategy when context exceeds max_context_tokens.
    #[serde(default)]
    pub context_overflow: ContextOverflow,
    /// Per-stage wall-clock timeout in seconds (K8s container timeout / Temporal startToClose).
    /// Agent execution is cancelled if it exceeds this deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Retry policy for transient agent failures. Retries with backoff before
    /// falling through to error_next routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    /// Agent execution config — transparent to kernel, consumed by worker.
    #[serde(flatten)]
    pub agent_config: AgentConfig,
}

fn default_has_llm() -> bool {
    false
}

fn default_max_tool_rounds() -> u32 {
    10
}

/// Join strategy for parallel execution groups.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, JsonSchema)]
pub enum JoinStrategy {
    /// Wait for all agents to complete
    #[default]
    WaitAll,
    /// Advance when the first agent completes
    WaitFirst,
}

/// Per-edge transition limit.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EdgeLimit {
    pub from_stage: String,
    pub to_stage: String,
    pub max_count: i32,
}

/// Type-safe key for edge traversal tracking.
///
/// Replaces string concatenation `"from->to"` to eliminate collision risk
/// (e.g., stage names containing "->").
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct EdgeKey {
    pub from_stage: String,
    pub to_stage: String,
}

impl EdgeKey {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self { from_stage: from.into(), to_stage: to.into() }
    }
}

impl std::fmt::Display for EdgeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}->{}", self.from_stage, self.to_stage)
    }
}

/// State for an active parallel execution group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelGroupState {
    pub group_name: String,
    pub stages: Vec<String>,
    pub pending_agents: HashSet<String>,
    pub completed_agents: HashSet<String>,
    pub failed_agents: HashMap<String, String>,
    pub join_strategy: JoinStrategy,
}

/// External representation of a pipeline session's state.
///
/// Returned by `KernelHandle::get_session_state()` and included in
/// `CheckpointSnapshot`. Contains full execution state serialized as JSON
/// for cross-language consumption (PyO3, MCP).
#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Process identifier for this session.
    pub process_id: ProcessId,
    /// Name of the stage currently being executed or about to execute.
    pub current_stage: String,
    /// Ordered list of all stage names in the pipeline.
    pub stage_order: Vec<String>,
    /// Full Envelope serialized as JSON (outputs, bounds, audit trail, interrupts).
    pub envelope: serde_json::Value,
    /// Per-edge traversal counts, keyed as `"from_stage->to_stage"`.
    pub edge_traversals: HashMap<String, i32>,
    /// Whether the pipeline has terminated.
    pub terminated: bool,
    /// Reason for termination, if terminated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<TerminalReason>,
}

/// Generate JSON Schema for PipelineConfig.
///
/// Used for editor validation of pipeline.json files and API documentation.
/// The schema respects all serde attributes (tag, default, flatten, etc.).
pub fn pipeline_config_json_schema() -> serde_json::Value {
    // schema_for! produces a concrete struct; to_value only fails on
    // types with non-string map keys, which RootSchema does not have.
    #[allow(clippy::expect_used)]
    serde_json::to_value(schemars::schema_for!(PipelineConfig)).expect("schema serialization")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_stage(name: &str) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            ..PipelineStage::default()
        }
    }

    fn minimal_config(stages: Vec<PipelineStage>) -> PipelineConfig {
        PipelineConfig::test_default("test", stages)
    }

    #[test]
    fn test_validate_duplicate_stage_names() {
        let config = minimal_config(vec![minimal_stage("a"), minimal_stage("a")]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Duplicate stage name 'a'"));
    }

    #[test]
    fn test_validate_empty_stage_name() {
        let config = minimal_config(vec![minimal_stage("")]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Stage name must not be empty"));
    }

    #[test]
    fn test_validate_gate_allows_agent_field() {
        // Gate stages may have agent field populated (ignored at runtime)
        let mut stage = minimal_stage("router");
        stage.node_kind = NodeKind::Gate;
        let config = minimal_config(vec![stage]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_agent_empty_agent_field() {
        let mut stage = minimal_stage("worker");
        stage.agent = String::new();
        let config = minimal_config(vec![stage]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("must have a non-empty agent field"));
    }

    #[test]
    fn test_validate_fork_requires_routing_fn() {
        let mut stage = minimal_stage("splitter");
        stage.node_kind = NodeKind::Fork;
        let config = minimal_config(vec![stage]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Fork stage 'splitter' must have a routing_fn"));
    }

    #[test]
    fn test_validate_self_loop_without_max_visits() {
        let mut stage = minimal_stage("loop");
        stage.default_next = Some("loop".to_string());
        // No max_visits — should fail
        let config = minimal_config(vec![stage]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("infinite loop"));
    }

    #[test]
    fn test_validate_self_loop_with_max_visits_ok() {
        let mut stage = minimal_stage("loop");
        stage.default_next = Some("loop".to_string());
        stage.max_visits = Some(3);
        let config = minimal_config(vec![stage]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_duplicate_output_key() {
        let mut a = minimal_stage("a");
        a.output_key = Some("shared".to_string());
        let mut b = minimal_stage("b");
        b.output_key = Some("shared".to_string());
        let config = minimal_config(vec![a, b]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Duplicate output_key 'shared'"));
    }

    #[test]
    fn test_validate_valid_complex_pipeline() {
        let mut gate = minimal_stage("router");
        gate.node_kind = NodeKind::Gate;
        gate.agent = String::new(); // Gate has no agent
        gate.default_next = Some("fallback".to_string());

        let fallback = minimal_stage("fallback");
        let config = minimal_config(vec![gate, fallback]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pipeline_config_json_schema() {
        let schema = pipeline_config_json_schema();
        // Schema should be a valid JSON object with standard JSON Schema fields
        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();
        // Should have a type or $ref at the top level
        assert!(
            obj.contains_key("type") || obj.contains_key("$ref") || obj.contains_key("definitions"),
            "Schema top-level keys: {:?}",
            obj.keys().collect::<Vec<_>>()
        );
    }

}
