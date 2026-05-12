//! Workflow: the static blueprint a kernel executes.
//!
//! A `PipelineConfig` (renamed `Workflow` in step 5) is a sequence of
//! `PipelineStage`s with global bounds and an optional `state_schema` for
//! cross-iteration accumulation. Deterministic pipelines, branching
//! pipelines, and self-routing agent harnesses all share this shape — the
//! difference is purely in how stages route to each other.

pub mod policy;
pub mod stage;
pub mod state_schema;

pub use policy::{ContextOverflow, RetryPolicy};
pub use stage::{AgentConfig, PipelineStage};
pub use state_schema::{MergeStrategy, StateField};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::types::{Error, Result};

/// Pipeline shape. Linear/branching/cyclic flows come from per-stage
/// `routing_fn` + `default_next`; no graph topology in the kernel.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PipelineConfig {
    /// Used in `PipelineEvent.pipeline` for event attribution.
    pub name: String,
    /// First stage is the entry point.
    pub stages: Vec<PipelineStage>,
    pub max_iterations: i32,
    pub max_llm_calls: i32,
    pub max_agent_hops: i32,
    /// Merge strategies for state accumulation across loop-backs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_schema: Vec<StateField>,
}

impl PipelineConfig {
    pub fn get_stage_order(&self) -> Vec<String> {
        self.stages.iter().map(|s| s.name.clone()).collect()
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(Error::validation("Pipeline name is required"));
        }
        if self.stages.is_empty() {
            return Err(Error::validation("Pipeline must have at least one stage"));
        }

        if self.max_iterations <= 0 {
            return Err(Error::validation(format!(
                "max_iterations must be > 0, got {}",
                self.max_iterations
            )));
        }
        if self.max_llm_calls <= 0 {
            return Err(Error::validation(format!(
                "max_llm_calls must be > 0, got {}",
                self.max_llm_calls
            )));
        }
        if self.max_agent_hops <= 0 {
            return Err(Error::validation(format!(
                "max_agent_hops must be > 0, got {}",
                self.max_agent_hops
            )));
        }

        let mut stage_names: HashSet<&str> = HashSet::new();
        let mut output_keys: HashSet<&str> = HashSet::new();
        for stage in &self.stages {
            if stage.name.is_empty() {
                return Err(Error::validation("Stage name must not be empty"));
            }
            if !stage_names.insert(stage.name.as_str()) {
                return Err(Error::validation(format!(
                    "Duplicate stage name '{}'",
                    stage.name
                )));
            }
            if let Some(ref ok) = stage.output_key {
                if !output_keys.insert(ok.as_str()) {
                    return Err(Error::validation(format!(
                        "Duplicate output_key '{}' on stage '{}'",
                        ok, stage.name
                    )));
                }
            }
        }

        for stage in &self.stages {
            if stage.agent.is_empty() {
                return Err(Error::validation(format!(
                    "Stage '{}' must have a non-empty agent field",
                    stage.name
                )));
            }

            // Reject `default_next` self-loops without `max_visits` — that's an
            // infinite loop hiding behind static config.
            if let Some(ref dn) = stage.default_next {
                if dn == &stage.name && stage.max_visits.is_none() {
                    return Err(Error::validation(format!(
                        "Stage '{}' has default_next pointing to itself without max_visits (infinite loop)",
                        stage.name
                    )));
                }
            }

            if let Some(ref dn) = stage.default_next {
                if !stage_names.contains(dn.as_str()) {
                    return Err(Error::validation(format!(
                        "Stage '{}' has default_next '{}' which does not exist in pipeline",
                        stage.name, dn
                    )));
                }
            }

            if let Some(ref en) = stage.error_next {
                if !stage_names.contains(en.as_str()) {
                    return Err(Error::validation(format!(
                        "Stage '{}' has error_next '{}' which does not exist in pipeline",
                        stage.name, en
                    )));
                }
            }

            if let Some(mv) = stage.max_visits {
                if mv <= 0 {
                    return Err(Error::validation(format!(
                        "Stage '{}' has max_visits {} which must be positive",
                        stage.name, mv
                    )));
                }
            }
            if let Some(mct) = stage.max_context_tokens {
                if mct <= 0 {
                    return Err(Error::validation(format!(
                        "Stage '{}' has max_context_tokens {} which must be positive",
                        stage.name, mct
                    )));
                }
            }
        }

        let mut state_keys: HashSet<&str> = HashSet::new();
        for field in &self.state_schema {
            if !state_keys.insert(field.key.as_str()) {
                return Err(Error::validation(format!(
                    "Duplicate state_schema key '{}'",
                    field.key
                )));
            }
        }

        Ok(())
    }

    /// Test-only minimal config constructor. Avoids field boilerplate.
    #[cfg(test)]
    pub fn test_default(name: &str, stages: Vec<PipelineStage>) -> Self {
        Self {
            name: name.to_string(),
            stages,
            max_iterations: 10,
            max_llm_calls: 50,
            max_agent_hops: 10,
            state_schema: vec![],
        }
    }
}

/// Generate the JSON Schema for `PipelineConfig` from the current Rust types.
/// Used by `tests/schema.rs` to detect drift between the on-disk schema and
/// the live types.
pub fn pipeline_config_json_schema() -> serde_json::Value {
    #[allow(clippy::expect_used)]
    serde_json::to_value(schemars::schema_for!(PipelineConfig)).expect("schema serialization")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_stage(name: &str) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            ..PipelineStage::default()
        }
    }

    fn minimal_config(stages: Vec<PipelineStage>) -> PipelineConfig {
        PipelineConfig::test_default("test", stages)
    }

    #[test]
    fn test_validate_duplicate_stage_names() {
        let config = minimal_config(vec![minimal_stage("a"), minimal_stage("a")]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Duplicate stage name 'a'"));
    }

    #[test]
    fn test_validate_empty_stage_name() {
        let config = minimal_config(vec![minimal_stage("")]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Stage name must not be empty"));
    }

    #[test]
    fn test_validate_agent_empty_agent_field() {
        let mut stage = minimal_stage("worker");
        stage.agent = String::new();
        let config = minimal_config(vec![stage]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("must have a non-empty agent field"));
    }

    #[test]
    fn test_validate_self_loop_without_max_visits() {
        let mut stage = minimal_stage("loop");
        stage.default_next = Some("loop".to_string());
        let config = minimal_config(vec![stage]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("infinite loop"));
    }

    #[test]
    fn test_validate_self_loop_with_max_visits_ok() {
        let mut stage = minimal_stage("loop");
        stage.default_next = Some("loop".to_string());
        stage.max_visits = Some(3);
        let config = minimal_config(vec![stage]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_duplicate_output_key() {
        let mut a = minimal_stage("a");
        a.output_key = Some("shared".to_string());
        let mut b = minimal_stage("b");
        b.output_key = Some("shared".to_string());
        let config = minimal_config(vec![a, b]);
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Duplicate output_key 'shared'"));
    }

    #[test]
    fn test_validate_valid_pipeline() {
        let mut router = minimal_stage("router");
        router.routing_fn = Some("decide".to_string());
        router.default_next = Some("fallback".to_string());

        let fallback = minimal_stage("fallback");
        let config = minimal_config(vec![router, fallback]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pipeline_config_json_schema() {
        let schema = pipeline_config_json_schema();
        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();
        assert!(
            obj.contains_key("type") || obj.contains_key("$ref") || obj.contains_key("definitions"),
            "Schema top-level keys: {:?}",
            obj.keys().collect::<Vec<_>>()
        );
    }
}
