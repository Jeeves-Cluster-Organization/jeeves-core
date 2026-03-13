//! Pipeline orchestration types — config, instructions, metrics, session state.
//!
//! These types are shared between the Orchestrator and Worker modules.

use crate::envelope::{FlowInterrupt, TerminalReason};
use crate::types::{Error, ProcessId, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::routing::RoutingRule;

// =============================================================================
// Graph Primitive Types
// =============================================================================

/// Node kind in the pipeline graph.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum NodeKind {
    /// Standard agent execution node
    #[default]
    Agent,
    /// Pure routing node — evaluates routing without running an agent
    Gate,
    /// Parallel fan-out node — evaluates ALL matching rules, dispatches branches
    Fork,
}

/// Merge strategy for state fields.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Overwrite the existing value
    #[default]
    Replace,
    /// Append to an array (creates array if not present)
    Append,
    /// Shallow-merge object keys
    MergeDict,
}

/// Schema entry for a state field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateField {
    pub key: String,
    pub merge: MergeStrategy,
}

// =============================================================================
// Instruction Types
// =============================================================================

/// InstructionKind indicates what the worker should do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstructionKind {
    /// Execute specified agent
    RunAgent,
    /// Execute multiple agents in parallel (fan-out)
    RunAgents,
    /// Parallel group not yet complete — wait for more results
    WaitParallel,
    /// End execution
    Terminate,
    /// Wait for interrupt resolution
    WaitInterrupt,
}

/// Instruction tells the worker what to do next.
///
/// `agents` carries the agent name(s) to execute:
///   - RunAgent  → exactly 1 element
///   - RunAgents → N elements (parallel fan-out)
///   - Terminate / WaitInterrupt / WaitParallel → empty
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    pub kind: InstructionKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<TerminalReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub termination_message: Option<String>,
    pub interrupt_pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<FlowInterrupt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_context: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
}

impl Instruction {
    /// Create a Terminate instruction.
    pub fn terminate(reason: TerminalReason, message: impl Into<String>) -> Self {
        Self {
            kind: InstructionKind::Terminate,
            agents: vec![],
            terminal_reason: Some(reason),
            termination_message: Some(message.into()),
            interrupt_pending: false,
            interrupt: None,
            agent_context: None,
            output_schema: None,
            allowed_tools: None,
        }
    }

    /// Create a Terminate instruction with reason only (no message).
    pub fn terminate_completed() -> Self {
        Self {
            kind: InstructionKind::Terminate,
            agents: vec![],
            terminal_reason: Some(TerminalReason::Completed),
            termination_message: None,
            interrupt_pending: false,
            interrupt: None,
            agent_context: None,
            output_schema: None,
            allowed_tools: None,
        }
    }

    /// Create a RunAgent instruction for a single agent.
    pub fn run_agent(name: impl Into<String>) -> Self {
        Self {
            kind: InstructionKind::RunAgent,
            agents: vec![name.into()],
            terminal_reason: None,
            termination_message: None,
            interrupt_pending: false,
            interrupt: None,
            agent_context: None,
            output_schema: None,
            allowed_tools: None,
        }
    }

    /// Create a RunAgents instruction for parallel fan-out.
    pub fn run_agents(names: Vec<String>) -> Self {
        Self {
            kind: InstructionKind::RunAgents,
            agents: names,
            terminal_reason: None,
            termination_message: None,
            interrupt_pending: false,
            interrupt: None,
            agent_context: None,
            output_schema: None,
            allowed_tools: None,
        }
    }

    /// Create a WaitParallel instruction.
    pub fn wait_parallel() -> Self {
        Self {
            kind: InstructionKind::WaitParallel,
            agents: vec![],
            terminal_reason: None,
            termination_message: None,
            interrupt_pending: false,
            interrupt: None,
            agent_context: None,
            output_schema: None,
            allowed_tools: None,
        }
    }

    /// Create a WaitInterrupt instruction.
    pub fn wait_interrupt(interrupt: FlowInterrupt) -> Self {
        Self {
            kind: InstructionKind::WaitInterrupt,
            agents: vec![],
            terminal_reason: None,
            termination_message: None,
            interrupt_pending: true,
            interrupt: Some(interrupt),
            agent_context: None,
            output_schema: None,
            allowed_tools: None,
        }
    }
}

/// AgentExecutionMetrics contains metrics from agent execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentExecutionMetrics {
    pub llm_calls: i32,
    pub tool_calls: i32,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub duration_ms: i64,
}

// =============================================================================
// Pipeline Config
// =============================================================================

/// Pipeline configuration for orchestration.
///
/// The kernel is a pure evaluator — it iterates routing rules, first match wins.
/// All pipeline shape (linear, branching, cycles) is defined by the capability
/// layer via routing rules on each stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: String,
    pub stages: Vec<PipelineStage>,
    pub max_iterations: i32,
    pub max_llm_calls: i32,
    pub max_agent_hops: i32,
    pub edge_limits: Vec<EdgeLimit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_limit: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_schema: Vec<StateField>,
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

        let stage_names: HashSet<&str> =
            self.stages.iter().map(|s| s.name.as_str()).collect();

        for stage in &self.stages {
            // Validate routing targets reference existing stages
            for rule in &stage.routing {
                if !stage_names.contains(rule.target.as_str()) {
                    return Err(Error::validation(format!(
                        "Stage '{}' has routing target '{}' which does not exist in pipeline. Valid stages: {:?}",
                        stage.name, rule.target, stage_names
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

        // Validate Fork stages: routing targets must exist
        for stage in &self.stages {
            if stage.node_kind == NodeKind::Fork {
                for rule in &stage.routing {
                    if !stage_names.contains(rule.target.as_str()) {
                        return Err(Error::validation(format!(
                            "Fork stage '{}' routing target '{}' not found in pipeline",
                            stage.name, rule.target
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

/// Pipeline stage with routing rules, fallbacks, and parallel config.
///
/// Routing evaluation order (Temporal/K8s pattern):
/// 1. If agent failed AND error_next set → route to error_next
/// 2. Evaluate routing rules (first match wins)
/// 3. If no match AND default_next set → route to default_next
/// 4. If no match AND no default_next → terminate (COMPLETED)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub name: String,
    pub agent: String,
    #[serde(default)]
    pub routing: Vec<RoutingRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_next: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_next: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_visits: Option<i32>,
    #[serde(default)]
    pub join_strategy: JoinStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub node_kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_key: Option<String>,

    // --- Agent execution fields (consumed by worker, transparent to kernel) ---

    /// Prompt template key for this agent. None = deterministic (no LLM call).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_key: Option<String>,
    /// Whether this agent makes LLM calls. Derived from prompt_key if absent.
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
}

fn default_has_llm() -> bool {
    true
}

/// Join strategy for parallel execution groups.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum JoinStrategy {
    /// Wait for all agents to complete
    #[default]
    WaitAll,
    /// Advance when the first agent completes
    WaitFirst,
}

/// Per-edge transition limit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeLimit {
    pub from_stage: String,
    pub to_stage: String,
    pub max_count: i32,
}

/// State for an active parallel execution group.
#[derive(Debug, Clone)]
pub struct ParallelGroupState {
    pub group_name: String,
    pub stages: Vec<String>,
    pub pending_agents: HashSet<String>,
    pub completed_agents: HashSet<String>,
    pub failed_agents: HashMap<String, String>,
    pub join_strategy: JoinStrategy,
}

/// SessionState is the external representation of session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub process_id: ProcessId,
    pub current_stage: String,
    pub stage_order: Vec<String>,
    pub envelope: serde_json::Value,
    pub edge_traversals: HashMap<String, i32>,
    pub terminated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<TerminalReason>,
}
