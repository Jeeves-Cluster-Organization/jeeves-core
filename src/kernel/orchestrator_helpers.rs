//! Helper functions for pipeline orchestration.
//!
//! These are module-level functions (not methods on Orchestrator) to avoid
//! borrow checker issues when the Orchestrator needs to pass its own fields
//! to helpers while holding a mutable borrow on `pipelines`.

use crate::types::{Error, Result};
use std::collections::HashMap;

use super::orchestrator_types::PipelineConfig;

/// Get agent name for a stage.
pub(crate) fn get_agent_for_stage(
    pipeline_config: &PipelineConfig,
    stage_name: &str,
) -> Result<String> {
    pipeline_config
        .stages
        .iter()
        .find(|s| s.name == stage_name)
        .map(|s| s.agent.clone())
        .ok_or_else(|| Error::not_found(format!("Stage not found in pipeline: {}", stage_name)))
}

/// Build a stage completion snapshot from agent output.
pub(crate) fn build_stage_snapshot(
    stage_name: &str,
    agent_output: &HashMap<String, serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    [
        ("stage".to_string(), serde_json::Value::String(stage_name.to_string())),
        ("output".to_string(), serde_json::Value::Object(
            agent_output.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        )),
    ].into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::orchestrator_types::PipelineStage;

    #[test]
    fn test_get_agent_for_stage_found() {
        let config = PipelineConfig::test_default("test", vec![PipelineStage {
            name: "s1".to_string(),
            agent: "agent1".to_string(),
            ..Default::default()
        }]);
        assert_eq!(get_agent_for_stage(&config, "s1").unwrap(), "agent1");
    }

    #[test]
    fn test_get_agent_for_stage_not_found() {
        let config = PipelineConfig::test_default("test", vec![]);
        assert!(get_agent_for_stage(&config, "missing").is_err());
    }

    #[test]
    fn test_build_stage_snapshot() {
        let mut output = HashMap::new();
        output.insert("key".to_string(), serde_json::json!("value"));
        let snapshot = build_stage_snapshot("my_stage", &output);
        assert_eq!(snapshot.get("stage").unwrap(), &serde_json::json!("my_stage"));
        assert!(snapshot.get("output").unwrap().is_object());
    }
}
