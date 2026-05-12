//! Schema drift regression test.
//!
//! Generates the JSON Schema for `Workflow` from the current Rust types
//! and compares it to the on-disk `schema/pipeline.schema.json`. Drift between
//! the two means the schema file is stale.
//!
//! To regenerate the schema after intentional type changes:
//! ```bash
//! JEEVES_UPDATE_SCHEMA=1 cargo test --test schema -- schema_matches_on_disk
//! ```

use std::path::PathBuf;

use jeeves_core::workflow::{pipeline_config_json_schema, Workflow};

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schema/pipeline.schema.json")
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("pretty-print")
}

#[test]
fn schema_matches_on_disk() {
    let generated = pipeline_config_json_schema();

    if std::env::var_os("JEEVES_UPDATE_SCHEMA").is_some() {
        std::fs::write(schema_path(), pretty(&generated))
            .expect("write schema/pipeline.schema.json");
        eprintln!("Updated {}", schema_path().display());
        return;
    }

    let on_disk: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(schema_path()).expect("read schema/pipeline.schema.json"),
    )
    .expect("parse on-disk schema");

    if on_disk != generated {
        let diff = format!(
            "--- on-disk\n+++ generated\n{}\n\n--- vs ---\n\n{}",
            pretty(&on_disk),
            pretty(&generated),
        );
        panic!(
            "schema/pipeline.schema.json is out of date. \
             Regenerate with: JEEVES_UPDATE_SCHEMA=1 cargo test --test schema -- schema_matches_on_disk\n\n{diff}"
        );
    }
}

/// Representative pipeline JSON shapes that game-mvp uses. Drift between this
/// fixture and `Workflow` deserialization surfaces wire-format breakage
/// before it reaches the consumer.
#[test]
fn representative_pipeline_json_deserializes() {
    // Shape 1: simple linear stages with response_format and timeout_seconds
    // (mirrors game-mvp's newspaper_publish.json).
    let json = serde_json::json!({
        "name": "newspaper_publish",
        "max_iterations": 5,
        "max_llm_calls": 10,
        "max_agent_hops": 5,
        "stages": [
            {
                "name": "analyze",
                "agent": "analyze",
                "has_llm": true,
                "output_key": "analyze",
                "prompt_key": "newspaper.analyze",
                "max_context_tokens": 2000,
                "context_overflow": "Fail",
                "timeout_seconds": 300,
                "response_format": {
                    "type": "object",
                    "properties": { "claim": {"type": "string"} },
                    "required": ["claim"]
                },
                "default_next": "score",
                "error_next": "score"
            },
            {
                "name": "score",
                "agent": "score",
                "has_llm": false,
                "output_key": "score",
                "timeout_seconds": 30
            }
        ]
    });
    let config: Workflow =
        serde_json::from_value(json).expect("deserialize newspaper-shape pipeline");
    assert_eq!(config.name, "newspaper_publish");
    assert_eq!(config.stages.len(), 2);
    assert!(config.stages[0].agent_config.has_llm);
    assert_eq!(
        config.stages[0].agent_config.prompt_key.as_deref(),
        Some("newspaper.analyze")
    );
    assert!(config.stages[0].response_format.is_some());

    // Shape 2: stage with routing_fn + retry_policy + max_visits (mirrors a
    // dialogue-style branching pipeline).
    let json = serde_json::json!({
        "name": "npc_dialogue",
        "max_iterations": 20,
        "max_llm_calls": 50,
        "max_agent_hops": 20,
        "state_schema": [
            {"key": "tool_outputs", "merge": "Append"}
        ],
        "stages": [
            {
                "name": "router",
                "agent": "router",
                "has_llm": false,
                "routing_fn": "think_router",
                "default_next": "think_general",
                "max_visits": 3,
                "retry_policy": {
                    "max_retries": 2,
                    "initial_backoff_ms": 500,
                    "max_backoff_ms": 5000,
                    "backoff_multiplier": 2.0
                }
            },
            { "name": "think_general", "agent": "think_general", "has_llm": true }
        ]
    });
    let config: Workflow =
        serde_json::from_value(json).expect("deserialize dialogue-shape pipeline");
    assert_eq!(config.name, "npc_dialogue");
    assert_eq!(config.state_schema.len(), 1);
    assert_eq!(config.stages[0].routing_fn.as_deref(), Some("think_router"));
    assert_eq!(config.stages[0].max_visits, Some(3));
    assert!(config.stages[0].retry_policy.is_some());

    // Shape 3: empty stage with all defaults.
    let json = serde_json::json!({
        "name": "minimal",
        "max_iterations": 1,
        "max_llm_calls": 1,
        "max_agent_hops": 1,
        "stages": [{ "name": "only", "agent": "only" }]
    });
    let config: Workflow = serde_json::from_value(json).expect("deserialize minimal pipeline");
    assert_eq!(config.stages[0].name, "only");
    assert!(!config.stages[0].agent_config.has_llm);
}
