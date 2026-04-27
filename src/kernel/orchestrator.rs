//! Pipeline orchestration — kernel-driven pipeline execution control.
//!
//! The Orchestrator:
//!   - Owns the orchestration loop
//!   - Dispatches registered routing functions
//!   - Enforces bounds (iterations, LLM calls, agent hops, max_visits)
//!   - Returns Instructions to the worker
//!
//! Workers:
//!   - Execute agents (LLM calls, tool execution)
//!   - Report results back
//!   - Have NO control over what runs next

use crate::envelope::{Envelope, TerminalReason};
use crate::types::{Error, ProcessId, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tracing::instrument;

// Re-export types from split modules so existing `orchestrator::X` paths keep working.
pub use super::orchestrator_types::*;
pub use super::routing::{
    evaluate_routing_with_reason, RoutingContext, RoutingDecision, RoutingFn, RoutingReason,
    RoutingRegistry, RoutingResult,
};

use super::orchestrator_helpers::{build_stage_snapshot, get_agent_for_stage};

// =============================================================================
// Orchestration Session
// =============================================================================

/// PipelineSession represents an active pipeline execution session.
///
/// The session tracks pipeline execution state only (config, stage visits,
/// last routing decision). The envelope is owned by the Kernel's
/// `process_envelopes` store.
#[derive(Debug, Clone)]
pub struct PipelineSession {
    pub process_id: ProcessId,
    pub pipeline_config: PipelineConfig,
    pub(crate) stage_visits: HashMap<String, i32>,
    #[allow(dead_code)] // Retained for diagnostics
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) last_activity_at: DateTime<Utc>,
    /// Last routing decision made by report_agent_result (consumed by get_next_instruction).
    pub(crate) last_routing_decision: Option<super::routing::RoutingDecision>,
}

// =============================================================================
// Orchestrator
// =============================================================================

/// Orchestrator manages kernel-side pipeline execution.
#[derive(Debug)]
pub struct Orchestrator {
    pub(crate) pipelines: HashMap<ProcessId, PipelineSession>,
    pub(crate) routing_registry: RoutingRegistry,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
            routing_registry: RoutingRegistry::new(),
        }
    }

    pub fn register_routing_fn(&mut self, name: impl Into<String>, f: std::sync::Arc<dyn RoutingFn>) {
        self.routing_registry.register(name, f);
    }

    /// Decide what to run next for `process_id`. Returns one of `RunAgent`,
    /// `Terminate`, or `WaitInterrupt`. The envelope may be mutated for
    /// bounds-driven termination.
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
        session.last_activity_at = Utc::now();

        if envelope.bounds.is_terminated() {
            return Ok(Instruction::terminate(
                envelope.bounds.terminal_reason().unwrap_or(TerminalReason::Completed),
                "Session already terminated",
            ));
        }

        // Pending tool-confirmation interrupt suspends the stage.
        if envelope.interrupts.is_pending() {
            let expired = envelope.interrupts.interrupt.as_ref()
                .and_then(|i| i.expires_at)
                .map(|exp| Utc::now() > exp)
                .unwrap_or(false);
            if expired {
                envelope.clear_interrupt();
                // Fall through to dispatch the agent again now that the interrupt is gone.
            } else {
                return Ok(Instruction::WaitInterrupt {
                    interrupt: envelope.interrupts.interrupt.clone(),
                });
            }
        }

        if let Some(reason) = envelope.check_bounds() {
            tracing::warn!(reason = ?reason, "bounds_terminated");
            envelope.bounds.terminate(reason, None);
            return Ok(Instruction::terminate(reason, format!("Bounds exceeded: {:?}", reason)));
        }

        let current_stage = &envelope.pipeline.current_stage;
        if current_stage.is_empty() {
            return Err(Error::state_transition("No current stage set"));
        }

        let agent_name = get_agent_for_stage(&session.pipeline_config, current_stage)?;
        Ok(Instruction::run_agent(agent_name))
    }

    /// Process agent execution result and advance the pipeline.
    ///
    /// Routing evaluation order:
    /// 1. If `agent_failed` AND `error_next` set → route to `error_next`
    /// 2. If `routing_fn` registered → call it
    /// 3. If no routing_fn → use `default_next`
    /// 4. If no `default_next` → terminate (Completed)
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

        // Bookkeeping
        envelope.bounds.llm_call_count += metrics.llm_calls;
        envelope.bounds.tool_call_count += metrics.tool_calls;
        if let Some(tokens_in) = metrics.tokens_in {
            envelope.bounds.tokens_in += tokens_in;
        }
        if let Some(tokens_out) = metrics.tokens_out {
            envelope.bounds.tokens_out += tokens_out;
        }
        envelope.pipeline.iteration += 1;

        if let Some(reason) = envelope.check_bounds() {
            envelope.bounds.terminate(reason, None);
            return Ok(());
        }

        if break_loop {
            envelope.bounds.terminate(TerminalReason::BreakRequested, None);
            session.last_activity_at = Utc::now();
            return Ok(());
        }

        let current_stage = envelope.pipeline.current_stage.clone();
        let pipeline_stage = session.pipeline_config.stages
            .iter()
            .find(|s| s.name == current_stage)
            .ok_or_else(|| Error::state_transition(format!(
                "Current stage '{}' not found in pipeline config",
                current_stage
            )))?
            .clone();

        *session.stage_visits.entry(current_stage.clone()).or_insert(0) += 1;
        envelope.complete_stage(&current_stage);

        if let Some(agent_output) = envelope.outputs.get(agent_name) {
            envelope.pipeline.completed_stage_snapshots.push(
                build_stage_snapshot(&current_stage, agent_output),
            );
        }

        let agent_lookup = pipeline_stage.agent.clone();
        let interrupt_response = envelope.interrupts.interrupt.as_ref()
            .and_then(|i| i.response.as_ref())
            .and_then(|r| serde_json::to_value(r).ok());

        let ctx = RoutingContext {
            current_stage: &current_stage,
            agent_name: &agent_lookup,
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

        if let Some(session) = self.pipelines.get_mut(process_id) {
            session.last_routing_decision = Some(routing_decision);
        }

        self.apply_routing_result(process_id, &current_stage, next_target, envelope)
    }

    /// Advance to the next stage or terminate.
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

                envelope.bounds.agent_hop_count += 1;
                tracing::info!(from = %from_stage, to = %target, "stage_transition");

                if let Some(pos) = envelope.pipeline.stage_order.iter().position(|s| s == &target) {
                    envelope.pipeline.current_stage_number = (pos + 1) as i32;
                }
                envelope.pipeline.current_stage = target;
                session.last_activity_at = Utc::now();
            }
            None => {
                tracing::info!(reason = ?TerminalReason::Completed, "pipeline_completed");
                envelope.bounds.terminate(TerminalReason::Completed, None);
                session.last_activity_at = Utc::now();
            }
        }

        Ok(())
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
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

    fn linear_stage(name: &str, default_next: Option<&str>) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            default_next: default_next.map(String::from),
            ..PipelineStage::default()
        }
    }

    #[test]
    fn run_agent_dispatch() {
        let config = PipelineConfig::test_default("p", vec![linear_stage("s1", None)]);
        let pid = ProcessId::must("p1");
        let mut envelope = make_envelope(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(pid.clone(), config, &mut envelope, false).unwrap();

        let instr = orch.get_next_instruction(&pid, &mut envelope).unwrap();
        match instr {
            Instruction::RunAgent { agent, .. } => assert_eq!(agent, "s1"),
            other => panic!("expected RunAgent, got {:?}", other),
        }
    }

    #[test]
    fn already_terminated_returns_terminate() {
        let config = PipelineConfig::test_default("p", vec![linear_stage("s1", None)]);
        let pid = ProcessId::must("p1");
        let mut envelope = make_envelope(&config);
        envelope.bounds.terminate(TerminalReason::Completed, None);

        let mut orch = Orchestrator::new();
        orch.initialize_session(pid.clone(), config, &mut envelope, false).unwrap();
        let instr = orch.get_next_instruction(&pid, &mut envelope).unwrap();
        assert!(matches!(instr, Instruction::Terminate { .. }));
    }

    #[test]
    fn pending_interrupt_returns_wait() {
        use crate::envelope::FlowInterrupt;
        let config = PipelineConfig::test_default("p", vec![linear_stage("s1", None)]);
        let pid = ProcessId::must("p1");
        let mut envelope = make_envelope(&config);
        envelope.set_interrupt(FlowInterrupt::new());

        let mut orch = Orchestrator::new();
        orch.initialize_session(pid.clone(), config, &mut envelope, false).unwrap();
        let instr = orch.get_next_instruction(&pid, &mut envelope).unwrap();
        assert!(matches!(instr, Instruction::WaitInterrupt { .. }));
    }

    #[test]
    fn linear_chain_advances() {
        let config = PipelineConfig::test_default("p", vec![
            linear_stage("s1", Some("s2")),
            linear_stage("s2", None),
        ]);
        let pid = ProcessId::must("p1");
        let mut envelope = make_envelope(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(pid.clone(), config, &mut envelope, false).unwrap();

        // first dispatch is s1
        let instr = orch.get_next_instruction(&pid, &mut envelope).unwrap();
        assert!(matches!(&instr, Instruction::RunAgent { agent, .. } if agent == "s1"));
        // report success → routes to s2 via default_next
        orch.report_agent_result(&pid, "s1", zero_metrics(), &mut envelope, false, false).unwrap();
        let instr = orch.get_next_instruction(&pid, &mut envelope).unwrap();
        assert!(matches!(&instr, Instruction::RunAgent { agent, .. } if agent == "s2"));
        // s2 has no default_next → next report terminates
        orch.report_agent_result(&pid, "s2", zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.is_terminated());
    }

    #[test]
    fn routing_fn_overrides_default_next() {
        let config = PipelineConfig::test_default("p", vec![
            PipelineStage {
                name: "s1".into(),
                agent: "s1".into(),
                routing_fn: Some("decide".into()),
                default_next: Some("fallback".into()),
                ..PipelineStage::default()
            },
            linear_stage("target", None),
            linear_stage("fallback", None),
        ]);
        let pid = ProcessId::must("p1");
        let mut envelope = make_envelope(&config);
        let mut orch = Orchestrator::new();
        orch.register_routing_fn("decide", Arc::new(|_ctx: &RoutingContext<'_>| {
            RoutingResult::Next("target".into())
        }));
        orch.initialize_session(pid.clone(), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&pid, "s1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "target");
    }

    #[test]
    fn error_next_routes_on_failure() {
        let config = PipelineConfig::test_default("p", vec![
            PipelineStage {
                name: "s1".into(),
                agent: "s1".into(),
                default_next: Some("s_ok".into()),
                error_next: Some("s_err".into()),
                ..PipelineStage::default()
            },
            linear_stage("s_ok", None),
            linear_stage("s_err", None),
        ]);
        let pid = ProcessId::must("p1");
        let mut envelope = make_envelope(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(pid.clone(), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&pid, "s1", zero_metrics(), &mut envelope, true, false).unwrap();
        assert_eq!(envelope.pipeline.current_stage, "s_err");
    }

    #[test]
    fn max_visits_terminates_self_loop() {
        let config = PipelineConfig::test_default("p", vec![PipelineStage {
            name: "loop".into(),
            agent: "loop".into(),
            default_next: Some("loop".into()),
            max_visits: Some(2),
            ..PipelineStage::default()
        }]);
        let pid = ProcessId::must("p1");
        let mut envelope = make_envelope(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(pid.clone(), config, &mut envelope, false).unwrap();

        // First report: visit count was 0, becomes 1, transitions back to loop (visits=1, allowed since limit=2)
        orch.report_agent_result(&pid, "loop", zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(!envelope.bounds.is_terminated(), "first transition allowed");
        // Second report: visit count becomes 2 (== limit), transition rejected
        orch.report_agent_result(&pid, "loop", zero_metrics(), &mut envelope, false, false).unwrap();
        assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::MaxStageVisitsExceeded));
    }

    #[test]
    fn break_loop_terminates() {
        let config = PipelineConfig::test_default("p", vec![linear_stage("s1", Some("s2")), linear_stage("s2", None)]);
        let pid = ProcessId::must("p1");
        let mut envelope = make_envelope(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(pid.clone(), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&pid, "s1", zero_metrics(), &mut envelope, false, true).unwrap();
        assert_eq!(envelope.bounds.terminal_reason(), Some(TerminalReason::BreakRequested));
    }

    #[test]
    fn bounds_terminate_when_exceeded() {
        let mut config = PipelineConfig::test_default("p", vec![PipelineStage {
            name: "s1".into(),
            agent: "s1".into(),
            default_next: Some("s1".into()),
            max_visits: Some(100),
            ..PipelineStage::default()
        }]);
        config.max_iterations = 1;
        let pid = ProcessId::must("p1");
        let mut envelope = make_envelope(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(pid.clone(), config, &mut envelope, false).unwrap();

        orch.report_agent_result(&pid, "s1", zero_metrics(), &mut envelope, false, false).unwrap();
        assert!(envelope.bounds.is_terminated());
    }

    #[test]
    fn instruction_serde_roundtrip() {
        let i = Instruction::run_agent("a");
        let s = serde_json::to_string(&i).unwrap();
        let back: Instruction = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, Instruction::RunAgent { .. }));

        let t = Instruction::terminate(TerminalReason::Completed, "done");
        let s = serde_json::to_string(&t).unwrap();
        let back: Instruction = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, Instruction::Terminate { .. }));
    }
}
