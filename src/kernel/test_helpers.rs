//! Shared test helpers for kernel orchestration tests.
//!
//! Eliminates copy-paste of `stage()`, `always_to()`, `create_test_pipeline()`,
//! and `create_test_envelope()` across orchestrator_*.rs test modules.

use crate::envelope::Envelope;

use super::orchestrator_types::*;
use super::routing::*;

/// Build a PipelineStage with routing rules and optional default_next.
/// All other fields use defaults.
pub fn stage(
    name: &str,
    agent: &str,
    routing: Vec<RoutingRule>,
    default_next: Option<&str>,
) -> PipelineStage {
    PipelineStage {
        name: name.to_string(),
        agent: agent.to_string(),
        routing,
        default_next: default_next.map(|s| s.to_string()),
        ..Default::default()
    }
}

/// Routing rule that always matches → target.
pub fn always_to(target: &str) -> RoutingRule {
    RoutingRule {
        expr: RoutingExpr::Always,
        target: target.to_string(),
    }
}

/// Linear 2-stage pipeline: stage1 --(Always)--> stage2 --> terminate.
pub fn create_test_pipeline() -> PipelineConfig {
    PipelineConfig::test_default(
        "test_pipeline",
        vec![
            stage("stage1", "agent1", vec![always_to("stage2")], None),
            stage("stage2", "agent2", vec![], None),
        ],
    )
}

/// Empty envelope with cleared current_stage (ready for pipeline init).
pub fn create_test_envelope() -> Envelope {
    let mut env = Envelope::new();
    env.pipeline.current_stage = String::new();
    env
}
