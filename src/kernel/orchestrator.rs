//! Workflow orchestration — kernel-driven workflow execution control.
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

use crate::run::{Run, TerminalReason};
use crate::types::{Error, RunId, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tracing::instrument;

pub use super::protocol::{Instruction, RunSnapshot};
pub use crate::agent::metrics::AgentExecutionMetrics;
pub use super::routing::{
    evaluate_routing_with_reason, RoutingContext, RoutingDecision, RoutingFn, RoutingReason,
    RoutingRegistry, RoutingResult,
};
pub use crate::workflow::{Workflow, Stage};

/// Look up the agent name for a stage in a workflow.
///
/// Module-level (not a method on `Orchestrator`) so the orchestrator can pass
/// its own fields to it while holding a mutable borrow on `sessions`.
pub(crate) fn get_agent_for_stage(
    workflow: &Workflow,
    stage_name: &str,
) -> Result<crate::types::AgentName> {
    workflow
        .stages
        .iter()
        .find(|s| s.name.as_str() == stage_name)
        .map(|s| s.agent.clone())
        .ok_or_else(|| Error::not_found(format!("Stage not found in workflow: {}", stage_name)))
}

/// Orchestration represents an active workflow execution session.
///
/// The session tracks workflow execution state only (workflow definition,
/// stage visits, last routing decision). The run is owned by the Kernel's
/// `runs` store.
#[derive(Debug, Clone)]
pub struct Orchestration {
    pub run_id: RunId,
    pub workflow: Workflow,
    pub(crate) stage_visits: HashMap<crate::types::StageName, i32>,
    #[allow(dead_code)] // Retained for diagnostics
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) last_activity_at: DateTime<Utc>,
    /// Last routing decision made by report_agent_result (consumed by get_next_instruction).
    pub(crate) last_routing_decision: Option<super::routing::RoutingDecision>,
}

/// Orchestrator manages kernel-side workflow execution.
#[derive(Debug)]
pub struct Orchestrator {
    pub(crate) sessions: HashMap<RunId, Orchestration>,
    pub(crate) routing_registry: RoutingRegistry,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            routing_registry: RoutingRegistry::new(),
        }
    }

    pub fn register_routing_fn(&mut self, name: impl Into<crate::types::RoutingFnName>, f: std::sync::Arc<dyn RoutingFn>) {
        self.routing_registry.register(name, f);
    }

    /// Decide what to run next for `run_id`. Returns one of `RunAgent`,
    /// `Terminate`, or `WaitInterrupt`. The run may be mutated for
    /// bounds-driven termination.
    #[instrument(skip(self, run), fields(run_id = %run_id))]
    pub fn get_next_instruction(
        &mut self,
        run_id: &RunId,
        run: &mut Run,
    ) -> Result<Instruction> {
        let session = self
            .sessions
            .get_mut(run_id)
            .ok_or_else(|| Error::not_found(format!("Unknown process: {}", run_id)))?;
        session.last_activity_at = Utc::now();

        if run.is_terminated() {
            return Ok(Instruction::terminate(
                run.terminal_reason().unwrap_or(TerminalReason::Completed),
                "Session already terminated",
            ));
        }

        // Pending tool-confirmation interrupt suspends the stage.
        if run.interrupts.is_pending() {
            let expired = run.interrupts.interrupt.as_ref()
                .and_then(|i| i.expires_at)
                .map(|exp| Utc::now() > exp)
                .unwrap_or(false);
            if expired {
                run.clear_interrupt();
                // Fall through to dispatch the agent again now that the interrupt is gone.
            } else {
                return Ok(Instruction::WaitInterrupt {
                    interrupt: run.interrupts.interrupt.clone(),
                });
            }
        }

        if let Some(reason) = run.check_bounds() {
            tracing::warn!(reason = ?reason, "bounds_terminated");
            run.terminate_with(reason, None);
            return Ok(Instruction::terminate(reason, format!("Bounds exceeded: {:?}", reason)));
        }

        let current_stage = &run.current_stage;
        if current_stage.is_empty() {
            return Err(Error::state_transition("No current stage set"));
        }

        let agent_name = get_agent_for_stage(&session.workflow, current_stage.as_str())?;
        Ok(Instruction::run_agent(agent_name.as_str()))
    }

    /// Process agent execution result and advance the workflow.
    ///
    /// Routing evaluation order:
    /// 1. If `agent_failed` AND `error_next` set → route to `error_next`
    /// 2. If `routing_fn` registered → call it
    /// 3. If no routing_fn → use `default_next`
    /// 4. If no `default_next` → terminate (Completed)
    #[instrument(skip(self, metrics, run), fields(run_id = %run_id, agent_failed))]
    pub fn report_agent_result(
        &mut self,
        run_id: &RunId,
        agent_name: &str,
        metrics: AgentExecutionMetrics,
        run: &mut Run,
        agent_failed: bool,
        break_loop: bool,
    ) -> Result<()> {
        let session = self
            .sessions
            .get_mut(run_id)
            .ok_or_else(|| Error::not_found(format!("Unknown process: {}", run_id)))?;

        // Bookkeeping
        run.metrics.llm_calls += metrics.llm_calls;
        run.metrics.tool_calls += metrics.tool_calls;
        if let Some(tokens_in) = metrics.tokens_in {
            run.metrics.tokens_in += tokens_in;
        }
        if let Some(tokens_out) = metrics.tokens_out {
            run.metrics.tokens_out += tokens_out;
        }
        run.iteration += 1;

        if let Some(reason) = run.check_bounds() {
            run.terminate_with(reason, None);
            return Ok(());
        }

        if break_loop {
            run.terminate_with(TerminalReason::BreakRequested, None);
            session.last_activity_at = Utc::now();
            return Ok(());
        }

        let current_stage = run.current_stage.clone();
        let pipeline_stage = session.workflow.stages
            .iter()
            .find(|s| s.name.as_str() == current_stage.as_str())
            .ok_or_else(|| Error::state_transition(format!(
                "Current stage '{}' not found in workflow",
                current_stage
            )))?
            .clone();

        *session.stage_visits.entry(current_stage.clone()).or_insert(0) += 1;

        let agent_lookup = pipeline_stage.agent.clone();
        let interrupt_response = run.interrupts.interrupt.as_ref()
            .and_then(|i| i.response.as_ref())
            .and_then(|r| serde_json::to_value(r).ok());

        let ctx = RoutingContext {
            current_stage: current_stage.as_str(),
            agent_name: agent_lookup.as_str(),
            agent_failed,
            outputs: &run.outputs,
            metadata: &run.audit.metadata,
            interrupt_response: interrupt_response.as_ref(),
            state: &run.state,
        };
        let routing_decision = evaluate_routing_with_reason(
            &pipeline_stage,
            &self.routing_registry,
            &ctx,
            current_stage.as_str(),
        );
        let next_target = routing_decision.target.clone();

        if let Some(session) = self.sessions.get_mut(run_id) {
            session.last_routing_decision = Some(routing_decision);
        }

        self.apply_routing_result(run_id, current_stage.as_str(), next_target, run)
    }

    /// Advance to the next stage or terminate.
    fn apply_routing_result(
        &mut self,
        run_id: &RunId,
        from_stage: &str,
        next_target: Option<crate::types::StageName>,
        run: &mut Run,
    ) -> Result<()> {
        let session = self
            .sessions
            .get_mut(run_id)
            .ok_or_else(|| Error::not_found(format!("Unknown process: {}", run_id)))?;

        match next_target {
            Some(target) => {
                if let Some(target_stage) = session.workflow.stages.iter().find(|s| s.name == target) {
                    if let Some(max_visits) = target_stage.max_visits {
                        let visits = session.stage_visits.get(target.as_str()).copied().unwrap_or(0);
                        if visits >= max_visits {
                            run.terminate_with(
                                TerminalReason::MaxStageVisitsExceeded,
                                Some(format!("Stage '{}' exceeded max_visits limit of {}", target, max_visits)),
                            );
                            session.last_activity_at = Utc::now();
                            return Ok(());
                        }
                    }
                }

                run.metrics.agent_hops += 1;
                tracing::info!(from = %from_stage, to = %target, "stage_transition");

                run.current_stage = target;
                session.last_activity_at = Utc::now();
            }
            None => {
                tracing::info!(reason = ?TerminalReason::Completed, "run_completed");
                run.terminate_with(TerminalReason::Completed, None);
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
    use crate::types::RunId;
    use std::sync::Arc;

    fn zero_metrics() -> AgentExecutionMetrics {
        AgentExecutionMetrics::default()
    }

    fn linear_stage(name: &str, default_next: Option<&str>) -> Stage {
        Stage {
            name: name.into(),
            agent: name.into(),
            default_next: default_next.map(Into::into),
            ..Stage::default()
        }
    }

    #[test]
    fn run_agent_dispatch() {
        let config = Workflow::test_default("p", vec![linear_stage("s1", None)]);
        let run_id = RunId::must("p1");
        let mut run = make_run(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(run_id.clone(), config, &mut run, false).unwrap();

        let instr = orch.get_next_instruction(&run_id, &mut run).unwrap();
        match instr {
            Instruction::RunAgent { agent, .. } => assert_eq!(agent, "s1"),
            other => panic!("expected RunAgent, got {:?}", other),
        }
    }

    #[test]
    fn already_terminated_returns_terminate() {
        let config = Workflow::test_default("p", vec![linear_stage("s1", None)]);
        let run_id = RunId::must("p1");
        let mut run = make_run(&config);
        run.terminate_with(TerminalReason::Completed, None);

        let mut orch = Orchestrator::new();
        orch.initialize_session(run_id.clone(), config, &mut run, false).unwrap();
        let instr = orch.get_next_instruction(&run_id, &mut run).unwrap();
        assert!(matches!(instr, Instruction::Terminate { .. }));
    }

    #[test]
    fn pending_interrupt_returns_wait() {
        use crate::run::FlowInterrupt;
        let config = Workflow::test_default("p", vec![linear_stage("s1", None)]);
        let run_id = RunId::must("p1");
        let mut run = make_run(&config);
        run.set_interrupt(FlowInterrupt::new());

        let mut orch = Orchestrator::new();
        orch.initialize_session(run_id.clone(), config, &mut run, false).unwrap();
        let instr = orch.get_next_instruction(&run_id, &mut run).unwrap();
        assert!(matches!(instr, Instruction::WaitInterrupt { .. }));
    }

    #[test]
    fn linear_chain_advances() {
        let config = Workflow::test_default("p", vec![
            linear_stage("s1", Some("s2")),
            linear_stage("s2", None),
        ]);
        let run_id = RunId::must("p1");
        let mut run = make_run(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(run_id.clone(), config, &mut run, false).unwrap();

        // first dispatch is s1
        let instr = orch.get_next_instruction(&run_id, &mut run).unwrap();
        assert!(matches!(&instr, Instruction::RunAgent { agent, .. } if agent == "s1"));
        // report success → routes to s2 via default_next
        orch.report_agent_result(&run_id, "s1", zero_metrics(), &mut run, false, false).unwrap();
        let instr = orch.get_next_instruction(&run_id, &mut run).unwrap();
        assert!(matches!(&instr, Instruction::RunAgent { agent, .. } if agent == "s2"));
        // s2 has no default_next → next report terminates
        orch.report_agent_result(&run_id, "s2", zero_metrics(), &mut run, false, false).unwrap();
        assert!(run.is_terminated());
    }

    #[test]
    fn routing_fn_overrides_default_next() {
        let config = Workflow::test_default("p", vec![
            Stage {
                name: "s1".into(),
                agent: "s1".into(),
                routing_fn: Some("decide".into()),
                default_next: Some("fallback".into()),
                ..Stage::default()
            },
            linear_stage("target", None),
            linear_stage("fallback", None),
        ]);
        let run_id = RunId::must("p1");
        let mut run = make_run(&config);
        let mut orch = Orchestrator::new();
        orch.register_routing_fn("decide", Arc::new(|_ctx: &RoutingContext<'_>| {
            RoutingResult::Next("target".into())
        }));
        orch.initialize_session(run_id.clone(), config, &mut run, false).unwrap();

        orch.report_agent_result(&run_id, "s1", zero_metrics(), &mut run, false, false).unwrap();
        assert_eq!(run.current_stage.as_str(), "target");
    }

    #[test]
    fn error_next_routes_on_failure() {
        let config = Workflow::test_default("p", vec![
            Stage {
                name: "s1".into(),
                agent: "s1".into(),
                default_next: Some("s_ok".into()),
                error_next: Some("s_err".into()),
                ..Stage::default()
            },
            linear_stage("s_ok", None),
            linear_stage("s_err", None),
        ]);
        let run_id = RunId::must("p1");
        let mut run = make_run(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(run_id.clone(), config, &mut run, false).unwrap();

        orch.report_agent_result(&run_id, "s1", zero_metrics(), &mut run, true, false).unwrap();
        assert_eq!(run.current_stage.as_str(), "s_err");
    }

    #[test]
    fn max_visits_terminates_self_loop() {
        let config = Workflow::test_default("p", vec![Stage {
            name: "loop".into(),
            agent: "loop".into(),
            default_next: Some("loop".into()),
            max_visits: Some(2),
            ..Stage::default()
        }]);
        let run_id = RunId::must("p1");
        let mut run = make_run(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(run_id.clone(), config, &mut run, false).unwrap();

        // First report: visit count was 0, becomes 1, transitions back to loop (visits=1, allowed since limit=2)
        orch.report_agent_result(&run_id, "loop", zero_metrics(), &mut run, false, false).unwrap();
        assert!(!run.is_terminated(), "first transition allowed");
        // Second report: visit count becomes 2 (== limit), transition rejected
        orch.report_agent_result(&run_id, "loop", zero_metrics(), &mut run, false, false).unwrap();
        assert_eq!(run.terminal_reason(), Some(TerminalReason::MaxStageVisitsExceeded));
    }

    #[test]
    fn break_loop_terminates() {
        let config = Workflow::test_default("p", vec![linear_stage("s1", Some("s2")), linear_stage("s2", None)]);
        let run_id = RunId::must("p1");
        let mut run = make_run(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(run_id.clone(), config, &mut run, false).unwrap();

        orch.report_agent_result(&run_id, "s1", zero_metrics(), &mut run, false, true).unwrap();
        assert_eq!(run.terminal_reason(), Some(TerminalReason::BreakRequested));
    }

    #[test]
    fn bounds_terminate_when_exceeded() {
        let mut config = Workflow::test_default("p", vec![Stage {
            name: "s1".into(),
            agent: "s1".into(),
            default_next: Some("s1".into()),
            max_visits: Some(100),
            ..Stage::default()
        }]);
        config.max_iterations = 1;
        let run_id = RunId::must("p1");
        let mut run = make_run(&config);
        let mut orch = Orchestrator::new();
        orch.initialize_session(run_id.clone(), config, &mut run, false).unwrap();

        orch.report_agent_result(&run_id, "s1", zero_metrics(), &mut run, false, false).unwrap();
        assert!(run.is_terminated());
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
