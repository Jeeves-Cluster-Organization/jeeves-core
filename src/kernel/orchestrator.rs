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

use crate::envelope::{Envelope, TerminalReason};
use crate::types::{Error, ProcessId, Result};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use tracing::instrument;

// Re-export types from split modules so existing `orchestrator::X` paths keep working.
pub use super::orchestrator_types::*;
pub use super::routing::{evaluate_expr, evaluate_routing, FieldRef, RoutingExpr, RoutingRule};

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
    // NOTE: No terminated/terminal_reason — lives exclusively in envelope.bounds
    pub edge_traversals: HashMap<String, i32>, // "from->to" -> count
    pub stage_visits: HashMap<String, i32>,    // stage_name -> visit count
    pub active_parallel: Option<ParallelGroupState>,
    pub created_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
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
    #[instrument(skip(self, pipeline_config, envelope), fields(process_id = %process_id))]
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
    #[instrument(skip(self, envelope), fields(process_id = %process_id))]
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
        if envelope.bounds.terminated {
            return Ok(Instruction {
                kind: InstructionKind::Terminate,
                agents: vec![],
                terminal_reason: envelope.bounds.terminal_reason,
                termination_message: Some("Session already terminated".to_string()),
                interrupt_pending: false,
                interrupt: None,
                agent_context: None,
                output_schema: None,
                allowed_tools: None,
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
                agent_context: None,
                output_schema: None,
                allowed_tools: None,
            });
        }

        // Check bounds
        if let Some(reason) = envelope.check_bounds() {
            tracing::warn!(reason = ?reason, "bounds_terminated");
            envelope.bounds.terminated = true;
            envelope.bounds.terminal_reason = Some(reason);
            return Ok(Instruction {
                kind: InstructionKind::Terminate,
                agents: vec![],
                terminal_reason: Some(reason),
                termination_message: Some(format!("Bounds exceeded: {:?}", reason)),
                interrupt_pending: false,
                interrupt: None,
                agent_context: None,
                output_schema: None,
                allowed_tools: None,
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
                    agent_context: None,
                    output_schema: None,
                    allowed_tools: None,
                });
            }

            if !parallel.pending_agents.is_empty() {
                // Parallel group started — dispatch all pending agents
                let agents: Vec<String> = parallel.pending_agents.drain().collect();
                tracing::info!(group = %parallel.group_name, ?agents, "parallel_fanout");
                return Ok(Instruction {
                    kind: InstructionKind::RunAgents,
                    agents,
                    terminal_reason: None,
                    termination_message: None,
                    interrupt_pending: false,
                    interrupt: None,
                    agent_context: None,
                    output_schema: None,
                    allowed_tools: None,
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
                agent_context: None,
                output_schema: None,
                allowed_tools: None,
            });
        }

        // Determine next agent to run (single agent)
        let current_stage = &envelope.pipeline.current_stage;
        if current_stage.is_empty() {
            return Err(Error::state_transition("No current stage set"));
        }

        // Gate dispatch: evaluate routing without running any agent
        let stage_config = session.pipeline_config.stages.iter()
            .find(|s| s.name == *current_stage)
            .cloned();
        if let Some(ref sc) = stage_config {
            if sc.node_kind == NodeKind::Gate {
                // Track visit + increment iteration
                let current_stage_owned = current_stage.to_string();
                *session.stage_visits.entry(current_stage_owned.clone()).or_insert(0) += 1;
                envelope.pipeline.iteration += 1;

                // Bounds check
                if let Some(reason) = envelope.check_bounds() {
                    envelope.bounds.terminated = true;
                    envelope.bounds.terminal_reason = Some(reason);
                    return Ok(Instruction {
                        kind: InstructionKind::Terminate,
                        agents: vec![],
                        terminal_reason: Some(reason),
                        termination_message: Some(format!("Gate bounds exceeded: {:?}", reason)),
                        interrupt_pending: false,
                        interrupt: None,
                        agent_context: None,
                        output_schema: None,
                        allowed_tools: None,
                    });
                }

                // Gate routing: no agent execution, no interrupt_response
                let interrupt_response = envelope.interrupts.interrupt.as_ref()
                    .and_then(|i| i.response.as_ref())
                    .and_then(|r| serde_json::to_value(r).ok());
                let next = evaluate_routing(
                    sc,
                    &envelope.outputs,
                    &sc.name,
                    false,
                    &envelope.audit.metadata,
                    interrupt_response.as_ref(),
                    &envelope.state,
                );
                self.apply_routing_result(process_id, &current_stage_owned, next, envelope)?;

                // Recurse — next stage might also be a Gate
                return self.get_next_instruction(process_id, envelope);
            }

            // Fork dispatch: evaluate ALL routing rules, collect matching targets
            if sc.node_kind == NodeKind::Fork {
                let current_stage_owned = current_stage.to_string();
                *session.stage_visits.entry(current_stage_owned.clone()).or_insert(0) += 1;
                envelope.pipeline.iteration += 1;

                if let Some(reason) = envelope.check_bounds() {
                    envelope.bounds.terminated = true;
                    envelope.bounds.terminal_reason = Some(reason);
                    return Ok(Instruction {
                        kind: InstructionKind::Terminate,
                        agents: vec![],
                        terminal_reason: Some(reason),
                        termination_message: Some(format!("Fork bounds exceeded: {:?}", reason)),
                        interrupt_pending: false,
                        interrupt: None,
                        agent_context: None,
                        output_schema: None,
                        allowed_tools: None,
                    });
                }

                // ALL-match: every matching rule contributes a target
                let mut targets: Vec<String> = vec![];
                for rule in &sc.routing {
                    if evaluate_expr(&rule.expr, &envelope.outputs, &sc.name, &envelope.audit.metadata, None, &envelope.state) {
                        if !targets.contains(&rule.target) {
                            targets.push(rule.target.clone());
                        }
                    }
                }
                if targets.is_empty() {
                    if let Some(ref d) = sc.default_next {
                        targets.push(d.clone());
                    }
                }

                match targets.len() {
                    0 => {
                        // No targets — terminate COMPLETED
                        envelope.bounds.terminated = true;
                        envelope.bounds.terminal_reason = Some(TerminalReason::Completed);
                        session.last_activity_at = Utc::now();
                        return Ok(Instruction {
                            kind: InstructionKind::Terminate,
                            agents: vec![],
                            terminal_reason: Some(TerminalReason::Completed),
                            termination_message: None,
                            interrupt_pending: false,
                            interrupt: None,
                            agent_context: None,
                            output_schema: None,
                            allowed_tools: None,
                        });
                    }
                    1 => {
                        // Single target — advance sequentially
                        let target = targets.into_iter().next().unwrap();
                        self.apply_routing_result(process_id, &current_stage_owned, Some(target), envelope)?;
                        return self.get_next_instruction(process_id, envelope);
                    }
                    _ => {
                        // Multiple targets — set up parallel state
                        let agent_names: HashSet<String> = targets.iter()
                            .filter_map(|t| get_agent_for_stage(&session.pipeline_config, t).ok())
                            .collect();
                        let stage_names: Vec<String> = targets;

                        session.active_parallel = Some(ParallelGroupState {
                            group_name: current_stage_owned.clone(),
                            stages: stage_names.clone(),
                            pending_agents: agent_names,
                            completed_agents: HashSet::new(),
                            failed_agents: HashMap::new(),
                            join_strategy: sc.join_strategy,
                        });

                        envelope.pipeline.parallel_mode = true;
                        envelope.pipeline.active_stages = stage_names.into_iter().collect();
                        envelope.pipeline.current_stage = current_stage_owned;
                        session.last_activity_at = Utc::now();

                        // Recurse to dispatch the pending agents
                        return self.get_next_instruction(process_id, envelope);
                    }
                }
            }
        }

        let agent_name = get_agent_for_stage(&session.pipeline_config, current_stage)?;

        Ok(Instruction {
            kind: InstructionKind::RunAgent,
            agents: vec![agent_name],
            terminal_reason: None,
            termination_message: None,
            interrupt_pending: false,
            interrupt: None,
            agent_context: None,
            output_schema: None,
            allowed_tools: None,
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
    #[instrument(skip(self, metrics, envelope), fields(process_id = %process_id, agent_failed))]
    pub fn report_agent_result(
        &mut self,
        process_id: &ProcessId,
        metrics: AgentExecutionMetrics,
        envelope: &mut Envelope,
        agent_failed: bool,
        break_loop: bool,
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
        if let Some(reason) = envelope.check_bounds() {
            envelope.bounds.terminated = true;
            envelope.bounds.terminal_reason = Some(reason);
            return Ok(());
        }

        // Break check — agent decided to exit scope
        if break_loop {
            if session.active_parallel.is_some() {
                // In fork context: break = early completion of this branch
                // Mark agent completed, fall through to join logic below
                // (don't terminate the whole pipeline)
            } else {
                // Top-level: break = pipeline exit
                envelope.bounds.terminated = true;
                envelope.bounds.terminal_reason = Some(TerminalReason::BreakRequested);
                session.last_activity_at = Utc::now();
                return Ok(());
            }
        }

        // ===== PARALLEL GROUP HANDLING =====
        // Step 1: Mark agent completed/failed (needs &mut)
        if let Some(parallel) = session.active_parallel.as_mut() {
            let current_stage = envelope.pipeline.current_stage.clone();
            let agent_name_for_parallel = get_agent_for_stage(&session.pipeline_config, &current_stage)
                .unwrap_or_default();
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

        // Step 2: Check join condition (needs &ref — reborrow fine after mut released)
        let join_met = session.active_parallel.as_ref()
            .map(is_parallel_join_met)
            .unwrap_or(false);

        if session.active_parallel.is_some() && !join_met {
            // More agents pending — wait
            session.last_activity_at = Utc::now();
            return Ok(());
        }

        // Step 3: Take ownership if join met — Fork node owns post-join routing
        if let Some(parallel) = session.active_parallel.take() {
            envelope.pipeline.parallel_mode = false;
            envelope.pipeline.active_stages.clear();

            // Mark all parallel stages as completed
            for stage_name in &parallel.stages {
                *session.stage_visits.entry(stage_name.clone()).or_insert(0) += 1;
                envelope.complete_stage(stage_name);
            }

            // Fork node owns post-join routing via its default_next
            let fork_node = session.pipeline_config.stages
                .iter()
                .find(|s| s.name == parallel.group_name)
                .cloned();

            let next_target = fork_node.as_ref().and_then(|f| f.default_next.clone());

            return self.apply_routing_result(
                process_id,
                &parallel.group_name,
                next_target,
                envelope,
            );
        }

        // ===== NORMAL (NON-PARALLEL) ROUTING =====
        let current_stage = envelope.pipeline.current_stage.clone();

        // Look up the PipelineStage config for the current stage
        let pipeline_stage = session.pipeline_config.stages
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
            &envelope.state,
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
                if let Some(target_stage) = session.pipeline_config.stages.iter().find(|s| s.name == target) {
                    if let Some(max_visits) = target_stage.max_visits {
                        let visits = session.stage_visits.get(&target).copied().unwrap_or(0);
                        if visits >= max_visits {
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

                // Normal advance to target stage
                tracing::info!(from = %from_stage, to = %target, "stage_transition");
                envelope.pipeline.current_stage = target;
                session.last_activity_at = Utc::now();
            }
            None => {
                // No routing matched, no default_next — pipeline complete (Temporal pattern)
                tracing::info!(reason = ?TerminalReason::Completed, "pipeline_completed");
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

    /// Get the output_schema for a specific stage in a process's pipeline.
    /// Returns a cloned Value so the caller can use it without holding a borrow.
    pub fn get_stage_output_schema(&self, process_id: &ProcessId, stage_name: &str) -> Option<serde_json::Value> {
        self.pipelines.get(process_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
                    .and_then(|s| s.output_schema.clone())
            })
    }

    /// Get the allowed_tools for a specific stage in a process's pipeline.
    pub fn get_stage_allowed_tools(&self, process_id: &ProcessId, stage_name: &str) -> Option<Vec<String>> {
        self.pipelines.get(process_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
                    .and_then(|s| s.allowed_tools.clone())
            })
    }

    /// Get the state_schema for a process's pipeline.
    pub fn get_state_schema(&self, process_id: &ProcessId) -> Option<&Vec<StateField>> {
        self.pipelines.get(process_id)
            .map(|session| &session.pipeline_config.state_schema)
    }

    /// Get the output_key for a stage, defaulting to the stage name.
    pub fn get_stage_output_key(&self, process_id: &ProcessId, stage_name: &str) -> Option<String> {
        self.pipelines.get(process_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
                    .map(|s| s.output_key.clone().unwrap_or_else(|| s.name.clone()))
            })
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
            .unwrap_or_default();

        SessionState {
            process_id: session.process_id.clone(),
            current_stage: envelope.pipeline.current_stage.clone(),
            stage_order: envelope.pipeline.stage_order.clone(),
            envelope: envelope_value,
            edge_traversals: session.edge_traversals.clone(),
            terminated: envelope.bounds.terminated,
            terminal_reason: envelope.bounds.terminal_reason,
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

/// Get agent name for a stage.
fn get_agent_for_stage(
    pipeline_config: &PipelineConfig,
    stage_name: &str,
) -> Result<String> {
    pipeline_config
        .stages
        .iter()
        .find(|s| s.name == stage_name)
        .map(|s| s.agent.clone())
        .ok_or_else(|| Error::not_found(format!("Stage not found in pipeline: {}", stage_name)))
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
            join_strategy: JoinStrategy::default(),
            output_schema: None,
            allowed_tools: None,
            node_kind: NodeKind::default(),
            output_key: None,
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

    fn agent_eq_rule(agent: &str, key: &str, value: impl Into<Value>, target: &str) -> RoutingRule {
        RoutingRule {
            expr: RoutingExpr::Eq {
                field: FieldRef::Agent { agent: agent.to_string(), key: key.to_string() },
                value: value.into(),
            },
            target: target.to_string(),
        }
    }

    fn zero_metrics() -> AgentExecutionMetrics {
        AgentExecutionMetrics::default()
    }

    /// Linear 2-stage pipeline: stage1 --(Always)--> stage2 --(no rules, no default)--> terminate
    fn create_test_pipeline() -> PipelineConfig {
        PipelineConfig {
            name: "test_pipeline".to_string(),
            stages: vec![
                stage("stage1", "agent1", vec![always_to("stage2")], None),
                stage("stage2", "agent2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 50,
            max_agent_hops: 5,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
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

        // Termination state lives exclusively in envelope.bounds
        envelope.bounds.terminated = true;
        envelope.bounds.terminal_reason = Some(TerminalReason::MaxIterationsExceeded);

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
            crate::envelope::FlowInterrupt::new(crate::envelope::InterruptKind::Clarification)
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

        orch.report_agent_result(&ProcessId::must("proc1"), metrics, &mut envelope, false, false)
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

        orch.report_agent_result(&ProcessId::must("proc1"), metrics, &mut envelope, false, false)
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
        pipeline2.stages = vec![];
        assert!(pipeline2.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_routing_target() {
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("nonexistent")], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_default_next() {
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![], Some("nonexistent")),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_error_next() {
        let mut s = stage("s1", "a1", vec![], None);
        s.error_next = Some("nonexistent".to_string());
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![s],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_max_visits_zero_rejected() {
        let mut s = stage("s1", "a1", vec![], None);
        s.max_visits = Some(0);
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![s],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_zero_max_iterations_rejected() {
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![stage("s1", "a1", vec![], None)],
            max_iterations: 0,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_negative_bounds_rejected() {
        // Negative values can arrive via JSON deserialization — verify <= 0 catches them
        for (iterations, llm, hops) in [(-1, 10, 10), (10, -5, 10), (10, 10, -3)] {
            let pipeline = PipelineConfig {
                name: "test".to_string(),
                stages: vec![stage("s1", "a1", vec![], None)],
                max_iterations: iterations,
                max_llm_calls: llm,
                max_agent_hops: hops,
                edge_limits: vec![],
                step_limit: None,
                state_schema: vec![],
            };
            assert!(pipeline.validate().is_err(), "Expected error for ({}, {}, {})", iterations, llm, hops);
        }
    }

    #[test]
    fn test_validation_zero_max_llm_calls_rejected() {
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![stage("s1", "a1", vec![], None)],
            max_iterations: 10,
            max_llm_calls: 0,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_zero_max_agent_hops_rejected() {
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![stage("s1", "a1", vec![], None)],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 0,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        assert!(pipeline.validate().is_err());
    }

    #[test]
    fn test_validation_zero_edge_limit_max_count_rejected() {
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![EdgeLimit {
                from_stage: "s1".to_string(),
                to_stage: "s2".to_string(),
                max_count: 0,
            }],
            step_limit: None,
            state_schema: vec![],
        };
        assert!(pipeline.validate().is_err());
    }

    // =========================================================================
    // Routing — integrated tests
    // =========================================================================

    #[test]
    fn test_routing_eq_match() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
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
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Agent a1 outputs intent="skip" → routes to s3
        let mut output = HashMap::new();
        output.insert("intent".to_string(), serde_json::json!("skip"));
        envelope.outputs.insert("a1".to_string(), output);

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "s3");
        assert!(!envelope.bounds.terminated);
    }

    #[test]
    fn test_routing_always_acts_as_catch_all() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "s2");
        assert!(!envelope.bounds.terminated);
    }

    #[test]
    fn test_routing_first_match_wins() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
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
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut output = HashMap::new();
        output.insert("intent".to_string(), serde_json::json!("greet"));
        envelope.outputs.insert("a1".to_string(), output);

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        // First rule wins → s2, not s3
        assert_eq!(envelope.pipeline.current_stage, "s2");
    }

    #[test]
    fn test_routing_default_next_fallback() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![], Some("s2")),
                stage("s2", "a2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "s2");
        assert!(!envelope.bounds.terminated);
    }

    #[test]
    fn test_routing_no_rules_no_default_terminates() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::Completed));
    }

    #[test]
    fn test_routing_error_next_on_failure() {
        let mut s1 = stage("s1", "a1", vec![always_to("s2")], None);
        s1.error_next = Some("s3".to_string());
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                s1,
                stage("s2", "a2", vec![], None),
                stage("s3", "a3", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, true, false).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "s3");
    }

    #[test]
    fn test_routing_failure_without_error_next_falls_through_to_rules() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, true, false).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "s2");
    }

    // =========================================================================
    // Temporal pattern (loop until signal)
    // =========================================================================

    #[test]
    fn test_temporal_loop_terminates_on_signal() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
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
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // First iteration: no output → completed missing → not_(eq) = true → loops
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s1");
        assert!(!envelope.bounds.terminated);

        // Second iteration: completed=false → not_(eq(false,true)) = true → loops
        let mut output = HashMap::new();
        output.insert("completed".to_string(), serde_json::json!(false));
        envelope.outputs.insert("a1".to_string(), output);
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s1");
        assert!(!envelope.bounds.terminated);

        // Third iteration: completed=true → not_(eq(true,true)) = false → no match → terminate
        let mut output2 = HashMap::new();
        output2.insert("completed".to_string(), serde_json::json!(true));
        envelope.outputs.insert("a1".to_string(), output2);
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::Completed));
    }

    // =========================================================================
    // Linear chain, cycles, edge limits
    // =========================================================================

    #[test]
    fn test_routing_linear_chain() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![always_to("s3")], None),
                stage("s3", "a3", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s3");

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::Completed));
    }

    #[test]
    fn test_routing_backward_cycle_terminated_by_bounds() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![always_to("s1")], None),
            ],
            max_iterations: 4,
            max_llm_calls: 50,
            max_agent_hops: 50,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..20 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

            if envelope.bounds.terminated {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::MaxIterationsExceeded));
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
            stages: vec![
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
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..20 {
            if envelope.bounds.terminated {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::MaxAgentHopsExceeded));
                break;
            }
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        }
        assert!(terminated, "Edge limit should have terminated the cycle");
    }

    #[test]
    fn test_edge_traversal_tracking() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

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
        s1.max_visits = Some(2);

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![s1, s2],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // visit 1: s1 → s2 (s1 visits=1)
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");

        // s2 → s1 (s1 visits still 1, about to visit again)
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s1");

        // visit 2: s1 → s2 (s1 visits=2)
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");

        // s2 tries to route to s1, but s1 has max_visits=2 and visits=2 → terminate
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::MaxStageVisitsExceeded));
    }

    // =========================================================================
    // Parallel execution tests
    // =========================================================================

    fn fork_stage(name: &str, routing: Vec<RoutingRule>, default_next: Option<&str>, join: JoinStrategy) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            routing,
            default_next: default_next.map(|s| s.to_string()),
            error_next: None,
            max_visits: None,
            join_strategy: join,
            output_schema: None,
            allowed_tools: None,
            node_kind: NodeKind::Fork,
            output_key: None,
        }
    }

    #[test]
    fn test_fork_fan_out_wait_all() {
        // Fork with 3 always() rules → 3 parallel branches, WaitAll
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("context", "ctx_agent", vec![always_to("fork")], None),
                fork_stage("fork", vec![
                    always_to("think_a"),
                    always_to("think_b"),
                    always_to("think_c"),
                ], Some("resolve"), JoinStrategy::WaitAll),
                stage("think_a", "agent_a", vec![], None),
                stage("think_b", "agent_b", vec![], None),
                stage("think_c", "agent_c", vec![], None),
                stage("resolve", "resolver", vec![], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // context agent reports → routes to fork
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        // get_next_instruction detects Fork → sets up parallel → dispatches agents
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::RunAgents);
        assert_eq!(instr.agents.len(), 3);

        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.active_parallel.is_some());
        let par = session.active_parallel.as_ref().unwrap();
        assert_eq!(par.group_name, "fork");
        assert_eq!(par.stages.len(), 3);
        assert!(envelope.pipeline.parallel_mode);

        // Report all 3 branches
        envelope.pipeline.current_stage = "think_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::WaitParallel);

        envelope.pipeline.current_stage = "think_b".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::WaitParallel);

        envelope.pipeline.current_stage = "think_c".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        // After all 3 branches complete, fork advances to resolve via default_next
        assert_eq!(envelope.pipeline.current_stage, "resolve");
        assert!(!envelope.pipeline.parallel_mode);
        assert!(!envelope.bounds.terminated);

        // Resolve completes
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::Completed));
    }

    #[test]
    fn test_fork_wait_first_strategy() {
        // Fork with WaitFirst — first completion advances immediately
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("start", "starter", vec![always_to("fork")], None),
                fork_stage("fork", vec![
                    always_to("think_a"),
                    always_to("think_b"),
                ], Some("resolve"), JoinStrategy::WaitFirst),
                stage("think_a", "agent_a", vec![], None),
                stage("think_b", "agent_b", vec![], None),
                stage("resolve", "resolver", vec![], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        // get_next_instruction sets up Fork parallel state
        let _instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert!(orch.pipelines.get(&ProcessId::must("p1")).unwrap().active_parallel.is_some());

        // First branch completes → should advance immediately
        envelope.pipeline.current_stage = "think_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

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
            stages: vec![
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
            state_schema: vec![],
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: PipelineConfig = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&deserialized).unwrap();
        let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&json2).unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_pipeline_config_default_fields() {
        let json = r#"{
            "name": "minimal",
            "stages": [],
            "max_iterations": 5,
            "max_llm_calls": 10,
            "max_agent_hops": 3,
            "edge_limits": []
        }"#;
        let config: PipelineConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "minimal");
        assert!(config.stages.is_empty());
        assert_eq!(config.max_iterations, 5);
        assert_eq!(config.max_llm_calls, 10);
        assert_eq!(config.max_agent_hops, 3);
        assert!(config.edge_limits.is_empty());
        assert!(config.step_limit.is_none());
    }

    #[test]
    fn test_instruction_serde_without_envelope() {
        let instruction = Instruction {
            kind: InstructionKind::RunAgent,
            agents: vec!["agent1".to_string()],
            terminal_reason: None,
            termination_message: None,
            interrupt_pending: false,
            interrupt: None,
            agent_context: None,
            output_schema: None,
            allowed_tools: None,
        };
        let json = serde_json::to_string(&instruction).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("envelope").is_none(), "Instruction should not contain envelope field");
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
            stages: vec![
                stage("s1", "a1", vec![always_to("s1")], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: Some(3),
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert_eq!(session.pipeline_config.step_limit, Some(3));
    }

    #[test]
    fn test_step_limit_none_is_unlimited() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s1")], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        for _ in 0..50 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
            if envelope.bounds.terminated {
                assert_ne!(envelope.bounds.terminal_reason, None);
                break;
            }
        }
        assert!(!envelope.bounds.terminated, "Pipeline should not terminate with step_limit=None after 50 iterations");
    }

    #[test]
    fn test_step_limit_interacts_with_max_iterations() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s1")], None),
            ],
            max_iterations: 3,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: Some(5),
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..10 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
            if envelope.bounds.terminated {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::MaxIterationsExceeded));
                break;
            }
        }
        assert!(terminated, "Pipeline should have terminated from max_iterations");
    }

    // =========================================================================
    // Additional tests
    // =========================================================================

    #[test]
    fn test_fork_conditional_targets() {
        // Fork with conditional rules → only matching branches dispatched
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                fork_stage("fork", vec![
                    always_to("branch_a"),
                    eq_rule("x", 999, "branch_b"),  // won't match — no output
                ], Some("done"), JoinStrategy::WaitAll),
                stage("branch_a", "agent_a", vec![], None),
                stage("branch_b", "agent_b", vec![], None),
                stage("done", "finisher", vec![], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Fork dispatches only branch_a (always matches), not branch_b (eq never matches)
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        // Single target → sequential advance (not RunAgents)
        assert_eq!(instr.kind, InstructionKind::RunAgent);
        assert_eq!(instr.agents, vec!["agent_a"]);
    }

    #[test]
    fn test_fork_post_join_uses_default_next() {
        // Fork node's default_next is the post-join target
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                fork_stage("fork", vec![
                    always_to("branch_a"),
                    always_to("branch_b"),
                ], Some("final"), JoinStrategy::WaitAll),
                stage("branch_a", "agent_a", vec![], None),
                stage("branch_b", "agent_b", vec![], None),
                stage("final", "final_agent", vec![], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let _instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        envelope.pipeline.current_stage = "branch_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        envelope.pipeline.current_stage = "branch_b".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        // After both branches, should advance to "final" (Fork's default_next)
        assert_eq!(envelope.pipeline.current_stage, "final");
    }

    #[test]
    fn test_max_visits_terminates() {
        let mut s1 = stage("s1", "a1", vec![always_to("s1")], None);
        s1.max_visits = Some(3);

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![s1],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..10 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
            if envelope.bounds.terminated {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::MaxStageVisitsExceeded));
                break;
            }
        }
        assert!(terminated, "Should have terminated with MaxStageVisitsExceeded");
    }

    #[test]
    fn test_max_visits_none_allows_unlimited() {
        let s1 = stage("s1", "a1", vec![always_to("s1")], None);

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![s1],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        for _ in 0..50 {
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
            if envelope.bounds.terminated {
                assert_ne!(envelope.bounds.terminal_reason, Some(TerminalReason::MaxStageVisitsExceeded));
                return;
            }
        }
        assert!(!envelope.bounds.terminated);
    }

    #[test]
    fn test_error_next_routes_on_failure() {
        let mut s1 = stage("s1", "a1", vec![always_to("s2")], None);
        s1.error_next = Some("recovery".to_string());

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                s1,
                stage("s2", "a2", vec![], None),
                stage("recovery", "rec_agent", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, true, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "recovery");
    }

    #[test]
    fn test_error_next_ignored_on_success() {
        let mut s1 = stage("s1", "a1", vec![always_to("s2")], None);
        s1.error_next = Some("recovery".to_string());

        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                s1,
                stage("s2", "a2", vec![], None),
                stage("recovery", "rec_agent", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");
    }

    #[test]
    fn test_join_strategy_copy() {
        let a = JoinStrategy::WaitAll;
        let b = a;
        let _c = a;
        assert_eq!(a, b);
        assert_eq!(a, JoinStrategy::WaitAll);
    }

    #[test]
    fn test_edge_limit_exceeded() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
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
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..20 {
            if envelope.bounds.terminated {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::MaxAgentHopsExceeded));
                break;
            }
            orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        }
        assert!(terminated, "Edge limit should have terminated the cycle");
    }

    #[test]
    fn test_empty_routing_falls_to_default_next() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![], Some("foo")),
                stage("foo", "a_foo", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "foo");
    }

    // =========================================================================
    // Phase 1c audit tests
    // =========================================================================

    #[test]
    fn test_has_session_true_after_init() {
        let mut orch = Orchestrator::new();
        let pid = ProcessId::must("p1");
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();
        orch.initialize_session(pid.clone(), pipeline, &mut envelope, false).unwrap();

        assert!(orch.has_session(&pid));
    }

    #[test]
    fn test_has_session_false_for_unknown_pid() {
        let orch = Orchestrator::new();
        assert!(!orch.has_session(&ProcessId::must("unknown")));
    }

    #[test]
    fn test_cleanup_stale_sessions_removes_old_keeps_young() {
        let mut orch = Orchestrator::new();

        // Create two sessions
        let pid_old = ProcessId::must("old");
        let pid_young = ProcessId::must("young");
        let pipeline = create_test_pipeline();

        let mut env1 = create_test_envelope();
        orch.initialize_session(pid_old.clone(), pipeline.clone(), &mut env1, false).unwrap();

        let mut env2 = create_test_envelope();
        orch.initialize_session(pid_young.clone(), pipeline, &mut env2, false).unwrap();

        // Manually set the old session's last_activity_at to the past
        if let Some(session) = orch.pipelines.get_mut(&pid_old) {
            session.last_activity_at = Utc::now() - chrono::TimeDelta::seconds(3600);
        }

        // Cleanup sessions older than 60 seconds
        let removed = orch.cleanup_stale_sessions(60);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], pid_old);

        assert!(!orch.has_session(&pid_old));
        assert!(orch.has_session(&pid_young));
    }

    #[test]
    fn test_check_bounds_agent_hops_path() {
        let mut envelope = create_test_envelope();
        envelope.bounds.max_agent_hops = 3;
        envelope.bounds.agent_hop_count = 2;

        // Below limit — no termination
        assert!(envelope.check_bounds().is_none());

        // At limit — terminates
        envelope.bounds.agent_hop_count = 3;
        assert_eq!(envelope.check_bounds(), Some(TerminalReason::MaxAgentHopsExceeded));

        // Above limit — also terminates
        envelope.bounds.agent_hop_count = 4;
        assert_eq!(envelope.check_bounds(), Some(TerminalReason::MaxAgentHopsExceeded));
    }

    #[test]
    fn test_check_bounds_iteration_path() {
        let mut envelope = create_test_envelope();
        envelope.pipeline.max_iterations = 5;
        envelope.pipeline.iteration = 4;
        assert!(envelope.check_bounds().is_none());

        envelope.pipeline.iteration = 5;
        assert_eq!(envelope.check_bounds(), Some(TerminalReason::MaxIterationsExceeded));
    }

    #[test]
    fn test_empty_routing_no_default_terminates() {
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![], None),
            ],
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::Completed));
    }

    // =========================================================================
    // Gate dispatch tests
    // =========================================================================

    fn gate_stage(name: &str, routing: Vec<RoutingRule>, default_next: Option<&str>) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            routing,
            default_next: default_next.map(|s| s.to_string()),
            error_next: None,
            max_visits: None,
            join_strategy: JoinStrategy::default(),
            output_schema: None,
            allowed_tools: None,
            node_kind: NodeKind::Gate,
            output_key: None,
        }
    }

    #[test]
    fn test_gate_routes_without_agent() {
        // Gate with 2 routing rules → correct agent dispatched, no Gate agent execution
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("entry", "entry_agent", vec![always_to("router")], None),
                gate_stage("router", vec![
                    agent_eq_rule("entry_agent", "choice", "a", "agent_a"),
                    agent_eq_rule("entry_agent", "choice", "b", "agent_b"),
                ], None),
                stage("agent_a", "agent_a", vec![], None),
                stage("agent_b", "agent_b", vec![], None),
            ],
            max_iterations: 20,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // First instruction: run entry agent
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::RunAgent);
        assert_eq!(instr.agents, vec!["entry_agent"]);

        // Entry agent reports output with choice=a, routes to router Gate
        let mut agent_output = std::collections::HashMap::new();
        agent_output.insert("choice".to_string(), serde_json::json!("a"));
        envelope.outputs.insert("entry_agent".to_string(), agent_output);
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();

        // Next instruction should skip the Gate and dispatch agent_a directly
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::RunAgent);
        assert_eq!(instr.agents, vec!["agent_a"]);
    }

    #[test]
    fn test_gate_chain() {
        // Gate→Gate→Agent — both Gates skipped, correct agent runs
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                gate_stage("gate1", vec![always_to("gate2")], None),
                gate_stage("gate2", vec![always_to("final")], None),
                stage("final", "final_agent", vec![], None),
            ],
            max_iterations: 20,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Should skip both gates and return RunAgent for final_agent
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::RunAgent);
        assert_eq!(instr.agents, vec!["final_agent"]);
    }

    #[test]
    fn test_gate_default_next() {
        // Gate with no matching rules + default_next → falls through
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                gate_stage("router", vec![
                    eq_rule("x", 999, "never_match"),
                ], Some("fallback")),
                stage("never_match", "nm_agent", vec![], None),
                stage("fallback", "fb_agent", vec![], None),
            ],
            max_iterations: 20,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::RunAgent);
        assert_eq!(instr.agents, vec!["fb_agent"]);
    }

    #[test]
    fn test_gate_no_match_terminates() {
        // Gate with no matching rules, no default_next → COMPLETED
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                gate_stage("router", vec![
                    eq_rule("x", 999, "never_match"),
                ], None),
                stage("never_match", "nm_agent", vec![], None),
            ],
            max_iterations: 20,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::Terminate);
        assert_eq!(instr.terminal_reason, Some(TerminalReason::Completed));
    }

    #[test]
    fn test_gate_respects_bounds() {
        // Gate in a self-loop → terminates when iteration limit hit
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                gate_stage("loop_gate", vec![always_to("loop_gate")], None),
            ],
            max_iterations: 5,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::Terminate);
        // Should terminate due to iteration bounds
        assert!(instr.terminal_reason.is_some());
        let reason = instr.terminal_reason.unwrap();
        assert_eq!(reason.outcome(), "bounds_exceeded");
    }

    #[test]
    fn test_gate_with_interrupt_pending() {
        // Interrupt check happens BEFORE Gate dispatch
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                gate_stage("router", vec![always_to("target")], None),
                stage("target", "target_agent", vec![], None),
            ],
            max_iterations: 20,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Set interrupt pending
        envelope.interrupts.interrupt_pending = true;

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::WaitInterrupt);
    }

    // =========================================================================
    // Break tests
    // =========================================================================

    #[test]
    fn test_break_terminates_pipeline() {
        // break_loop=true → BREAK_REQUESTED, outcome "completed"
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s2")], None),
                stage("s2", "a2", vec![], None),
            ],
            max_iterations: 20,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Agent reports break
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, true).unwrap();

        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::BreakRequested));

        // Verify outcome is "completed" (clean exit)
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert_eq!(instr.kind, InstructionKind::Terminate);
        assert_eq!(instr.terminal_reason.unwrap().outcome(), "completed");
    }

    #[test]
    fn test_break_stores_output_first() {
        // Output is stored in envelope.outputs before termination
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s1")], None),
            ],
            max_iterations: 20,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Store output before reporting (simulates kernel/mod.rs behavior)
        let mut agent_output = std::collections::HashMap::new();
        agent_output.insert("result".to_string(), serde_json::json!("done"));
        envelope.outputs.insert("a1".to_string(), agent_output);

        // Break
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, true).unwrap();

        // Output should still be there
        assert!(envelope.outputs.contains_key("a1"));
        assert_eq!(envelope.outputs["a1"]["result"], serde_json::json!("done"));
        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::BreakRequested));
    }

    #[test]
    fn test_break_exits_loop() {
        // Agent in routing loop, reports break_loop=true → clean exit
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("loop", "loop_agent", vec![always_to("loop")], None),
            ],
            max_iterations: 100,
            max_llm_calls: 100,
            max_agent_hops: 100,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // First iteration — no break, should loop
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(!envelope.bounds.terminated);
        assert_eq!(envelope.pipeline.current_stage, "loop");

        // Second iteration — break
        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, true).unwrap();
        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::BreakRequested));
    }

    #[test]
    fn test_break_wins_over_interrupt() {
        // break_loop=true checked BEFORE interrupt injection
        let mut orch = Orchestrator::new();
        let pipeline = PipelineConfig {
            name: "test".to_string(),
            stages: vec![
                stage("s1", "a1", vec![always_to("s1")], None),
            ],
            max_iterations: 20,
            max_llm_calls: 50,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
        };
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        // Set interrupt pending — but break should win
        envelope.interrupts.interrupt_pending = true;

        orch.report_agent_result(&ProcessId::must("p1"), zero_metrics(), &mut envelope, false, true).unwrap();
        assert!(envelope.bounds.terminated);
        assert_eq!(envelope.bounds.terminal_reason, Some(TerminalReason::BreakRequested));
    }
}
