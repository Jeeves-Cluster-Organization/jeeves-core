//! Shared test helpers for kernel orchestration tests.
//!
//! Eliminates copy-paste of `stage()`, `create_test_pipeline()`,
//! and `create_test_envelope()` across orchestrator_*.rs test modules.

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

/// Linear 2-stage pipeline: stage1 → stage2 → terminate.
/// stage1 has default_next="stage2", stage2 terminates.
pub fn create_test_pipeline() -> Workflow {
    Workflow::test_default(
        "test_pipeline",
        vec![
            stage("stage1", "agent1", None, Some("stage2")),
            stage("stage2", "agent2", None, None),
        ],
    )
}

/// Empty run with cleared current_stage (ready for pipeline init).
pub fn create_test_envelope() -> Run {
    let mut env = Run::anonymous();
    env.current_stage = crate::types::StageName::default();
    env
}

/// Build an run with the pipeline's bounds and stage_order populated.
pub fn make_envelope(config: &Workflow) -> Run {
    let mut env = create_test_envelope();
    env.max_iterations = config.max_iterations;
    env.limits.max_llm_calls = config.max_llm_calls;
    env.limits.max_agent_hops = config.max_agent_hops;
    env.stage_order = config.get_stage_order();
    if !env.stage_order.is_empty() {
        env.current_stage = env.stage_order[0].clone();
    }
    env
}
