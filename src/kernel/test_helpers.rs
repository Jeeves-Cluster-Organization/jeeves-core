//! Shared test helpers for kernel orchestration tests.
//!
//! Eliminates copy-paste of `stage()`, `create_test_workflow()`, and
//! `create_test_run()` across orchestrator_*.rs test modules.

use crate::run::Run;
use crate::workflow::{Workflow, Stage};

/// Build a Stage with optional routing_fn and default_next.
/// All other fields use defaults.
pub fn stage(
    name: &str,
    agent: &str,
    routing_fn: Option<&str>,
    default_next: Option<&str>,
) -> Stage {
    Stage {
        name: name.into(),
        agent: agent.into(),
        routing_fn: routing_fn.map(Into::into),
        default_next: default_next.map(Into::into),
        ..Default::default()
    }
}

/// Linear 2-stage workflow: stage1 → stage2 → terminate.
/// stage1 has default_next="stage2", stage2 terminates.
pub fn create_test_workflow() -> Workflow {
    Workflow::test_default(
        "test_workflow",
        vec![
            stage("stage1", "agent1", None, Some("stage2")),
            stage("stage2", "agent2", None, None),
        ],
    )
}

/// Empty run with cleared current_stage (ready for workflow init).
pub fn create_test_run() -> Run {
    let mut run = Run::anonymous();
    run.current_stage = crate::types::StageName::default();
    run
}

/// Build a run with the workflow's bounds and stage_order populated.
pub fn make_run(config: &Workflow) -> Run {
    let mut run = create_test_run();
    run.max_iterations = config.max_iterations;
    run.limits.max_llm_calls = config.max_llm_calls;
    run.limits.max_agent_hops = config.max_agent_hops;
    run.stage_order = config.get_stage_order();
    if !run.stage_order.is_empty() {
        run.current_stage = run.stage_order[0].clone();
    }
    run
}
