//! Pipeline orchestration - kernel-driven pipeline execution control.
//!
//! The Orchestrator:
//!   - Owns the orchestration loop (moved from Python)
//!   - Evaluates routing rules
//!   - Tracks edge traversals
//!   - Enforces bounds (iterations, LLM calls, agent hops)
//!   - Returns Instructions to Python workers
//!
//! Python workers:
//!   - Execute agents (LLM calls, tool execution)
//!   - Report results back
//!   - Have NO control over what runs next

use crate::envelope::{Envelope, FlowInterrupt, TerminalReason};
use crate::types::{Error, ProcessId, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// =============================================================================
// Instruction Types
// =============================================================================

/// InstructionKind indicates what the Python worker should do next.
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

/// Instruction tells Python worker what to do next.
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
}

/// AgentExecutionMetrics contains metrics from agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub agents: Vec<PipelineStage>,
    pub max_iterations: i32,
    pub max_llm_calls: i32,
    pub max_agent_hops: i32,
    pub edge_limits: Vec<EdgeLimit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_limit: Option<i32>,
}

impl PipelineConfig {
    /// Get stage order from pipeline.
    pub fn get_stage_order(&self) -> Vec<String> {
        self.agents.iter().map(|s| s.name.clone()).collect()
    }

    /// Validate pipeline configuration.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(Error::validation("Pipeline name is required"));
        }
        if self.agents.is_empty() {
            return Err(Error::validation("Pipeline must have at least one stage"));
        }

        let stage_names: HashSet<&str> =
            self.agents.iter().map(|s| s.name.as_str()).collect();

        for stage in &self.agents {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_group: Option<String>,
    #[serde(default)]
    pub join_strategy: JoinStrategy,
}

/// Routing rule: expression tree + target stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub expr: RoutingExpr,
    pub target: String,
}

/// Routing expression tree evaluated recursively by the kernel.
///
/// Serde: internally tagged with "op" field.
/// JSON example: `{"op": "Eq", "field": {"scope": "Current", "key": "intent"}, "value": "greet"}`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum RoutingExpr {
    /// Equality: field == value
    Eq { field: FieldRef, value: serde_json::Value },
    /// Inequality: field != value
    Neq { field: FieldRef, value: serde_json::Value },
    /// Greater than (numeric f64 comparison)
    Gt { field: FieldRef, value: serde_json::Value },
    /// Less than (numeric f64 comparison)
    Lt { field: FieldRef, value: serde_json::Value },
    /// Greater than or equal (numeric f64 comparison)
    Gte { field: FieldRef, value: serde_json::Value },
    /// Less than or equal (numeric f64 comparison)
    Lte { field: FieldRef, value: serde_json::Value },
    /// String contains substring, or array contains element
    Contains { field: FieldRef, value: serde_json::Value },
    /// Field exists and is not null
    Exists { field: FieldRef },
    /// Field is absent or null
    NotExists { field: FieldRef },
    /// All sub-expressions must be true
    And { exprs: Vec<RoutingExpr> },
    /// At least one sub-expression must be true
    Or { exprs: Vec<RoutingExpr> },
    /// Negate a sub-expression
    Not { expr: Box<RoutingExpr> },
    /// Unconditional match (always true)
    Always,
}

/// Scoped field reference for routing expressions.
///
/// Serde: internally tagged with "scope" field.
/// JSON example: `{"scope": "Agent", "agent": "understand", "key": "topic"}`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "scope")]
pub enum FieldRef {
    /// output[current_agent][key]
    Current { key: String },
    /// output[agent][key] — cross-agent reference
    Agent { agent: String, key: String },
    /// envelope.audit.metadata with dot-notation traversal
    Meta { path: String },
    /// interrupt_response[key] — serialized InterruptResponse
    Interrupt { key: String },
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

// =============================================================================
// Orchestration Session
// =============================================================================

/// PipelineSession represents an active pipeline execution session.
///
/// The session tracks pipeline execution state only (config, edge traversals,
/// termination status). The envelope is owned by the Kernel's `process_envelopes`
/// store — the microkernel pattern: kernel owns state, subsystems process it.
#[derive(Debug, Clone)]
pub struct PipelineSession {
    pub process_id: ProcessId,
    pub pipeline_config: PipelineConfig,
    // NOTE: No envelope field — kernel owns it via `process_envelopes`
    pub edge_traversals: HashMap<String, i32>, // "from->to" -> count
    pub stage_visits: HashMap<String, i32>,    // stage_name -> visit count
    pub active_parallel: Option<ParallelGroupState>,
    pub terminated: bool,
    pub terminal_reason: Option<TerminalReason>,
    pub created_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
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

// =============================================================================
// Orchestrator
// =============================================================================

/// Orchestrator manages kernel-side pipeline execution.
///
/// Stateless with respect to envelopes — the Kernel extracts envelopes from
/// `process_envelopes`, passes them in, and re-stores the results.
#[derive(Debug)]
pub struct Orchestrator {
    pipelines: HashMap<ProcessId, PipelineSession>,
}

impl Orchestrator {
    /// Create a new Orchestrator.
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
        }
    }

    // =============================================================================
    // Session Management
    // =============================================================================

    /// Initialize a new pipeline session.
    ///
    /// Takes a mutable envelope reference to set pipeline bounds (kernel owns the envelope).
    /// If force is false and a session already exists, returns an error.
    /// If force is true, replaces any existing session.
    pub fn initialize_session(
        &mut self,
        process_id: ProcessId,
        pipeline_config: PipelineConfig,
        envelope: &mut Envelope,
        force: bool,
    ) -> Result<SessionState> {
        // Check for duplicate processID
        if self.pipelines.contains_key(&process_id) && !force {
            return Err(Error::validation(format!(
                "Session already exists for process: {} (use force=true to replace)",
                process_id
            )));
        }

        // Validate pipeline config
        pipeline_config.validate()?;

        // Initialize envelope with pipeline bounds
        envelope.pipeline.max_iterations = pipeline_config.max_iterations;
        envelope.bounds.max_llm_calls = pipeline_config.max_llm_calls;
        envelope.bounds.max_agent_hops = pipeline_config.max_agent_hops;
        envelope.pipeline.stage_order = pipeline_config.get_stage_order();

        // Set initial stage if not set
        if envelope.pipeline.current_stage.is_empty() && !envelope.pipeline.stage_order.is_empty() {
            envelope.pipeline.current_stage = envelope.pipeline.stage_order[0].clone();
        }

        let now = Utc::now();
        let session = PipelineSession {
            process_id: process_id.clone(),
            pipeline_config,
            edge_traversals: HashMap::new(),
            stage_visits: HashMap::new(),
            active_parallel: None,
            terminated: false,
            terminal_reason: None,
            created_at: now,
            last_activity_at: now,
        };

        let state = self.build_session_state(&session, envelope);
        self.pipelines.insert(process_id, session);

        Ok(state)
    }

    /// Get the next instruction for a process.
    ///
    /// Envelope is passed in by the Kernel (which owns it). The envelope may be
    /// mutated (e.g., bounds termination) — the Kernel re-stores it afterward.
    pub fn get_next_instruction(
        &mut self,
        process_id: &ProcessId,
        envelope: &mut Envelope,
    ) -> Result<Instruction> {
        let session = self
            .pipelines
            .get_mut(process_id)
            .ok_or_else(|| Error::not_found(format!("Unknown process: {}", process_id)))?;

        // Update activity timestamp
        session.last_activity_at = Utc::now();

        // Check if already terminated
        if session.terminated {
            return Ok(Instruction {
                kind: InstructionKind::Terminate,
                agents: vec![],
                terminal_reason: session.terminal_reason,
                termination_message: Some("Session already terminated".to_string()),
                interrupt_pending: false,
                interrupt: None,
            });
        }

        // Check for pending interrupt
        if envelope.interrupts.interrupt_pending {
            return Ok(Instruction {
                kind: InstructionKind::WaitInterrupt,
                agents: vec![],
                terminal_reason: None,
                termination_message: None,
                interrupt_pending: true,
                interrupt: envelope.interrupts.interrupt.clone(),
            });
        }

        // Check bounds
        if let Some(reason) = check_bounds(envelope) {
            session.terminated = true;
            session.terminal_reason = Some(reason);
            envelope.bounds.terminated = true;
            envelope.bounds.terminal_reason = Some(reason);
            return Ok(Instruction {
                kind: InstructionKind::Terminate,
                agents: vec![],
                terminal_reason: Some(reason),
                termination_message: Some(format!("Bounds exceeded: {:?}", reason)),
                interrupt_pending: false,
                interrupt: None,
            });
        }

        // Check for active parallel group
        if let Some(ref mut parallel) = session.active_parallel {
            if is_parallel_join_met(parallel) {
                // Join condition met — will be resolved in next report_agent_result
                return Ok(Instruction {
                    kind: InstructionKind::WaitParallel,
                    agents: vec![],
                    terminal_reason: None,
                    termination_message: None,
                    interrupt_pending: false,
                    interrupt: None,
                });
            }

            if !parallel.pending_agents.is_empty() {
                // Parallel group started — dispatch all pending agents
                let agents: Vec<String> = parallel.pending_agents.drain().collect();
                return Ok(Instruction {
                    kind: InstructionKind::RunAgents,
                    agents,
                    terminal_reason: None,
                    termination_message: None,
                    interrupt_pending: false,
                    interrupt: None,
                });
            }

            // Agents dispatched, waiting for results
            return Ok(Instruction {
                kind: InstructionKind::WaitParallel,
                agents: vec![],
                terminal_reason: None,
                termination_message: None,
                interrupt_pending: false,
                interrupt: None,
            });
        }

        // Determine next agent to run (single agent)
        let current_stage = &envelope.pipeline.current_stage;
        if current_stage.is_empty() {
            return Err(Error::state_transition("No current stage set"));
        }

        let agent_name = get_agent_for_stage(&session.pipeline_config, current_stage)?;

        Ok(Instruction {
            kind: InstructionKind::RunAgent,
            agents: vec![agent_name],
            terminal_reason: None,
            termination_message: None,
            interrupt_pending: false,
            interrupt: None,
        })
    }

    /// Process agent execution result.
    ///
    /// Takes the updated envelope (Kernel extracted it from `process_envelopes`,
    /// IPC handler may have merged agent outputs into it). Updates metrics,
    /// advances the pipeline via routing evaluation, and mutates the envelope.
    ///
    /// Routing evaluation order (Temporal/K8s pattern):
    /// 1. If agent_failed AND error_next set → route to error_next
    /// 2. Evaluate routing rules (first match wins)
    /// 3. If no match AND default_next set → route to default_next
    /// 4. If no match AND no default_next → terminate (COMPLETED)
    pub fn report_agent_result(
        &mut self,
        process_id: &ProcessId,
        metrics: AgentExecutionMetrics,
        envelope: &mut Envelope,
        agent_failed: bool,
    ) -> Result<()> {
        let session = self
            .pipelines
            .get_mut(process_id)
            .ok_or_else(|| Error::not_found(format!("Unknown process: {}", process_id)))?;

        // Update metrics in envelope
        envelope.bounds.llm_call_count += metrics.llm_calls;
        envelope.bounds.tool_call_count += metrics.tool_calls;
        if let Some(tokens_in) = metrics.tokens_in {
            envelope.bounds.tokens_in += tokens_in;
        }
        if let Some(tokens_out) = metrics.tokens_out {
            envelope.bounds.tokens_out += tokens_out;
        }

        // Increment iteration
        envelope.pipeline.iteration += 1;

        // Check bounds after iteration increment (terminates cycles)
        if let Some(reason) = check_bounds(envelope) {
            session.terminated = true;
            session.terminal_reason = Some(reason);
            envelope.bounds.terminated = true;
            envelope.bounds.terminal_reason = Some(reason);
            return Ok(());
        }

        // ===== PARALLEL GROUP HANDLING =====
        if session.active_parallel.is_some() {
            let current_stage = envelope.pipeline.current_stage.clone();
            let agent_name_for_parallel = get_agent_for_stage(&session.pipeline_config, &current_stage)
                .unwrap_or_default();

            // Mark agent as completed/failed in the parallel group
            {
                let parallel = session.active_parallel.as_mut().unwrap();
                parallel.pending_agents.remove(&agent_name_for_parallel);
                if agent_failed {
                    parallel.failed_agents.insert(
                        agent_name_for_parallel.clone(),
                        "agent failed".to_string(),
                    );
                } else {
                    parallel.completed_agents.insert(agent_name_for_parallel.clone());
                }
            }

            // Check if join condition is met
            let join_met = is_parallel_join_met(session.active_parallel.as_ref().unwrap());

            if !join_met {
                // More agents pending — wait
                session.last_activity_at = Utc::now();
                return Ok(());
            }

            // Join met — resolve routing using FIRST stage in the parallel group
            let parallel = session.active_parallel.take().unwrap();
            envelope.pipeline.parallel_mode = false;
            envelope.pipeline.active_stages.clear();

            let first_stage_name = parallel.stages.first()
                .ok_or_else(|| Error::state_transition("Parallel group has no stages"))?
                .clone();

            // Mark all parallel stages as completed
            for stage_name in &parallel.stages {
                *session.stage_visits.entry(stage_name.clone()).or_insert(0) += 1;
                envelope.complete_stage(stage_name);
            }

            let first_stage = session.pipeline_config.agents
                .iter()
                .find(|s| s.name == first_stage_name)
                .ok_or_else(|| Error::state_transition(format!(
                    "Parallel first stage '{}' not found in pipeline config",
                    first_stage_name
                )))?
                .clone();

            let first_agent_name = first_stage.agent.clone();
            let any_failed = !parallel.failed_agents.is_empty();

            let interrupt_response = envelope.interrupts.interrupt.as_ref()
                .and_then(|i| i.response.as_ref())
                .and_then(|r| serde_json::to_value(r).ok());

            let next_target = evaluate_routing(
                &first_stage,
                &envelope.outputs,
                &first_agent_name,
                any_failed,
                &envelope.audit.metadata,
                interrupt_response.as_ref(),
            );

            return self.apply_routing_result(
                process_id,
                &first_stage_name,
                next_target,
                envelope,
            );
        }

        // ===== NORMAL (NON-PARALLEL) ROUTING =====
        let current_stage = envelope.pipeline.current_stage.clone();

        // Look up the PipelineStage config for the current stage
        let pipeline_stage = session.pipeline_config.agents
            .iter()
            .find(|s| s.name == current_stage)
            .ok_or_else(|| Error::state_transition(format!(
                "Current stage '{}' not found in pipeline config",
                current_stage
            )))?
            .clone();

        // Track stage visit
        *session.stage_visits.entry(current_stage.clone()).or_insert(0) += 1;

        // Mark current stage as completed
        envelope.complete_stage(&current_stage);

        // Get agent name for output lookup
        let agent_name = pipeline_stage.agent.clone();

        // Prepare interrupt response for routing evaluation
        let interrupt_response = envelope.interrupts.interrupt.as_ref()
            .and_then(|i| i.response.as_ref())
            .and_then(|r| serde_json::to_value(r).ok());

        // Evaluate routing (error_next → rules → default_next → terminate)
        let next_target = evaluate_routing(
            &pipeline_stage,
            &envelope.outputs,
            &agent_name,
            agent_failed,
            &envelope.audit.metadata,
            interrupt_response.as_ref(),
        );

        self.apply_routing_result(process_id, &current_stage, next_target, envelope)
    }

    /// Apply the result of routing evaluation — advance to target, start parallel group, or terminate.
    fn apply_routing_result(
        &mut self,
        process_id: &ProcessId,
        from_stage: &str,
        next_target: Option<String>,
        envelope: &mut Envelope,
    ) -> Result<()> {
        let session = self
            .pipelines
            .get_mut(process_id)
            .ok_or_else(|| Error::not_found(format!("Unknown process: {}", process_id)))?;

        match next_target {
            Some(target) => {
                // Check max_visits on the TARGET stage
                if let Some(target_stage) = session.pipeline_config.agents.iter().find(|s| s.name == target) {
                    if let Some(max_visits) = target_stage.max_visits {
                        let visits = session.stage_visits.get(&target).copied().unwrap_or(0);
                        if visits >= max_visits {
                            session.terminated = true;
                            session.terminal_reason = Some(TerminalReason::MaxStageVisitsExceeded);
                            envelope.bounds.terminated = true;
                            envelope.bounds.terminal_reason = Some(TerminalReason::MaxStageVisitsExceeded);
                            envelope.bounds.termination_reason = Some(format!(
                                "Stage '{}' exceeded max_visits limit of {}", target, max_visits
                            ));
                            session.last_activity_at = Utc::now();
                            return Ok(());
                        }
                    }
                }

                // Check edge limit
                if !check_edge_limit(
                    &session.pipeline_config,
                    &session.edge_traversals,
                    from_stage,
                    &target,
                ) {
                    session.terminated = true;
                    session.terminal_reason = Some(TerminalReason::MaxAgentHopsExceeded);
                    envelope.bounds.terminated = true;
                    envelope.bounds.terminal_reason = Some(TerminalReason::MaxAgentHopsExceeded);
                    envelope.bounds.termination_reason = Some(format!(
                        "Edge limit exceeded: {} -> {}", from_stage, target
                    ));
                    session.last_activity_at = Utc::now();
                    return Ok(());
                }

                // Record edge traversal
                let edge_key = format!("{}->{}", from_stage, target);
                *session.edge_traversals.entry(edge_key).or_insert(0) += 1;
                envelope.bounds.agent_hop_count += 1;

                // Check if target has a parallel_group → start parallel fan-out
                let target_stage = session.pipeline_config.agents.iter()
                    .find(|s| s.name == target)
                    .cloned();
                if let Some(ref ts) = target_stage {
                    if let Some(ref group_name) = ts.parallel_group {
                        // Collect ALL stages in this parallel group
                        let group_stages: Vec<PipelineStage> = session.pipeline_config.agents.iter()
                            .filter(|s| s.parallel_group.as_deref() == Some(group_name.as_str()))
                            .cloned()
                            .collect();

                        let stage_names: Vec<String> = group_stages.iter().map(|s| s.name.clone()).collect();
                        let agent_names: HashSet<String> = group_stages.iter().map(|s| s.agent.clone()).collect();

                        let join_strategy = group_stages.first()
                            .map(|s| s.join_strategy)
                            .unwrap_or_default();

                        session.active_parallel = Some(ParallelGroupState {
                            group_name: group_name.clone(),
                            stages: stage_names.clone(),
                            pending_agents: agent_names,
                            completed_agents: HashSet::new(),
                            failed_agents: HashMap::new(),
                            join_strategy,
                        });

                        // Set envelope parallel state
                        envelope.pipeline.parallel_mode = true;
                        envelope.pipeline.active_stages = stage_names.into_iter().collect();
                        // Set current_stage to the first stage in the group (primary marker)
                        envelope.pipeline.current_stage = target;

                        session.last_activity_at = Utc::now();
                        return Ok(());
                    }
                }

                // Normal advance to target stage
                envelope.pipeline.current_stage = target;
                session.last_activity_at = Utc::now();
            }
            None => {
                // No routing matched, no default_next — pipeline complete (Temporal pattern)
                session.terminated = true;
                session.terminal_reason = Some(TerminalReason::Completed);
                envelope.bounds.terminated = true;
                envelope.bounds.terminal_reason = Some(TerminalReason::Completed);
                session.last_activity_at = Utc::now();
            }
        }

        Ok(())
    }

    /// Get session state for external queries.
    ///
    /// Envelope is passed in by the Kernel (which owns it).
    pub fn get_session_state(
        &self,
        process_id: &ProcessId,
        envelope: &Envelope,
    ) -> Result<SessionState> {
        let session = self
            .pipelines
            .get(process_id)
            .ok_or_else(|| Error::not_found(format!("Unknown process: {}", process_id)))?;

        Ok(self.build_session_state(session, envelope))
    }

    /// Check if a pipeline session exists for the given process.
    pub fn has_session(&self, process_id: &ProcessId) -> bool {
        self.pipelines.contains_key(process_id)
    }

    /// Cleanup a pipeline session.
    pub fn cleanup_session(&mut self, process_id: &ProcessId) -> bool {
        self.pipelines.remove(process_id).is_some()
    }

    /// Get pipeline session count.
    pub fn get_session_count(&self) -> usize {
        self.pipelines.len()
    }

    /// Cleanup stale pipeline sessions older than the given duration.
    /// Returns the PIDs of removed sessions so the Kernel can also clean up
    /// the corresponding envelopes from `process_envelopes`.
    pub fn cleanup_stale_sessions(&mut self, max_age_seconds: i64) -> Vec<ProcessId> {
        let cutoff = Utc::now() - chrono::TimeDelta::seconds(max_age_seconds);
        let mut to_remove = Vec::new();

        for (pid, session) in &self.pipelines {
            if session.last_activity_at < cutoff {
                to_remove.push(pid.clone());
            }
        }

        for pid in &to_remove {
            self.pipelines.remove(pid);
        }

        to_remove
    }

    // =============================================================================
    // Internal Helpers
    // =============================================================================

    /// Build external session state representation.
    fn build_session_state(&self, session: &PipelineSession, envelope: &Envelope) -> SessionState {
        let envelope_value = serde_json::to_value(envelope)
            .expect("envelope must be serializable to JSON");

        SessionState {
            process_id: session.process_id.clone(),
            current_stage: envelope.pipeline.current_stage.clone(),
            stage_order: envelope.pipeline.stage_order.clone(),
            envelope: envelope_value,
            edge_traversals: session.edge_traversals.clone(),
            terminated: session.terminated,
            terminal_reason: session.terminal_reason,
        }
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions (outside impl to avoid borrow checker issues)
// =============================================================================

/// Check if envelope exceeds bounds.
fn check_bounds(envelope: &Envelope) -> Option<TerminalReason> {
    if envelope.bounds.llm_call_count >= envelope.bounds.max_llm_calls {
        return Some(TerminalReason::MaxLlmCallsExceeded);
    }
    if envelope.pipeline.iteration >= envelope.pipeline.max_iterations {
        return Some(TerminalReason::MaxIterationsExceeded);
    }
    if envelope.bounds.agent_hop_count >= envelope.bounds.max_agent_hops {
        return Some(TerminalReason::MaxAgentHopsExceeded);
    }
    None
}

/// Get agent name for a stage.
fn get_agent_for_stage(
    pipeline_config: &PipelineConfig,
    stage_name: &str,
) -> Result<String> {
    pipeline_config
        .agents
        .iter()
        .find(|s| s.name == stage_name)
        .map(|s| s.agent.clone())
        .ok_or_else(|| Error::not_found(format!("Stage not found in pipeline: {}", stage_name)))
}

/// Evaluate routing for a stage (Temporal/K8s pattern).
///
/// Evaluation order:
/// 1. If agent failed AND error_next set → route to error_next
/// 2. Evaluate routing rules (first match wins)
/// 3. If no match AND default_next set → route to default_next
/// 4. If no match AND no default_next → None (caller terminates with COMPLETED)
fn evaluate_routing(
    stage: &PipelineStage,
    agent_outputs: &HashMap<String, HashMap<String, serde_json::Value>>,
    agent_name: &str,
    agent_failed: bool,
    metadata: &HashMap<String, serde_json::Value>,
    interrupt_response: Option<&serde_json::Value>,
) -> Option<String> {
    // 1. Error path
    if agent_failed {
        if let Some(ref error_next) = stage.error_next {
            return Some(error_next.clone());
        }
    }

    // 2. Routing rules (first match wins)
    for rule in &stage.routing {
        if evaluate_expr(&rule.expr, agent_outputs, agent_name, metadata, interrupt_response) {
            return Some(rule.target.clone());
        }
    }

    // 3. Default fallback
    if let Some(ref default_next) = stage.default_next {
        return Some(default_next.clone());
    }

    // 4. No match — Temporal pattern: kernel terminates
    None
}

/// Recursively evaluate a routing expression against agent outputs and metadata.
fn evaluate_expr(
    expr: &RoutingExpr,
    agent_outputs: &HashMap<String, HashMap<String, serde_json::Value>>,
    current_agent: &str,
    metadata: &HashMap<String, serde_json::Value>,
    interrupt_response: Option<&serde_json::Value>,
) -> bool {
    match expr {
        RoutingExpr::Always => true,

        RoutingExpr::Eq { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(false, |v| v == *value)
        }

        RoutingExpr::Neq { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(false, |v| v != *value)
        }

        RoutingExpr::Gt { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .and_then(|v| Some(v.as_f64()? > value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Lt { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .and_then(|v| Some(v.as_f64()? < value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Gte { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .and_then(|v| Some(v.as_f64()? >= value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Lte { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .and_then(|v| Some(v.as_f64()? <= value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Contains { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(false, |v| {
                    // String contains substring
                    if let (Some(s), Some(substr)) = (v.as_str(), value.as_str()) {
                        return s.contains(substr);
                    }
                    // Array contains element
                    if let Some(arr) = v.as_array() {
                        return arr.contains(value);
                    }
                    false
                })
        }

        RoutingExpr::Exists { field } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(false, |v| !v.is_null())
        }

        RoutingExpr::NotExists { field } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(true, |v| v.is_null())
        }

        RoutingExpr::And { exprs } => {
            exprs.iter().all(|e| evaluate_expr(e, agent_outputs, current_agent, metadata, interrupt_response))
        }

        RoutingExpr::Or { exprs } => {
            exprs.iter().any(|e| evaluate_expr(e, agent_outputs, current_agent, metadata, interrupt_response))
        }

        RoutingExpr::Not { expr } => {
            !evaluate_expr(expr, agent_outputs, current_agent, metadata, interrupt_response)
        }
    }
}

/// Resolve a field reference to its value from agent outputs, metadata, or interrupt response.
fn resolve_field(
    field: &FieldRef,
    agent_outputs: &HashMap<String, HashMap<String, serde_json::Value>>,
    current_agent: &str,
    metadata: &HashMap<String, serde_json::Value>,
    interrupt_response: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    match field {
        FieldRef::Current { key } => {
            agent_outputs.get(current_agent)?.get(key).cloned()
        }
        FieldRef::Agent { agent, key } => {
            agent_outputs.get(agent.as_str())?.get(key).cloned()
        }
        FieldRef::Meta { path } => {
            // Dot-notation traversal: "session_context.has_history"
            let parts: Vec<&str> = path.split('.').collect();
            if parts.is_empty() {
                return None;
            }
            let first = metadata.get(parts[0])?;
            let mut current = first.clone();
            for part in &parts[1..] {
                current = current.get(part)?.clone();
            }
            Some(current)
        }
        FieldRef::Interrupt { key } => {
            interrupt_response?.get(key).cloned()
        }
    }
}

/// Check if an edge transition is within its limit.
/// Returns true if the transition is allowed.
fn check_edge_limit(
    config: &PipelineConfig,
    edge_traversals: &HashMap<String, i32>,
    from_stage: &str,
    to_stage: &str,
) -> bool {
    for limit in &config.edge_limits {
        if limit.from_stage == from_stage && limit.to_stage == to_stage {
            let edge_key = format!("{}->{}", from_stage, to_stage);
            let count = edge_traversals.get(&edge_key).copied().unwrap_or(0);
            return count < limit.max_count;
        }
    }
    // No limit configured — allowed
    true
}

/// Check if a parallel group's join condition is met.
///
/// `pending_agents` is drained on dispatch, so join is based on reported count vs total stages.
fn is_parallel_join_met(parallel: &ParallelGroupState) -> bool {
    let reported = parallel.completed_agents.len() + parallel.failed_agents.len();
    match parallel.join_strategy {
        JoinStrategy::WaitAll => reported >= parallel.stages.len(),
        JoinStrategy::WaitFirst => reported > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProcessId;
    use serde_json::Value;

    // =========================================================================
    // Helpers
    // =========================================================================

    fn stage(name: &str, agent: &str, routing: Vec<RoutingRule>, default_next: Option<&str>) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: agent.to_string(),
            routing,
            default_next: default_next.map(|s| s.to_string()),
            error_next: None,
            max_visits: None,
            parallel_group: None,
            join_strategy: JoinStrategy::default(),
        }
    }

    fn always_to(target: &str) -> RoutingRule {
        RoutingRule { expr: RoutingExpr::Always, target: target.to_string() }
    }

    fn eq_rule(key: &str, value: impl Into<Value>, target: &str) -> RoutingRule {
        RoutingRule {
            expr: RoutingExpr::Eq {
                field: FieldRef::Current { key: key.to_string() },
                value: value.into(),
            },
            target: target.to_string(),
        }
    }

    fn zero_metrics() -> AgentExecutionMetrics {
        AgentExecutionMetrics { llm_calls: 0, tool_calls: 0, tokens_in: Some(0), tokens_out: Some(0), duration_ms: 0 }
    }

    /// Linear 2-stage pipeline: stage1 --(Always)--> stage2 --(no rules, no default)--> terminate
    fn create_test_pipeline() -> PipelineConfig {
        PipelineConfig {
            name: "test_pipeline".to_string(),
            agents: vec![
                stage("stage1", "agent1", vec![always_to("stage2")], None),
                stage("stage2", "agent2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 50,
            max_agent_hops: 5,
            edge_limits: vec![],
            step_limit: None,
        }
    }

    fn create_test_envelope() -> Envelope {
        let mut env = Envelope::new();
        env.pipeline.current_stage = String::new();
        env
    }

    // =========================================================================
    // Session lifecycle
    // =========================================================================

    #[test]
    fn test_initialize_session() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let state = orch
            .initialize_session(ProcessId::must("proc1"), pipeline.clone(), &mut envelope, false)
            .unwrap();

        assert_eq!(state.process_id.as_str(), "proc1");
        assert_eq!(state.current_stage, "stage1");
        assert_eq!(state.stage_order, vec!["stage1", "stage2"]);
        assert!(!state.terminated);
    }

    #[test]
    fn test_initialize_session_duplicate_fails() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline.clone(), &mut envelope, false)
            .unwrap();

        let mut envelope2 = create_test_envelope();
        let result =
            orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope2, false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_initialize_session_force_replaces() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline.clone(), &mut envelope, false)
            .unwrap();

        let mut envelope2 = create_test_envelope();
        let result =
            orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope2, true);

        assert!(result.is_ok());
    }

    // =========================================================================
    // get_next_instruction
    // =========================================================================

    #[test]
    fn test_get_next_instruction_run_agent() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        assert_eq!(instruction.kind, InstructionKind::RunAgent);
        assert_eq!(instruction.agents, vec!["agent1".to_string()]);
        assert!(!instruction.interrupt_pending);
    }

    #[test]
    fn test_get_next_instruction_terminate_after_terminated() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let session = orch.pipelines.get_mut(&ProcessId::must("proc1")).unwrap();
        session.terminated = true;
        session.terminal_reason = Some(TerminalReason::MaxIterationsExceeded);

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        assert_eq!(instruction.kind, InstructionKind::Terminate);
        assert_eq!(
            instruction.terminal_reason,
            Some(TerminalReason::MaxIterationsExceeded)
        );
    }

    #[test]
    fn test_get_next_instruction_wait_interrupt() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        envelope.interrupts.interrupt_pending = true;
        envelope.interrupts.interrupt = Some(
            FlowInterrupt::new(crate::envelope::InterruptKind::Clarification)
                .with_message("Need clarification".to_string())
        );

        orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        assert_eq!(instruction.kind, InstructionKind::WaitInterrupt);
        assert!(instruction.interrupt_pending);
        assert!(instruction.interrupt.is_some());
    }

    #[test]
    fn test_check_bounds_llm_exceeded() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        envelope.bounds.llm_call_count = 50;
        envelope.bounds.max_llm_calls = 50;

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        assert_eq!(instruction.kind, InstructionKind::Terminate);
        assert_eq!(
            instruction.terminal_reason,
            Some(TerminalReason::MaxLlmCallsExceeded)
        );
    }

    #[test]
    fn test_check_bounds_iterations_exceeded() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        envelope.pipeline.iteration = 10;
        envelope.pipeline.max_iterations = 10;

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        assert_eq!(instruction.kind, InstructionKind::Terminate);
        assert_eq!(
            instruction.terminal_reason,
            Some(TerminalReason::MaxIterationsExceeded)
        );
    }

    // =========================================================================
    // report_agent_result — metrics
    // =========================================================================

    #[test]
    fn test_report_agent_result_updates_metrics() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let metrics = AgentExecutionMetrics {
            llm_calls: 2,
            tool_calls: 5,
            tokens_in: Some(1000),
            tokens_out: Some(500),
            duration_ms: 1500,
        };

        orch.report_agent_result(&ProcessId::must("proc1"), metrics, &mut envelope, false)
            .unwrap();

        assert_eq!(envelope.bounds.llm_call_count, 2);
        assert_eq!(envelope.bounds.tool_call_count, 5);
        assert_eq!(envelope.bounds.tokens_in, 1000);
        assert_eq!(envelope.bounds.tokens_out, 500);
        assert_eq!(envelope.pipeline.iteration, 1);
    }

    #[test]
    fn test_report_agent_result_with_unknown_tokens_keeps_token_counters() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let metrics = AgentExecutionMetrics {
            llm_calls: 1,
            tool_calls: 1,
            tokens_in: None,
            tokens_out: None,
            duration_ms: 100,
        };

        orch.report_agent_result(&ProcessId::must("proc1"), metrics, &mut envelope, false)
            .unwrap();

        assert_eq!(envelope.bounds.tokens_in, 0);
        assert_eq!(envelope.bounds.tokens_out, 0);
    }

    // =========================================================================
    // Session state & cleanup
    // =========================================================================

    #[test]
    fn test_get_session_state() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let state = orch.get_session_state(&ProcessId::must("proc1"), &envelope).unwrap();

        assert_eq!(state.process_id.as_str(), "proc1");
        assert_eq!(state.current_stage, "stage1");
        assert!(state.envelope.is_object());
        assert!(!state.terminated);
    }

    #[test]
    fn test_cleanup_session() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        assert_eq!(orch.get_session_count(), 1);

        let removed = orch.cleanup_session(&ProcessId::must("proc1"));
        assert!(removed);
        assert_eq!(orch.get_session_count(), 0);

        let removed = orch.cleanup_session(&ProcessId::must("proc1"));
        assert!(!removed);
    }

    #[test]
    fn test_get_session_state_not_found() {
        let orch = Orchestrator::new();
        let envelope = create_test_envelope();

        let result = orch.get_session_state(&ProcessId::must("nonexistent"), &envelope);
        assert!(result.is_err());
    }

    // =========================================================================
    // Pipeline validation
    // =========================================================================

    #[test]
    fn test_pipeline_validation() {
        let mut pipeline = create_test_pipeline();

        assert!(pipeline.validate().is_ok());

        // Empty name
        pipeline.name = String::new();
        assert!(pipeline.validate().is_err());

        // Empty stages
        let mut pipeline2 = create_test_pipeline();
        pipeline2.agents = vec![];
        assert!(pipeline2.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_routing_target() {
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("nonexistent")], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_default_next() {
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![], Some("nonexistent")),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_error_next() {
        let mut s = stage("s1", "a1", vec![], None);
        s.error_next = Some("nonexistent".to_string());
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![s],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_max_visits_zero_rejected() {
        let mut s = stage("s1", "a1", vec![], None);
        s.max_visits = Some(0);
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![s],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        assert!(pipeline.validate().is_err());
    }

    // =========================================================================
    // Routing — RoutingExpr evaluation
    // =========================================================================

    #[test]
    fn test_routing_eq_match() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![
                    eq_rule("intent", "skip", "s3"),
                    always_to("s2"),
                ], None),
                stage("s2", "a2", vec![], None),
                stage("s3", "a3", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Agent a1 outputs intent="skip" → routes to s3
        let mut output = HashMap::new();
        output.insert("intent".to_string(), serde_json::json!("skip"));
        envelope.outputs.insert("a1".to_string(), output);

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "s3");
        assert!(!orch.pipelines.get(&ProcessId::must("p1")).unwrap().terminated);
    }

    #[test]
    fn test_routing_always_acts_as_catch_all() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "s2");
        assert!(!orch.pipelines.get(&ProcessId::must("p1")).unwrap().terminated);
    }

    #[test]
    fn test_routing_first_match_wins() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![
                    eq_rule("intent", "greet", "s2"),
                    eq_rule("intent", "greet", "s3"),
                ], None),
                stage("s2", "a2", vec![], None),
                stage("s3", "a3", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut output = HashMap::new();
        output.insert("intent".to_string(), serde_json::json!("greet"));
        envelope.outputs.insert("a1".to_string(), output);

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // First rule wins → s2, not s3
        assert_eq!(envelope.pipeline.current_stage, "s2");
    }

    #[test]
    fn test_routing_default_next_fallback() {
        // No routing rules, but default_next is set → routes to default_next
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![], Some("s2")),
                stage("s2", "a2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "s2");
        assert!(!orch.pipelines.get(&ProcessId::must("p1")).unwrap().terminated);
    }

    #[test]
    fn test_routing_no_rules_no_default_terminates() {
        // Temporal/K8s pattern: no match + no default_next = COMPLETED
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.terminated);
        assert_eq!(session.terminal_reason, Some(TerminalReason::Completed));
    }

    #[test]
    fn test_routing_error_next_on_failure() {
        // agent_failed=true AND error_next set → routes to error_next (bypasses rules)
        let mut s1 = stage("s1", "a1", vec![always_to("s2")], None);
        s1.error_next = Some("s3".to_string());
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                s1,
                stage("s2", "a2", vec![], None),
                stage("s3", "a3", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // agent_failed = true → should go to error_next (s3), NOT s2
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, true).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "s3");
    }

    #[test]
    fn test_routing_failure_without_error_next_falls_through_to_rules() {
        // agent_failed=true but no error_next → normal rule evaluation
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, true).unwrap();

        // No error_next → falls through to Always → s2
        assert_eq!(envelope.pipeline.current_stage, "s2");
    }

    // =========================================================================
    // Routing — Temporal pattern (loop until signal)
    // =========================================================================

    #[test]
    fn test_temporal_loop_terminates_on_signal() {
        // not_(eq("completed", true)) → "s1" (keep looping)
        // When completed=true → not_(eq) is false → no match → no default → COMPLETED
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![
                    RoutingRule {
                        expr: RoutingExpr::Not {
                            expr: Box::new(RoutingExpr::Eq {
                                field: FieldRef::Current { key: "completed".to_string() },
                                value: serde_json::json!(true),
                            }),
                        },
                        target: "s1".to_string(),
                    },
                ], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // First iteration: no output → completed missing → not_(eq) = true → loops
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s1");
        assert!(!orch.pipelines.get(&ProcessId::must("p1")).unwrap().terminated);

        // Second iteration: completed=false → not_(eq(false,true)) = true → loops
        let mut output = HashMap::new();
        output.insert("completed".to_string(), serde_json::json!(false));
        envelope.outputs.insert("a1".to_string(), output);
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s1");
        assert!(!orch.pipelines.get(&ProcessId::must("p1")).unwrap().terminated);

        // Third iteration: completed=true → not_(eq(true,true)) = false → no match → terminate
        let mut output2 = HashMap::new();
        output2.insert("completed".to_string(), serde_json::json!(true));
        envelope.outputs.insert("a1".to_string(), output2);
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.terminated);
        assert_eq!(session.terminal_reason, Some(TerminalReason::Completed));
    }

    // =========================================================================
    // Routing — linear chain, cycles, edge limits
    // =========================================================================

    #[test]
    fn test_routing_linear_chain() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![always_to("s3")], None),
                stage("s3", "a3", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s3");

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.terminated);
        assert_eq!(session.terminal_reason, Some(TerminalReason::Completed));
    }

    #[test]
    fn test_routing_backward_cycle_terminated_by_bounds() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![always_to("s1")], None),
            ],
            max_iterations: 4,
            max_llm_calls: 50,
            max_agent_hops: 50,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..20 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

            let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
            if session.terminated {
                terminated = true;
                assert_eq!(session.terminal_reason, Some(TerminalReason::MaxIterationsExceeded));
                break;
            }
        }
        assert!(terminated, "Cycle should have been terminated by bounds");
    }

    #[test]
    fn test_edge_limit_enforcement() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![always_to("s1")], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![EdgeLimit {
                from_stage: "s2".to_string(),
                to_stage: "s1".to_string(),
                max_count: 2,
            }],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..20 {
            let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
            if session.terminated {
                terminated = true;
                assert_eq!(session.terminal_reason, Some(TerminalReason::MaxAgentHopsExceeded));
                break;
            }
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        }
        assert!(terminated, "Edge limit should have terminated the cycle");
    }

    #[test]
    fn test_edge_traversal_tracking() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert_eq!(session.edge_traversals.get("stage1->stage2"), Some(&1));
    }

    // =========================================================================
    // max_visits enforcement
    // =========================================================================

    #[test]
    fn test_max_visits_terminates_when_exceeded() {
        let mut s2 = stage("s2", "a2", vec![always_to("s1")], None);
        s2.max_visits = None;
        let mut s1 = stage("s1", "a1", vec![always_to("s2")], None);
        s1.max_visits = Some(2); // s1 can be visited at most 2 times

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![s1, s2],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // visit 1: s1 → s2 (s1 visits=1)
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");

        // s2 → s1 (s1 visits still 1, about to visit again)
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s1");

        // visit 2: s1 → s2 (s1 visits=2)
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");

        // s2 tries to route to s1, but s1 has max_visits=2 and visits=2 → terminate
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.terminated);
        assert_eq!(session.terminal_reason, Some(TerminalReason::MaxStageVisitsExceeded));
    }

    // =========================================================================
    // RoutingExpr unit tests (evaluate_expr directly)
    // =========================================================================

    #[test]
    fn test_evaluate_expr_neq() {
        let outputs: HashMap<String, HashMap<String, Value>> = HashMap::new();
        let metadata: HashMap<String, Value> = HashMap::new();

        // Field doesn't exist → Neq returns false (field must exist)
        let expr = RoutingExpr::Neq {
            field: FieldRef::Current { key: "k".to_string() },
            value: serde_json::json!("x"),
        };
        assert!(!evaluate_expr(&expr, &outputs, "a1", &metadata, None));

        // Field exists and differs → true
        let mut agent_out = HashMap::new();
        agent_out.insert("k".to_string(), serde_json::json!("y"));
        let mut outputs2 = HashMap::new();
        outputs2.insert("a1".to_string(), agent_out);
        assert!(evaluate_expr(&expr, &outputs2, "a1", &metadata, None));

        // Field exists and matches → false
        let mut agent_out2 = HashMap::new();
        agent_out2.insert("k".to_string(), serde_json::json!("x"));
        let mut outputs3 = HashMap::new();
        outputs3.insert("a1".to_string(), agent_out2);
        assert!(!evaluate_expr(&expr, &outputs3, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_gt_lt() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let mut agent_out = HashMap::new();
        agent_out.insert("score".to_string(), serde_json::json!(0.8));
        let mut outputs = HashMap::new();
        outputs.insert("a1".to_string(), agent_out);

        let gt = RoutingExpr::Gt {
            field: FieldRef::Current { key: "score".to_string() },
            value: serde_json::json!(0.5),
        };
        assert!(evaluate_expr(&gt, &outputs, "a1", &metadata, None));

        let lt = RoutingExpr::Lt {
            field: FieldRef::Current { key: "score".to_string() },
            value: serde_json::json!(0.5),
        };
        assert!(!evaluate_expr(&lt, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_contains_string() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let mut agent_out = HashMap::new();
        agent_out.insert("text".to_string(), serde_json::json!("hello world"));
        let mut outputs = HashMap::new();
        outputs.insert("a1".to_string(), agent_out);

        let expr = RoutingExpr::Contains {
            field: FieldRef::Current { key: "text".to_string() },
            value: serde_json::json!("world"),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, None));

        let expr2 = RoutingExpr::Contains {
            field: FieldRef::Current { key: "text".to_string() },
            value: serde_json::json!("missing"),
        };
        assert!(!evaluate_expr(&expr2, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_exists_not_exists() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let mut agent_out = HashMap::new();
        agent_out.insert("k".to_string(), serde_json::json!("v"));
        let mut outputs = HashMap::new();
        outputs.insert("a1".to_string(), agent_out);

        let exists = RoutingExpr::Exists { field: FieldRef::Current { key: "k".to_string() } };
        assert!(evaluate_expr(&exists, &outputs, "a1", &metadata, None));

        let not_exists = RoutingExpr::NotExists { field: FieldRef::Current { key: "k".to_string() } };
        assert!(!evaluate_expr(&not_exists, &outputs, "a1", &metadata, None));

        let exists_missing = RoutingExpr::Exists { field: FieldRef::Current { key: "nope".to_string() } };
        assert!(!evaluate_expr(&exists_missing, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_and_or_not() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let outputs: HashMap<String, HashMap<String, Value>> = HashMap::new();

        let t = RoutingExpr::Always;
        let f = RoutingExpr::Not { expr: Box::new(RoutingExpr::Always) };

        let and_tt = RoutingExpr::And { exprs: vec![t.clone(), t.clone()] };
        assert!(evaluate_expr(&and_tt, &outputs, "a1", &metadata, None));

        let and_tf = RoutingExpr::And { exprs: vec![t.clone(), f.clone()] };
        assert!(!evaluate_expr(&and_tf, &outputs, "a1", &metadata, None));

        let or_tf = RoutingExpr::Or { exprs: vec![t.clone(), f.clone()] };
        assert!(evaluate_expr(&or_tf, &outputs, "a1", &metadata, None));

        let or_ff = RoutingExpr::Or { exprs: vec![f.clone(), f.clone()] };
        assert!(!evaluate_expr(&or_ff, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_field_ref_agent() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let mut agent_out = HashMap::new();
        agent_out.insert("topic".to_string(), serde_json::json!("time"));
        let mut outputs = HashMap::new();
        outputs.insert("understand".to_string(), agent_out);

        let expr = RoutingExpr::Eq {
            field: FieldRef::Agent { agent: "understand".to_string(), key: "topic".to_string() },
            value: serde_json::json!("time"),
        };
        assert!(evaluate_expr(&expr, &outputs, "current", &metadata, None));

        // Agent hasn't run → field doesn't exist → Eq returns false
        let expr2 = RoutingExpr::Eq {
            field: FieldRef::Agent { agent: "missing_agent".to_string(), key: "topic".to_string() },
            value: serde_json::json!("time"),
        };
        assert!(!evaluate_expr(&expr2, &outputs, "current", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_field_ref_meta_dot_notation() {
        let mut metadata: HashMap<String, Value> = HashMap::new();
        metadata.insert("session_context".to_string(), serde_json::json!({
            "has_history": true,
            "nested": { "deep": 42 }
        }));
        let outputs: HashMap<String, HashMap<String, Value>> = HashMap::new();

        let expr = RoutingExpr::Eq {
            field: FieldRef::Meta { path: "session_context.has_history".to_string() },
            value: serde_json::json!(true),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, None));

        let expr2 = RoutingExpr::Eq {
            field: FieldRef::Meta { path: "session_context.nested.deep".to_string() },
            value: serde_json::json!(42),
        };
        assert!(evaluate_expr(&expr2, &outputs, "a1", &metadata, None));

        // Missing path
        let expr3 = RoutingExpr::Exists {
            field: FieldRef::Meta { path: "session_context.nonexistent".to_string() },
        };
        assert!(!evaluate_expr(&expr3, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_field_ref_interrupt() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let outputs: HashMap<String, HashMap<String, Value>> = HashMap::new();

        let interrupt_val = serde_json::json!({
            "approved": true,
            "text": "yes go ahead",
        });

        let expr = RoutingExpr::Eq {
            field: FieldRef::Interrupt { key: "approved".to_string() },
            value: serde_json::json!(true),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, Some(&interrupt_val)));

        // No interrupt response → field doesn't exist
        assert!(!evaluate_expr(&expr, &outputs, "a1", &metadata, None));
    }

    // =========================================================================
    // Parallel execution tests
    // =========================================================================

    fn parallel_stage(name: &str, agent: &str, group: &str, routing: Vec<RoutingRule>, default_next: Option<&str>) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: agent.to_string(),
            routing,
            default_next: default_next.map(|s| s.to_string()),
            error_next: None,
            max_visits: None,
            parallel_group: Some(group.to_string()),
            join_strategy: JoinStrategy::WaitAll,
        }
    }

    #[test]
    fn test_parallel_fan_out_fan_in() {
        // Pipeline: context → [think_a, think_b, think_c] (parallel) → resolve → terminate
        // Routing: context → think_a (which has parallel_group="think")
        // think_a owns fan-in routing: default_next="resolve"
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("context", "ctx_agent", vec![always_to("think_a")], None),
                parallel_stage("think_a", "agent_a", "think", vec![], Some("resolve")),
                parallel_stage("think_b", "agent_b", "think", vec![], None),
                parallel_stage("think_c", "agent_c", "think", vec![], None),
                stage("resolve", "resolver", vec![], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // context runs → routes to think_a → detect parallel group → fan-out
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Session should now have active parallel
        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.active_parallel.is_some());
        let parallel = session.active_parallel.as_ref().unwrap();
        assert_eq!(parallel.group_name, "think");
        assert_eq!(parallel.stages.len(), 3);
        assert!(parallel.pending_agents.contains("agent_a"));
        assert!(parallel.pending_agents.contains("agent_b"));
        assert!(parallel.pending_agents.contains("agent_c"));
        assert!(envelope.pipeline.parallel_mode);

        // get_next_instruction → RunAgents with all 3
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::RunAgents);
        assert_eq!(instr.agents.len(), 3);

        // Report agent_a result
        envelope.pipeline.current_stage = "think_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Still waiting (WaitAll, 2 pending)
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::WaitParallel);

        // Report agent_b result
        envelope.pipeline.current_stage = "think_b".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Still waiting (1 pending)
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::WaitParallel);

        // Report agent_c result — join met!
        envelope.pipeline.current_stage = "think_c".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Parallel complete → routing on first stage (think_a) → default_next="resolve"
        assert_eq!(envelope.pipeline.current_stage, "resolve");
        assert!(!envelope.pipeline.parallel_mode);
        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.active_parallel.is_none());
        assert!(!session.terminated);

        // Resolve runs → no routing → terminate
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.terminated);
        assert_eq!(session.terminal_reason, Some(TerminalReason::Completed));
    }

    #[test]
    fn test_parallel_wait_first_strategy() {
        // WaitFirst: join met when first agent completes
        let mut think_a = parallel_stage("think_a", "agent_a", "think", vec![], Some("resolve"));
        think_a.join_strategy = JoinStrategy::WaitFirst;
        let mut think_b = parallel_stage("think_b", "agent_b", "think", vec![], None);
        think_b.join_strategy = JoinStrategy::WaitFirst;

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("start", "starter", vec![always_to("think_a")], None),
                think_a,
                think_b,
                stage("resolve", "resolver", vec![], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // start → fan-out to parallel group
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert!(orch.pipelines.get(&ProcessId::must("p1")).unwrap().active_parallel.is_some());

        // Report agent_a → WaitFirst join met immediately
        envelope.pipeline.current_stage = "think_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Should have resolved routing → advance to resolve
        assert_eq!(envelope.pipeline.current_stage, "resolve");
        assert!(orch.pipelines.get(&ProcessId::must("p1")).unwrap().active_parallel.is_none());
    }

    // =========================================================================
    // Serde round-trip tests
    // =========================================================================

    #[test]
    fn test_pipeline_config_serde_roundtrip() {
        let config = PipelineConfig {
            name: "roundtrip_test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s2")], Some("s2")),
                stage("s2", "a2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 50,
            max_agent_hops: 5,
            edge_limits: vec![EdgeLimit {
                from_stage: "s1".to_string(),
                to_stage: "s2".to_string(),
                max_count: 3,
            }],
            step_limit: Some(100),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: PipelineConfig = serde_json::from_str(&json).unwrap();
        // Re-serialize and compare JSON values for equality
        let json2 = serde_json::to_string(&deserialized).unwrap();
        let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&json2).unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_routing_rule_serde_roundtrip() {
        let rule = RoutingRule {
            expr: RoutingExpr::And {
                exprs: vec![
                    RoutingExpr::Or {
                        exprs: vec![
                            RoutingExpr::Eq {
                                field: FieldRef::Current { key: "intent".to_string() },
                                value: serde_json::json!("greet"),
                            },
                            RoutingExpr::Neq {
                                field: FieldRef::Agent { agent: "other".to_string(), key: "status".to_string() },
                                value: serde_json::json!("done"),
                            },
                        ],
                    },
                    RoutingExpr::Not {
                        expr: Box::new(RoutingExpr::Exists {
                            field: FieldRef::Meta { path: "skip".to_string() },
                        }),
                    },
                ],
            },
            target: "next_stage".to_string(),
        };
        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: RoutingRule = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&deserialized).unwrap();
        let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&json2).unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_field_ref_serde_roundtrip() {
        let variants: Vec<FieldRef> = vec![
            FieldRef::Current { key: "intent".to_string() },
            FieldRef::Agent { agent: "understand".to_string(), key: "topic".to_string() },
            FieldRef::Meta { path: "session.has_history".to_string() },
            FieldRef::Interrupt { key: "approved".to_string() },
        ];
        for field_ref in variants {
            let json = serde_json::to_string(&field_ref).unwrap();
            let deserialized: FieldRef = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&deserialized).unwrap();
            let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
            let v2: serde_json::Value = serde_json::from_str(&json2).unwrap();
            assert_eq!(v1, v2, "FieldRef round-trip failed for: {}", json);
        }
    }

    #[test]
    fn test_pipeline_config_default_fields() {
        // Minimal JSON: only required fields
        let json = r#"{
            "name": "minimal",
            "agents": [],
            "max_iterations": 5,
            "max_llm_calls": 10,
            "max_agent_hops": 3,
            "edge_limits": []
        }"#;
        let config: PipelineConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "minimal");
        assert!(config.agents.is_empty());
        assert_eq!(config.max_iterations, 5);
        assert_eq!(config.max_llm_calls, 10);
        assert_eq!(config.max_agent_hops, 3);
        assert!(config.edge_limits.is_empty());
        assert!(config.step_limit.is_none());
    }

    #[test]
    fn test_instruction_serde_without_envelope() {
        // Instruction no longer has envelope field
        let instruction = Instruction {
            kind: InstructionKind::RunAgent,
            agents: vec!["agent1".to_string()],
            terminal_reason: None,
            termination_message: None,
            interrupt_pending: false,
            interrupt: None,
        };
        let json = serde_json::to_string(&instruction).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Verify no "envelope" key in serialized JSON
        assert!(parsed.get("envelope").is_none(), "Instruction should not contain envelope field");
        // Verify round-trip
        let deserialized: Instruction = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.kind, InstructionKind::RunAgent);
        assert_eq!(deserialized.agents, vec!["agent1"]);
        assert!(!deserialized.interrupt_pending);
    }

    // =========================================================================
    // step_limit tests
    // =========================================================================

    #[test]
    fn test_step_limit_stored_on_session() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s1")], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: Some(3),
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // step_limit is preserved on the session's pipeline config
        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert_eq!(session.pipeline_config.step_limit, Some(3));
    }

    #[test]
    fn test_step_limit_none_is_unlimited() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s1")], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Run many iterations — should not terminate from step_limit
        for _ in 0..50 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
            let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
            if session.terminated {
                // Should be from max_iterations (100), not step_limit
                assert_ne!(session.terminal_reason, None);
                break;
            }
        }
        // After 50 iterations, should not be terminated (max_iterations=100)
        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(!session.terminated, "Pipeline should not terminate with step_limit=None after 50 iterations");
    }

    #[test]
    fn test_step_limit_interacts_with_max_iterations() {
        // step_limit=5, max_iterations=3 → max_iterations should trigger first
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s1")], None),
            ],
            max_iterations: 3,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: Some(5),
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..10 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
            let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
            if session.terminated {
                terminated = true;
                assert_eq!(session.terminal_reason, Some(TerminalReason::MaxIterationsExceeded));
                break;
            }
        }
        assert!(terminated, "Pipeline should have terminated from max_iterations");
    }

    // =========================================================================
    // Parallel execution tests (new)
    // =========================================================================

    #[test]
    fn test_parallel_fan_out_wait_all() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("start", "starter", vec![always_to("think_a")], None),
                parallel_stage("think_a", "agent_a", "think", vec![], Some("done")),
                parallel_stage("think_b", "agent_b", "think", vec![], None),
                stage("done", "finisher", vec![], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // start → fan-out
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // get_next_instruction → RunAgents with both agents
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::RunAgents);
        assert_eq!(instr.agents.len(), 2);

        // Report agent_a
        envelope.pipeline.current_stage = "think_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Still waiting (WaitAll)
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::WaitParallel);

        // Report agent_b → join met, routes to "done"
        envelope.pipeline.current_stage = "think_b".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "done");
    }

    #[test]
    fn test_parallel_fan_out_wait_first() {
        let mut think_a = parallel_stage("think_a", "agent_a", "think", vec![], Some("done"));
        think_a.join_strategy = JoinStrategy::WaitFirst;
        let mut think_b = parallel_stage("think_b", "agent_b", "think", vec![], None);
        think_b.join_strategy = JoinStrategy::WaitFirst;

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("start", "starter", vec![always_to("think_a")], None),
                think_a,
                think_b,
                stage("done", "finisher", vec![], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // start → fan-out
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Report only agent_a → WaitFirst join met immediately
        envelope.pipeline.current_stage = "think_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Should proceed without waiting for agent_b
        assert_eq!(envelope.pipeline.current_stage, "done");
        assert!(orch.pipelines.get(&ProcessId::must("p1")).unwrap().active_parallel.is_none());
    }

    #[test]
    fn test_parallel_group_routing_uses_first_stage() {
        // First stage (think_a) has routing to "special", second (think_b) has routing to "other"
        // After join, routing should use think_a's rules
        let mut think_a = parallel_stage("think_a", "agent_a", "think", vec![always_to("special")], None);
        think_a.join_strategy = JoinStrategy::WaitAll;
        let mut think_b = parallel_stage("think_b", "agent_b", "think", vec![always_to("other")], None);
        think_b.join_strategy = JoinStrategy::WaitAll;

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("start", "starter", vec![always_to("think_a")], None),
                think_a,
                think_b,
                stage("special", "sp_agent", vec![], None),
                stage("other", "ot_agent", vec![], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // start → fan-out
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Dispatch and complete both
        let _instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        envelope.pipeline.current_stage = "think_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        envelope.pipeline.current_stage = "think_b".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();

        // Routing resolved using first stage (think_a) → "special", not "other"
        assert_eq!(envelope.pipeline.current_stage, "special");
    }

    // =========================================================================
    // max_visits tests (new)
    // =========================================================================

    #[test]
    fn test_max_visits_terminates() {
        let mut s1 = stage("s1", "a1", vec![always_to("s1")], None);
        s1.max_visits = Some(3);

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![s1],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..10 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
            let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
            if session.terminated {
                terminated = true;
                assert_eq!(session.terminal_reason, Some(TerminalReason::MaxStageVisitsExceeded));
                break;
            }
        }
        assert!(terminated, "Should have terminated with MaxStageVisitsExceeded");
    }

    #[test]
    fn test_max_visits_none_allows_unlimited() {
        let s1 = stage("s1", "a1", vec![always_to("s1")], None);
        // max_visits defaults to None in stage() helper

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![s1],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Run 50 iterations — should not terminate from max_visits
        for _ in 0..50 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
            let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
            if session.terminated {
                // If terminated, it should NOT be MaxStageVisitsExceeded
                assert_ne!(session.terminal_reason, Some(TerminalReason::MaxStageVisitsExceeded));
                return;
            }
        }
        // Still running after 50 iterations — max_visits=None works
        assert!(!orch.pipelines.get(&ProcessId::must("p1")).unwrap().terminated);
    }

    // =========================================================================
    // error_next tests (new)
    // =========================================================================

    #[test]
    fn test_error_next_routes_on_failure() {
        let mut s1 = stage("s1", "a1", vec![always_to("s2")], None);
        s1.error_next = Some("recovery".to_string());

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                s1,
                stage("s2", "a2", vec![], None),
                stage("recovery", "rec_agent", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // agent_failed=true → should route to error_next "recovery"
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, true).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "recovery");
    }

    #[test]
    fn test_error_next_ignored_on_success() {
        let mut s1 = stage("s1", "a1", vec![always_to("s2")], None);
        s1.error_next = Some("recovery".to_string());

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                s1,
                stage("s2", "a2", vec![], None),
                stage("recovery", "rec_agent", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // agent_failed=false → should use normal routing (Always → s2), not error_next
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");
    }

    // =========================================================================
    // Edge cases
    // =========================================================================

    #[test]
    fn test_join_strategy_copy() {
        // JoinStrategy should implement Copy
        let a = JoinStrategy::WaitAll;
        let b = a; // Copy
        let _c = a; // Still usable after copy
        assert_eq!(a, b);
        assert_eq!(a, JoinStrategy::WaitAll);
    }

    #[test]
    fn test_edge_limit_exceeded() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![always_to("s1")], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![EdgeLimit {
                from_stage: "s1".to_string(),
                to_stage: "s2".to_string(),
                max_count: 2,
            }],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..20 {
            let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
            if session.terminated {
                terminated = true;
                assert_eq!(session.terminal_reason, Some(TerminalReason::MaxAgentHopsExceeded));
                break;
            }
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        }
        assert!(terminated, "Edge limit should have terminated the cycle");
    }

    #[test]
    fn test_empty_routing_falls_to_default_next() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![], Some("foo")),
                stage("foo", "a_foo", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "foo");
    }

    #[test]
    fn test_empty_routing_no_default_terminates() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            agents: vec![
                stage("s1", "a1", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false).unwrap();
        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.terminated);
        assert_eq!(session.terminal_reason, Some(TerminalReason::Completed));
    }
}

