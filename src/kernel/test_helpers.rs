//! Shared test helpers for kernel orchestration tests.
//!
//! Eliminates copy-paste of `stage()`, `create_test_pipeline()`,
//! and `create_test_envelope()` across orchestrator_*.rs test modules.

use crate::envelope::Envelope;

use super::orchestrator_types::*;

/// Build a PipelineStage with optional routing_fn and default_next.
/// All other fields use defaults.
pub fn stage(
    name: &str,
    agent: &str,
    routing_fn: Option<&str>,
    default_next: Option<&str>,
) -> PipelineStage {
    PipelineStage {
        name: name.to_string(),
        agent: agent.to_string(),
        routing_fn: routing_fn.map(|s| s.to_string()),
        default_next: default_next.map(|s| s.to_string()),
        ..Default::default()
    }
}

/// Linear 2-stage pipeline: stage1 → stage2 → terminate.
/// stage1 has default_next="stage2", stage2 terminates.
pub fn create_test_pipeline() -> PipelineConfig {
    PipelineConfig::test_default(
        "test_pipeline",
        vec![
            stage("stage1", "agent1", None, Some("stage2")),
            stage("stage2", "agent2", None, None),
        ],
    )
}

/// Empty envelope with cleared current_stage (ready for pipeline init).
pub fn create_test_envelope() -> Envelope {
    let mut env = Envelope::new();
    env.pipeline.current_stage = String::new();
    env
}

/// Build an envelope with the pipeline's bounds and stage_order populated.
pub fn make_envelope(config: &PipelineConfig) -> Envelope {
    let mut env = create_test_envelope();
    env.pipeline.max_iterations = config.max_iterations;
    env.bounds.max_llm_calls = config.max_llm_calls;
    env.bounds.max_agent_hops = config.max_agent_hops;
    env.pipeline.stage_order = config.get_stage_order();
    if !env.pipeline.stage_order.is_empty() {
        env.pipeline.current_stage = env.pipeline.stage_order[0].clone();
    }
    env
}
