//! Workflow construction validation. External workflow schemas were removed;
//! workflows are now Rust values.

#![allow(clippy::expect_used)]

use jeeves_core::prelude::*;
use serde_json::json;

#[test]
fn duplicate_stages_are_rejected_at_build_time() {
    let result = Workflow::builder("duplicate")
        .stage(Stage::deterministic_fn("same", |_| Ok(json!(1))))
        .stage(Stage::deterministic_fn("same", |_| Ok(json!(2))))
        .build();
    assert!(result.is_err());
}

#[test]
fn missing_static_route_is_rejected_at_build_time() {
    let result = Workflow::builder("missing")
        .stage(Stage::route_only("entry").next("absent"))
        .build();
    assert!(result.is_err());
}

#[test]
fn llm_workflow_requires_provider_when_engine_is_built() {
    let workflow = Workflow::builder("llm")
        .stage(Stage::llm(
            "answer",
            LlmAction::text(Prompt::text("answer")),
        ))
        .build()
        .expect("valid workflow");
    let result = Engine::builder()
        .workflow(workflow)
        .expect("workflow registration")
        .build();
    assert!(result.is_err());
}
