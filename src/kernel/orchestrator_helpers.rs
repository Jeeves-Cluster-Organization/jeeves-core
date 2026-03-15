//! Helper functions for pipeline orchestration.
//!
//! These are module-level functions (not methods on Orchestrator) to avoid
//! borrow checker issues when the Orchestrator needs to pass its own fields
//! to helpers while holding a mutable borrow on `pipelines`.

use crate::types::{Error, Result};
use std::collections::HashMap;

use super::orchestrator_types::{EdgeKey, PipelineConfig, ParallelGroupState, JoinStrategy};

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

/// Check if an edge transition is within its limit.
/// Returns true if the transition is allowed.
pub(crate) fn check_edge_limit(
    config: &PipelineConfig,
    edge_traversals: &HashMap<EdgeKey, i32>,
    from_stage: &str,
    to_stage: &str,
) -> bool {
    for limit in &config.edge_limits {
        if limit.from_stage == from_stage && limit.to_stage == to_stage {
            let edge_key = EdgeKey::new(from_stage, to_stage);
            let count = edge_traversals.get(&edge_key).copied().unwrap_or(0);
            return count < limit.max_count;
        }
    }
    // No limit configured — allowed
    true
}

/// Check if a parallel group's join condition is met.
///
/// `pending_agents` is drained on dispatch, so join is based on reported count vs total stages.
pub(crate) fn is_parallel_join_met(parallel: &ParallelGroupState) -> bool {
    let reported = parallel.completed_agents.len() + parallel.failed_agents.len();
    match parallel.join_strategy {
        JoinStrategy::WaitAll => reported >= parallel.stages.len(),
        JoinStrategy::WaitFirst => reported > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::orchestrator_types::EdgeLimit;
    use std::collections::HashSet;

    #[test]
    fn test_get_agent_for_stage_found() {
        use super::super::orchestrator_types::PipelineStage;
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

    #[test]
    fn test_check_edge_limit_no_limits() {
        let config = PipelineConfig::test_default("test", vec![]);
        assert!(check_edge_limit(&config, &HashMap::new(), "a", "b"));
    }

    #[test]
    fn test_check_edge_limit_within() {
        let mut config = PipelineConfig::test_default("test", vec![]);
        config.edge_limits = vec![EdgeLimit {
            from_stage: "a".to_string(),
            to_stage: "b".to_string(),
            max_count: 3,
        }];
        let mut traversals = HashMap::new();
        traversals.insert(EdgeKey::new("a", "b"), 2);
        assert!(check_edge_limit(&config, &traversals, "a", "b"));
    }

    #[test]
    fn test_check_edge_limit_exceeded() {
        let mut config = PipelineConfig::test_default("test", vec![]);
        config.edge_limits = vec![EdgeLimit {
            from_stage: "a".to_string(),
            to_stage: "b".to_string(),
            max_count: 2,
        }];
        let mut traversals = HashMap::new();
        traversals.insert(EdgeKey::new("a", "b"), 2);
        assert!(!check_edge_limit(&config, &traversals, "a", "b"));
    }

    #[test]
    fn test_is_parallel_join_met_wait_all() {
        let parallel = ParallelGroupState {
            group_name: "fork".to_string(),
            stages: vec!["a".to_string(), "b".to_string()],
            pending_agents: HashSet::new(),
            completed_agents: ["agent_a".to_string(), "agent_b".to_string()].into(),
            failed_agents: HashMap::new(),
            join_strategy: JoinStrategy::WaitAll,
        };
        assert!(is_parallel_join_met(&parallel));
    }

    #[test]
    fn test_is_parallel_join_met_wait_all_incomplete() {
        let parallel = ParallelGroupState {
            group_name: "fork".to_string(),
            stages: vec!["a".to_string(), "b".to_string()],
            pending_agents: HashSet::new(),
            completed_agents: ["agent_a".to_string()].into(),
            failed_agents: HashMap::new(),
            join_strategy: JoinStrategy::WaitAll,
        };
        assert!(!is_parallel_join_met(&parallel));
    }

    #[test]
    fn test_is_parallel_join_met_wait_first() {
        let parallel = ParallelGroupState {
            group_name: "fork".to_string(),
            stages: vec!["a".to_string(), "b".to_string()],
            pending_agents: HashSet::new(),
            completed_agents: ["agent_a".to_string()].into(),
            failed_agents: HashMap::new(),
            join_strategy: JoinStrategy::WaitFirst,
        };
        assert!(is_parallel_join_met(&parallel));
    }

    #[test]
    fn test_is_parallel_join_met_wait_first_none() {
        let parallel = ParallelGroupState {
            group_name: "fork".to_string(),
            stages: vec!["a".to_string(), "b".to_string()],
            pending_agents: HashSet::new(),
            completed_agents: HashSet::new(),
            failed_agents: HashMap::new(),
            join_strategy: JoinStrategy::WaitFirst,
        };
        assert!(!is_parallel_join_met(&parallel));
    }
}
