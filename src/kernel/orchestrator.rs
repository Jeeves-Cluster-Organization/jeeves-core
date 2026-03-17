//! Pipeline orchestration - kernel-driven pipeline execution control.
//!
//! The Orchestrator:
//!   - Owns the orchestration loop
//!   - Dispatches registered routing functions
//!   - Tracks edge traversals
//!   - Enforces bounds (iterations, LLM calls, agent hops)
//!   - Returns Instructions to the worker
//!
//! Workers:
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
pub use super::routing::{evaluate_routing_with_reason, evaluate_fork_routing, RoutingContext, RoutingDecision, RoutingFn, RoutingReason, RoutingRegistry, RoutingResult};

// Use helpers from split module
use super::orchestrator_helpers::{get_agent_for_stage, build_stage_snapshot, check_edge_limit, is_parallel_join_met};

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
    pub(crate) edge_traversals: HashMap<EdgeKey, i32>,
    pub(crate) stage_visits: HashMap<String, i32>,    // stage_name -> visit count
    pub(crate) active_parallel: Option<ParallelGroupState>,
    #[allow(dead_code)]  // Retained for checkpoint metadata
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) last_activity_at: DateTime<Utc>,
    /// Last routing decision made by report_agent_result (consumed by get_next_instruction).
    pub(crate) last_routing_decision: Option<super::routing::RoutingDecision>,
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
    pub(crate) pipelines: HashMap<ProcessId, PipelineSession>,
    pub(crate) routing_registry: RoutingRegistry,
}

impl Orchestrator {
    /// Create a new Orchestrator.
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
            routing_registry: RoutingRegistry::new(),
        }
    }

    /// Register a routing function by name.
    pub fn register_routing_fn(&mut self, name: impl Into<String>, f: std::sync::Arc<dyn RoutingFn>) {
        self.routing_registry.register(name, f);
    }

    /// Get the next instruction for a process.
    ///
    /// Envelope is passed in by the Kernel (which owns it). The envelope may be
    /// mutated (e.g., bounds termination) — the Kernel re-stores it afterward.
    ///
    /// Uses an iterative loop instead of recursion to handle Gate chains and
    /// Fork-to-single-target collapses without risking stack overflow.
    #[instrument(skip(self, envelope), fields(process_id = %process_id))]
    pub fn get_next_instruction(
        &mut self,
        process_id: &ProcessId,
        envelope: &mut Envelope,
    ) -> Result<Instruction> {
        // Use step_limit from PipelineConfig if set, otherwise default to 100.
        let max_steps = self
            .pipelines
            .get(process_id)
            .map(|s| s.pipeline_config.step_limit.unwrap_or(100))
            .unwrap_or(100) as usize;

        for _step in 0..max_steps {
            let session = self
                .pipelines
                .get_mut(process_id)
                .ok_or_else(|| Error::not_found(format!("Unknown process: {}", process_id)))?;

            // Update activity timestamp
            session.last_activity_at = Utc::now();

            // Check if already terminated
            if envelope.bounds.is_terminated() {
                return Ok(Instruction::terminate(
                    envelope.bounds.terminal_reason().unwrap_or(TerminalReason::Completed),
                    "Session already terminated",
                ));
            }

            // Check for pending interrupt
            if envelope.interrupts.is_pending() {
                // Auto-clear expired interrupts
                let expired = envelope.interrupts.interrupt.as_ref()
                    .and_then(|i| i.expires_at)
                    .map(|exp| Utc::now() > exp)
                    .unwrap_or(false);
                if expired {
                    envelope.clear_interrupt();
                    // Fall through to routing — don't block on expired interrupt
                } else {
                    return Ok(Instruction::WaitInterrupt {
                        interrupt: envelope.interrupts.interrupt.clone(),
                    });
                }
            }

            // Check bounds
            if let Some(reason) = envelope.check_bounds() {
                tracing::warn!(reason = ?reason, "bounds_terminated");
                envelope.bounds.terminate(reason, None);
                return Ok(Instruction::terminate(reason, format!("Bounds exceeded: {:?}", reason)));
            }

            // Check for active parallel group
            if let Some(ref mut parallel) = session.active_parallel {
                if is_parallel_join_met(parallel) {
                    // Join condition met — will be resolved in next report_agent_result
                    return Ok(Instruction::wait_parallel());
                }

                if !parallel.pending_agents.is_empty() {
                    // Parallel group started — dispatch all pending agents
                    let agents: Vec<String> = parallel.pending_agents.drain().collect();
                    tracing::info!(group = %parallel.group_name, ?agents, "parallel_fanout");
                    return Ok(Instruction::run_agents(agents));
                }

                // Agents dispatched, waiting for results
                return Ok(Instruction::wait_parallel());
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
                        envelope.bounds.terminate(reason, None);
                        return Ok(Instruction::terminate(reason, format!("Gate bounds exceeded: {:?}", reason)));
                    }

                    // Gate routing: no agent execution
                    let interrupt_response = envelope.interrupts.interrupt.as_ref()
                        .and_then(|i| i.response.as_ref())
                        .and_then(|r| serde_json::to_value(r).ok());
                    let ctx = RoutingContext {
                        current_stage: &current_stage_owned,
                        agent_name: &sc.name,
                        agent_failed: false,
                        outputs: &envelope.outputs,
                        metadata: &envelope.audit.metadata,
                        interrupt_response: interrupt_response.as_ref(),
                        state: &envelope.state,
                    };
                    let decision = evaluate_routing_with_reason(
                        sc,
                        &self.routing_registry,
                        &ctx,
                        &current_stage_owned,
                    );
                    let next = decision.target;
                    self.apply_routing_result(process_id, &current_stage_owned, next, envelope)?;

                    // Loop back — next stage might also be a Gate
                    continue;
                }

                // Fork dispatch: routing function returns fan-out targets
                if sc.node_kind == NodeKind::Fork {
                    let current_stage_owned = current_stage.to_string();
                    *session.stage_visits.entry(current_stage_owned.clone()).or_insert(0) += 1;
                    envelope.pipeline.iteration += 1;

                    if let Some(reason) = envelope.check_bounds() {
                        envelope.bounds.terminate(reason, None);
                        return Ok(Instruction::terminate(reason, format!("Fork bounds exceeded: {:?}", reason)));
                    }

                    // Fork: routing function returns fan-out targets
                    let fork_ctx = RoutingContext {
                        current_stage: &current_stage_owned,
                        agent_name: &sc.name,
                        agent_failed: false,
                        outputs: &envelope.outputs,
                        metadata: &envelope.audit.metadata,
                        interrupt_response: None,
                        state: &envelope.state,
                    };
                    let targets = evaluate_fork_routing(sc, &self.routing_registry, &fork_ctx);

                    match targets.len() {
                        0 => {
                            // No targets — terminate COMPLETED
                            envelope.bounds.terminate(TerminalReason::Completed, None);
                            // Re-acquire session for timestamp update
                            if let Some(s) = self.pipelines.get_mut(process_id) {
                                s.last_activity_at = Utc::now();
                            }
                            return Ok(Instruction::terminate_completed());
                        }
                        1 => {
                            // Single target — advance sequentially
                            if let Some(target) = targets.into_iter().next() {
                                self.apply_routing_result(process_id, &current_stage_owned, Some(target), envelope)?;
                                // Loop back — next stage might be another Gate/Fork
                                continue;
                            }
                        }
                        _ => {
                            // Multiple targets — set up parallel state
                            // Re-acquire session since we need to mutate it
                            let session = self.pipelines.get_mut(process_id)
                                .ok_or_else(|| Error::not_found(format!("Unknown process: {}", process_id)))?;
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

                            // Loop back to dispatch the pending agents
                            continue;
                        }
                    }
                }
            }

            let agent_name = get_agent_for_stage(&session.pipeline_config, current_stage)?;

            return Ok(Instruction::run_agent(agent_name));
        }

        // Step limit exceeded — terminate to prevent infinite Gate/Fork chains
        tracing::warn!(max_steps, "step_limit_exceeded in get_next_instruction");
        envelope.bounds.terminate(TerminalReason::MaxIterationsExceeded, Some(
            format!("Step limit ({}) exceeded in routing evaluation", max_steps),
        ));
        Ok(Instruction::terminate(
            TerminalReason::MaxIterationsExceeded,
            format!("Step limit ({}) exceeded in routing evaluation", max_steps),
        ))
    }

    /// Process agent execution result.
    ///
    /// Takes the updated envelope (Kernel extracted it from `process_envelopes`,
    /// the actor dispatch may have merged agent outputs into it). Updates metrics,
    /// advances the pipeline via routing evaluation, and mutates the envelope.
    ///
    /// Routing evaluation order:
    /// 1. If agent_failed AND error_next set → route to error_next
    /// 2. If routing_fn registered → call it
    /// 3. If no routing_fn → use default_next
    /// 4. If no default_next → terminate (COMPLETED)
    #[instrument(skip(self, metrics, envelope), fields(process_id = %process_id, agent_failed))]
    pub fn report_agent_result(
        &mut self,
        process_id: &ProcessId,
        agent_name: &str,
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
            envelope.bounds.terminate(reason, None);
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
                envelope.bounds.terminate(TerminalReason::BreakRequested, None);
                session.last_activity_at = Utc::now();
                return Ok(());
            }
        }

        // ===== PARALLEL GROUP HANDLING =====
        // Step 1: Mark agent completed/failed (needs &mut)
        if let Some(parallel) = session.active_parallel.as_mut() {
            parallel.pending_agents.remove(agent_name);
            if agent_failed {
                parallel.failed_agents.insert(
                    agent_name.to_string(),
                    "agent failed".to_string(),
                );
            } else {
                parallel.completed_agents.insert(agent_name.to_string());
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

            // Mark all parallel stages as completed + snapshot execution
            for stage_name in &parallel.stages {
                *session.stage_visits.entry(stage_name.clone()).or_insert(0) += 1;
                envelope.complete_stage(stage_name);

                if let Ok(agent) = get_agent_for_stage(&session.pipeline_config, stage_name) {
                    if let Some(agent_output) = envelope.outputs.get(&agent) {
                        envelope.pipeline.completed_stage_snapshots.push(
                            build_stage_snapshot(stage_name, agent_output),
                        );
                    }
                }
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

        // Snapshot completed stage into execution tracking
        if let Some(agent_output) = envelope.outputs.get(agent_name) {
            envelope.pipeline.completed_stage_snapshots.push(
                build_stage_snapshot(&current_stage, agent_output),
            );
        }

        // Get agent name for output lookup
        let agent_name = pipeline_stage.agent.clone();

        // Prepare interrupt response for routing evaluation
        let interrupt_response = envelope.interrupts.interrupt.as_ref()
            .and_then(|i| i.response.as_ref())
            .and_then(|r| serde_json::to_value(r).ok());

        // Evaluate routing via registered function (or fall through to default_next)
        let ctx = RoutingContext {
            current_stage: &current_stage,
            agent_name: &agent_name,
            agent_failed,
            outputs: &envelope.outputs,
            metadata: &envelope.audit.metadata,
            interrupt_response: interrupt_response.as_ref(),
            state: &envelope.state,
        };
        let routing_decision = evaluate_routing_with_reason(
            &pipeline_stage,
            &self.routing_registry,
            &ctx,
            &current_stage,
        );
        let next_target = routing_decision.target.clone();

        // Store routing decision on session for get_next_instruction to thread through
        // Re-borrow session mutably (previous borrow was dropped after pipeline_stage clone)
        if let Some(session) = self.pipelines.get_mut(process_id) {
            session.last_routing_decision = Some(routing_decision);
        }

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
                            envelope.bounds.terminate(
                                TerminalReason::MaxStageVisitsExceeded,
                                Some(format!("Stage '{}' exceeded max_visits limit of {}", target, max_visits)),
                            );
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
                    envelope.bounds.terminate(
                        TerminalReason::EdgeLimitExceeded,
                        Some(format!("Edge limit exceeded: {} -> {}", from_stage, target)),
                    );
                    session.last_activity_at = Utc::now();
                    return Ok(());
                }

                // Record edge traversal
                let edge_key = EdgeKey::new(from_stage, &target);
                *session.edge_traversals.entry(edge_key).or_insert(0) += 1;
                envelope.bounds.agent_hop_count += 1;

                // Normal advance to target stage
                tracing::info!(from = %from_stage, to = %target, "stage_transition");

                // Update execution tracking before target is moved
                if let Some(pos) = envelope.pipeline.stage_order.iter().position(|s| s == &target) {
                    envelope.pipeline.current_stage_number = (pos + 1) as i32;
                }

                envelope.pipeline.current_stage = target;
                session.last_activity_at = Utc::now();
            }
            None => {
                // No routing matched, no default_next — pipeline complete (Temporal pattern)
                tracing::info!(reason = ?TerminalReason::Completed, "pipeline_completed");
                envelope.bounds.terminate(TerminalReason::Completed, None);
                session.last_activity_at = Utc::now();
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::*;
    use crate::types::ProcessId;
    use std::sync::Arc;

    fn zero_metrics() -> AgentExecutionMetrics {
        AgentExecutionMetrics::default()
    }

    fn gate_stage(name: &str, routing_fn: Option<&str>, default_next: Option<&str>) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            routing_fn: routing_fn.map(|s| s.to_string()),
            default_next: default_next.map(|s| s.to_string()),
            node_kind: NodeKind::Gate,
            ..PipelineStage::default()
        }
    }

    fn fork_stage(name: &str, routing_fn: &str, default_next: Option<&str>, join: JoinStrategy) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            routing_fn: Some(routing_fn.to_string()),
            default_next: default_next.map(|s| s.to_string()),
            join_strategy: join,
            node_kind: NodeKind::Fork,
            ..PipelineStage::default()
        }
    }

    // =========================================================================
    // get_next_instruction
    // =========================================================================

    #[test]
    fn test_get_next_instruction_run_agent() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        let Instruction::RunAgent { ref agent, .. } = instruction else { panic!("Expected RunAgent") };
        assert_eq!(agent, "agent1");
    }

    #[test]
    fn test_get_next_instruction_terminate_after_terminated() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        // Termination state lives exclusively in envelope.bounds
        envelope.bounds.terminate(TerminalReason::MaxIterationsExceeded, None);

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        let Instruction::Terminate { reason, .. } = instruction else { panic!("Expected Terminate") };
        assert_eq!(reason, TerminalReason::MaxIterationsExceeded);
    }

    #[test]
    fn test_get_next_instruction_wait_interrupt() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        envelope.set_interrupt(
            crate::envelope::FlowInterrupt::new(crate::envelope::InterruptKind::Clarification)
                .with_message("Need clarification".to_string())
        );

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        let Instruction::WaitInterrupt { ref interrupt } = instruction else { panic!("Expected WaitInterrupt") };
        assert!(interrupt.is_some());
    }

    #[test]
    fn test_expired_interrupt_auto_cleared() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        // Set interrupt with past expiry (already expired)
        let expired_interrupt = crate::envelope::FlowInterrupt::new(crate::envelope::InterruptKind::Clarification)
            .with_message("Expired question".to_string());
        // Manually set expires_at to the past
        let mut interrupt = expired_interrupt;
        interrupt.expires_at = Some(Utc::now() - chrono::TimeDelta::seconds(60));
        envelope.set_interrupt(interrupt);

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        // get_next_instruction should auto-clear the expired interrupt and return RunAgent
        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        let Instruction::RunAgent { ref agent, .. } = instruction else {
            panic!("Expected RunAgent after expired interrupt auto-clear, got {:?}", instruction)
        };
        assert_eq!(agent, "agent1");
        assert!(!envelope.interrupts.is_pending());
    }

    #[test]
    fn test_check_bounds_llm_exceeded() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        envelope.bounds.llm_call_count = 50;
        envelope.bounds.max_llm_calls = 50;

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        let Instruction::Terminate { reason, .. } = instruction else { panic!("Expected Terminate") };
        assert_eq!(reason, TerminalReason::MaxLlmCallsExceeded);
    }

    #[test]
    fn test_check_bounds_iterations_exceeded() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        envelope.pipeline.iteration = 10;
        envelope.pipeline.max_iterations = 10;

        let instruction = orch.get_next_instruction(&ProcessId::must("proc1"), &mut envelope).unwrap();

        let Instruction::Terminate { reason, .. } = instruction else { panic!("Expected Terminate") };
        assert_eq!(reason, TerminalReason::MaxIterationsExceeded);
    }

    // =========================================================================
    // report_agent_result — metrics
    // =========================================================================

    #[test]
    fn test_report_agent_result_updates_metrics() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let metrics = AgentExecutionMetrics {
            llm_calls: 2,
            tool_calls: 5,
            tokens_in: Some(1000),
            tokens_out: Some(500),
            duration_ms: 1500,
            ..Default::default()
        };

        orch.report_agent_result(&ProcessId::must("proc1"), "agent1", metrics, &mut envelope, false, false)
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

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let metrics = AgentExecutionMetrics {
            llm_calls: 1,
            tool_calls: 1,
            tokens_in: None,
            tokens_out: None,
            duration_ms: 100,
            ..Default::default()
        };

        orch.report_agent_result(&ProcessId::must("proc1"), "agent1", metrics, &mut envelope, false, false)
            .unwrap();

        assert_eq!(envelope.bounds.tokens_in, 0);
        assert_eq!(envelope.bounds.tokens_out, 0);
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
    fn test_validation_invalid_default_next() {
        let config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("nonexistent")),
        ]);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_error_next() {
        let mut s = stage("s1", "a1", None, None);
        s.error_next = Some("nonexistent".to_string());
        let config = PipelineConfig::test_default("test", vec![s]);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_max_visits_zero_rejected() {
        let mut s = stage("s1", "a1", None, None);
        s.max_visits = Some(0);
        let config = PipelineConfig::test_default("test", vec![s]);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_zero_max_iterations_rejected() {
        let mut config = PipelineConfig::test_default("test", vec![stage("s1", "a1", None, None)]);
        config.max_iterations = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_negative_bounds_rejected() {
        for (iterations, llm, hops) in [(-1, 10, 10), (10, -5, 10), (10, 10, -3)] {
            let mut config = PipelineConfig::test_default("test", vec![stage("s1", "a1", None, None)]);
            config.max_iterations = iterations;
            config.max_llm_calls = llm;
            config.max_agent_hops = hops;
            assert!(config.validate().is_err(), "Expected error for ({}, {}, {})", iterations, llm, hops);
        }
    }

    #[test]
    fn test_validation_zero_edge_limit_max_count_rejected() {
        let mut config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s2")),
            stage("s2", "a2", None, None),
        ]);
        config.edge_limits = vec![EdgeLimit {
            from_stage: "s1".to_string(),
            to_stage: "s2".to_string(),
            max_count: 0,
        }];
        assert!(config.validate().is_err());
    }

    // =========================================================================
    // Routing — integrated tests
    // =========================================================================

    #[test]
    fn test_routing_fn_match() {
        let mut orch = Orchestrator::new();
        // Register routing fn: if intent=="skip" → s3, else → s2
        orch.register_routing_fn("intent_router", Arc::new(|ctx: &RoutingContext| {
            let intent = ctx.outputs.get("a1")
                .and_then(|o| o.get("intent"))
                .and_then(|v| v.as_str());
            match intent {
                Some("skip") => RoutingResult::Next("s3".to_string()),
                _ => RoutingResult::Next("s2".to_string()),
            }
        }));
        let config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", Some("intent_router"), None),
            stage("s2", "a2", None, None),
            stage("s3", "a3", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let mut output = HashMap::new();
        output.insert("intent".to_string(), serde_json::json!("skip"));
        envelope.outputs.insert("a1".to_string(), output);

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s3");
    }

    #[test]
    fn test_routing_default_next_fallback() {
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s2")),
            stage("s2", "a2", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");
        assert!(!envelope.bounds.is_terminated());
    }

    #[test]
    fn test_routing_no_fn_no_default_terminates() {
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.is_terminated());
        assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::Completed));
    }

    #[test]
    fn test_routing_error_next_on_failure() {
        let mut s1 = stage("s1", "a1", None, Some("s2"));
        s1.error_next = Some("s3".to_string());
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            s1,
            stage("s2", "a2", None, None),
            stage("s3", "a3", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, true, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s3");
    }

    #[test]
    fn test_routing_failure_without_error_next_falls_through() {
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s2")),
            stage("s2", "a2", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        // agent_failed=true, no error_next → falls through to default_next
        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, true, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");
    }

    // =========================================================================
    // Temporal pattern (loop until signal)
    // =========================================================================

    #[test]
    fn test_temporal_loop_terminates_on_signal() {
        let mut orch = Orchestrator::new();
        // Loop s1 until completed=true
        orch.register_routing_fn("loop_until_done", Arc::new(|ctx: &RoutingContext| {
            let completed = ctx.outputs.get(ctx.agent_name)
                .and_then(|o| o.get("completed"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if completed {
                RoutingResult::Terminate
            } else {
                RoutingResult::Next("s1".to_string())
            }
        }));
        let mut config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", Some("loop_until_done"), None),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        // First iteration: no output → not completed → loops
        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s1");
        assert!(!envelope.bounds.is_terminated());

        // Second iteration: completed=false → loops
        let mut output = HashMap::new();
        output.insert("completed".to_string(), serde_json::json!(false));
        envelope.outputs.insert("a1".to_string(), output);
        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s1");
        assert!(!envelope.bounds.is_terminated());

        // Third iteration: completed=true → terminates
        let mut output2 = HashMap::new();
        output2.insert("completed".to_string(), serde_json::json!(true));
        envelope.outputs.insert("a1".to_string(), output2);
        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.is_terminated());
        assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::Completed));
    }

    // =========================================================================
    // Linear chain, cycles, edge limits
    // =========================================================================

    #[test]
    fn test_routing_linear_chain() {
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s2")),
            stage("s2", "a2", None, Some("s3")),
            stage("s3", "a3", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s3");

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.is_terminated());
        assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::Completed));
    }

    #[test]
    fn test_routing_backward_cycle_terminated_by_bounds() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s2")),
            stage("s2", "a2", None, Some("s1")),
        ]);
        config.max_iterations = 4;
        config.max_llm_calls = 50;
        config.max_agent_hops = 50;
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..20 {
            orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
            if envelope.bounds.is_terminated() {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::MaxIterationsExceeded));
                break;
            }
        }
        assert!(terminated, "Cycle should have been terminated by bounds");
    }

    #[test]
    fn test_edge_limit_enforcement() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s2")),
            stage("s2", "a2", None, Some("s1")),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        config.edge_limits = vec![EdgeLimit {
            from_stage: "s2".to_string(),
            to_stage: "s1".to_string(),
            max_count: 2,
        }];
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..20 {
            if envelope.bounds.is_terminated() {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::EdgeLimitExceeded));
                break;
            }
            orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        }
        assert!(terminated, "Edge limit should have terminated the cycle");
    }

    #[test]
    fn test_edge_traversal_tracking() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), pipeline, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();

        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert_eq!(session.edge_traversals.get(&EdgeKey::new("stage1", "stage2")), Some(&1));
    }

    // =========================================================================
    // max_visits enforcement
    // =========================================================================

    #[test]
    fn test_max_visits_terminates_when_exceeded() {
        let mut s1 = stage("s1", "a1", None, Some("s2"));
        s1.max_visits = Some(2);

        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![s1, stage("s2", "a2", None, Some("s1"))]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        // visit 1: s1 → s2
        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");

        // s2 → s1
        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s1");

        // visit 2: s1 → s2
        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");

        // s2 tries to route to s1, but s1 has max_visits=2 and visits=2 → terminate
        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.is_terminated());
        assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::MaxStageVisitsExceeded));
    }

    // =========================================================================
    // Parallel execution tests
    // =========================================================================

    #[test]
    fn test_fork_fan_out_wait_all() {
        let mut orch = Orchestrator::new();
        orch.register_routing_fn("fan_abc", Arc::new(|_: &RoutingContext| {
            RoutingResult::Fan(vec!["think_a".into(), "think_b".into(), "think_c".into()])
        }));
        let mut config = PipelineConfig::test_default("test", vec![
            stage("context", "ctx_agent", None, Some("fork")),
            fork_stage("fork", "fan_abc", Some("resolve"), JoinStrategy::WaitAll),
            stage("think_a", "agent_a", None, None),
            stage("think_b", "agent_b", None, None),
            stage("think_c", "agent_c", None, None),
            stage("resolve", "resolver", None, None),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        // context agent reports → routes to fork
        orch.report_agent_result(&ProcessId::must("p1"), "ctx_agent", zero_metrics(), &mut envelope, false, false).unwrap();

        // get_next_instruction detects Fork → sets up parallel → dispatches agents
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::RunAgents { ref agents, .. } = instr else { panic!("Expected RunAgents") };
        assert_eq!(agents.len(), 3);

        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert!(session.active_parallel.is_some());
        assert!(envelope.pipeline.parallel_mode);

        // Report all 3 branches
        envelope.pipeline.current_stage = "think_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), "agent_a", zero_metrics(), &mut envelope, false, false).unwrap();
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert!(matches!(instr, Instruction::WaitParallel));

        envelope.pipeline.current_stage = "think_b".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), "agent_b", zero_metrics(), &mut envelope, false, false).unwrap();
        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert!(matches!(instr, Instruction::WaitParallel));

        envelope.pipeline.current_stage = "think_c".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), "agent_c", zero_metrics(), &mut envelope, false, false).unwrap();

        assert_eq!(envelope.pipeline.current_stage, "resolve");
        assert!(!envelope.pipeline.parallel_mode);
        assert!(!envelope.bounds.is_terminated());

        orch.report_agent_result(&ProcessId::must("p1"), "resolver", zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.is_terminated());
        assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::Completed));
    }

    #[test]
    fn test_fork_wait_first_strategy() {
        let mut orch = Orchestrator::new();
        orch.register_routing_fn("fan_ab", Arc::new(|_: &RoutingContext| {
            RoutingResult::Fan(vec!["think_a".into(), "think_b".into()])
        }));
        let mut config = PipelineConfig::test_default("test", vec![
            stage("start", "starter", None, Some("fork")),
            fork_stage("fork", "fan_ab", Some("resolve"), JoinStrategy::WaitFirst),
            stage("think_a", "agent_a", None, None),
            stage("think_b", "agent_b", None, None),
            stage("resolve", "resolver", None, None),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "starter", zero_metrics(), &mut envelope, false, false).unwrap();
        let _instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert!(orch.pipelines.get(&ProcessId::must("p1")).unwrap().active_parallel.is_some());

        // First branch completes → should advance immediately
        envelope.pipeline.current_stage = "think_a".to_string();
        orch.report_agent_result(&ProcessId::must("p1"), "agent_a", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "resolve");
        assert!(orch.pipelines.get(&ProcessId::must("p1")).unwrap().active_parallel.is_none());
    }

    // =========================================================================
    // Serde round-trip tests
    // =========================================================================

    #[test]
    fn test_pipeline_config_serde_roundtrip() {
        let mut config = PipelineConfig::test_default("roundtrip_test", vec![
            stage("s1", "a1", None, Some("s2")),
            stage("s2", "a2", None, None),
        ]);
        config.edge_limits = vec![EdgeLimit {
            from_stage: "s1".to_string(),
            to_stage: "s2".to_string(),
            max_count: 3,
        }];
        config.step_limit = Some(100);
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
    fn test_instruction_serde_roundtrip() {
        let instruction = Instruction::run_agent("agent1");
        let json = serde_json::to_string(&instruction).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.get("kind").unwrap().as_str().unwrap(), "RUN_AGENT");
        assert_eq!(parsed.get("agent").unwrap().as_str().unwrap(), "agent1");
        let deserialized: Instruction = serde_json::from_str(&json).unwrap();
        let Instruction::RunAgent { agent, .. } = deserialized else { panic!("Expected RunAgent") };
        assert_eq!(agent, "agent1");
    }

    // =========================================================================
    // step_limit tests
    // =========================================================================

    #[test]
    fn test_step_limit_stored_on_session() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s1")),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        config.step_limit = Some(3);
        // s1 self-loops need max_visits
        config.stages[0].max_visits = Some(100);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let session = orch.pipelines.get(&ProcessId::must("p1")).unwrap();
        assert_eq!(session.pipeline_config.step_limit, Some(3));
    }

    #[test]
    fn test_step_limit_none_is_unlimited() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s1")),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        config.stages[0].max_visits = Some(100);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        for _ in 0..50 {
            orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
            if envelope.bounds.is_terminated() {
                assert!(envelope.bounds.terminal_reason().is_some());
                break;
            }
        }
        assert!(!envelope.bounds.is_terminated());
    }

    #[test]
    fn test_step_limit_interacts_with_max_iterations() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s1")),
        ]);
        config.max_iterations = 3;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        config.step_limit = Some(5);
        config.stages[0].max_visits = Some(100);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..10 {
            orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
            if envelope.bounds.is_terminated() {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::MaxIterationsExceeded));
                break;
            }
        }
        assert!(terminated);
    }

    #[test]
    fn test_max_visits_terminates() {
        let mut s1 = stage("s1", "a1", None, Some("s1"));
        s1.max_visits = Some(3);
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![s1]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..10 {
            orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
            if envelope.bounds.is_terminated() {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::MaxStageVisitsExceeded));
                break;
            }
        }
        assert!(terminated);
    }

    #[test]
    fn test_error_next_routes_on_failure() {
        let mut s1 = stage("s1", "a1", None, Some("s2"));
        s1.error_next = Some("recovery".to_string());
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            s1,
            stage("s2", "a2", None, None),
            stage("recovery", "rec_agent", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, true, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "recovery");
    }

    #[test]
    fn test_error_next_ignored_on_success() {
        let mut s1 = stage("s1", "a1", None, Some("s2"));
        s1.error_next = Some("recovery".to_string());
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            s1,
            stage("s2", "a2", None, None),
            stage("recovery", "rec_agent", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s2");
    }

    #[test]
    fn test_edge_limit_exceeded() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s2")),
            stage("s2", "a2", None, Some("s1")),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        config.edge_limits = vec![EdgeLimit {
            from_stage: "s1".to_string(),
            to_stage: "s2".to_string(),
            max_count: 2,
        }];
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let mut terminated = false;
        for _ in 0..20 {
            if envelope.bounds.is_terminated() {
                terminated = true;
                assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::EdgeLimitExceeded));
                break;
            }
            orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        }
        assert!(terminated);
    }

    #[test]
    fn test_check_bounds_agent_hops_path() {
        let mut envelope = create_test_envelope();
        envelope.bounds.max_agent_hops = 3;
        envelope.bounds.agent_hop_count = 2;
        assert!(envelope.check_bounds().is_none());

        envelope.bounds.agent_hop_count = 3;
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

    // =========================================================================
    // Gate dispatch tests
    // =========================================================================

    #[test]
    fn test_gate_routes_without_agent() {
        let mut orch = Orchestrator::new();
        orch.register_routing_fn("choice_router", Arc::new(|ctx: &RoutingContext| {
            let choice = ctx.outputs.get("entry_agent")
                .and_then(|o| o.get("choice"))
                .and_then(|v| v.as_str());
            match choice {
                Some("a") => RoutingResult::Next("agent_a".to_string()),
                Some("b") => RoutingResult::Next("agent_b".to_string()),
                _ => RoutingResult::Terminate,
            }
        }));
        let config = PipelineConfig::test_default("test", vec![
            stage("entry", "entry_agent", None, Some("router")),
            gate_stage("router", Some("choice_router"), None),
            stage("agent_a", "agent_a", None, None),
            stage("agent_b", "agent_b", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::RunAgent { ref agent, .. } = instr else { panic!("Expected RunAgent") };
        assert_eq!(agent, "entry_agent");

        let mut agent_output = std::collections::HashMap::new();
        agent_output.insert("choice".to_string(), serde_json::json!("a"));
        envelope.outputs.insert("entry_agent".to_string(), agent_output);
        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::RunAgent { ref agent, .. } = instr else { panic!("Expected RunAgent") };
        assert_eq!(agent, "agent_a");
    }

    #[test]
    fn test_gate_chain() {
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            gate_stage("gate1", None, Some("gate2")),
            gate_stage("gate2", None, Some("final")),
            stage("final", "final_agent", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::RunAgent { ref agent, .. } = instr else { panic!("Expected RunAgent") };
        assert_eq!(agent, "final_agent");
    }

    #[test]
    fn test_gate_default_next() {
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            gate_stage("router", None, Some("fallback")),
            stage("fallback", "fb_agent", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::RunAgent { ref agent, .. } = instr else { panic!("Expected RunAgent") };
        assert_eq!(agent, "fb_agent");
    }

    #[test]
    fn test_gate_no_match_terminates() {
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            gate_stage("router", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::Terminate { reason, .. } = instr else { panic!("Expected Terminate") };
        assert_eq!(reason, TerminalReason::Completed);
    }

    #[test]
    fn test_gate_respects_bounds() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![
            gate_stage("loop_gate", None, Some("loop_gate")),
        ]);
        config.max_iterations = 5;
        config.stages[0].max_visits = Some(100);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::Terminate { reason, .. } = instr else { panic!("Expected Terminate") };
        assert_eq!(reason.outcome(), "bounds_exceeded");
    }

    #[test]
    fn test_gate_with_interrupt_pending() {
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            gate_stage("router", None, Some("target")),
            stage("target", "target_agent", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        envelope.set_interrupt(
            crate::envelope::FlowInterrupt::new(crate::envelope::InterruptKind::Clarification)
        );

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        assert!(matches!(instr, Instruction::WaitInterrupt { .. }));
    }

    // =========================================================================
    // Break tests
    // =========================================================================

    #[test]
    fn test_break_terminates_pipeline() {
        let mut orch = Orchestrator::new();
        let config = PipelineConfig::test_default("test", vec![
            stage("s1", "a1", None, Some("s2")),
            stage("s2", "a2", None, None),
        ]);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, true).unwrap();
        assert!(envelope.bounds.is_terminated());
        assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::BreakRequested));

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::Terminate { reason, .. } = instr else { panic!("Expected Terminate") };
        assert_eq!(reason.outcome(), "completed");
    }

    #[test]
    fn test_break_exits_loop() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("test", vec![
            stage("loop", "loop_agent", None, Some("loop")),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        config.stages[0].max_visits = Some(100);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(!envelope.bounds.is_terminated());
        assert_eq!(envelope.pipeline.current_stage, "loop");

        orch.report_agent_result(&ProcessId::must("p1"), "agent1", zero_metrics(), &mut envelope, false, true).unwrap();
        assert!(envelope.bounds.is_terminated());
        assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::BreakRequested));
    }

    // =========================================================================
    // step_limit with Gate chain
    // =========================================================================

    #[test]
    fn test_step_limit_terminates_gate_chain() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("gate_chain", vec![
            gate_stage("gate1", None, Some("gate2")),
            gate_stage("gate2", None, Some("gate3")),
            gate_stage("gate3", None, Some("gate4")),
            gate_stage("gate4", None, Some("gate5")),
            gate_stage("gate5", None, None),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        config.step_limit = Some(3);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::Terminate { reason, .. } = instr else {
            panic!("Expected Terminate due to step_limit, got {:?}", instr);
        };
        assert_eq!(reason, TerminalReason::MaxIterationsExceeded);
    }

    #[test]
    fn test_step_limit_allows_completion_if_sufficient() {
        let mut orch = Orchestrator::new();
        let mut config = PipelineConfig::test_default("gate_chain_ok", vec![
            gate_stage("gate1", None, Some("gate2")),
            gate_stage("gate2", None, Some("gate3")),
            gate_stage("gate3", None, Some("final")),
            stage("final", "final_agent", None, None),
        ]);
        config.max_iterations = 100;
        config.max_llm_calls = 100;
        config.max_agent_hops = 100;
        config.step_limit = Some(10);
        let mut envelope = create_test_envelope();
        orch.initialize_session(ProcessId::must("p1"), config, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&ProcessId::must("p1"), &mut envelope).unwrap();
        let Instruction::RunAgent { ref agent, .. } = instr else {
            panic!("Expected RunAgent, got {:?}", instr);
        };
        assert_eq!(agent, "final_agent");
        assert!(!envelope.bounds.is_terminated());
    }
}
