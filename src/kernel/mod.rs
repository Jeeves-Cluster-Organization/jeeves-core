//! Kernel - the main orchestration actor.
//!
//! The Kernel owns all mutable state and processes commands via a single
//! message channel. Subsystems (lifecycle, resources, interrupts) are plain
//! structs owned by the Kernel, not separate actors.

use std::collections::HashMap;

pub mod actor;
pub mod handle;
pub mod interrupts;
pub mod lifecycle;
pub mod orchestrator;
mod orchestrator_queries;
mod orchestrator_session;
pub mod protocol;
pub mod resources;
pub mod routing;
pub mod runner;
pub mod types;

#[cfg(test)]
pub(crate) mod test_helpers;

mod dispatch;

// Re-export key types
pub use interrupts::{InterruptService, PendingInterrupt};
pub use lifecycle::RunRegistry;
pub use resources::ResourceTracker;
pub use types::{
    RunRecord, RunStatus, QuotaViolation, ResourceQuota, ResourceUsage,
};

use crate::run::Run;
use crate::workflow::MergeStrategy;
use crate::types::RunId;

/// Merge a value into run.state according to the configured strategy.
fn merge_state_field(
    state: &mut HashMap<String, serde_json::Value>,
    key: &str,
    val: serde_json::Value,
    strategy: MergeStrategy,
) {
    match strategy {
        MergeStrategy::Replace => {
            state.insert(key.to_string(), val);
        }
        MergeStrategy::Append => {
            let arr = state.entry(key.to_string()).or_insert_with(|| serde_json::json!([]));
            if let Some(existing) = arr.as_array_mut() {
                existing.push(val);
            } else {
                // Not an array — replace with [existing, val]
                let old = arr.clone();
                *arr = serde_json::json!([old, val]);
            }
        }
        MergeStrategy::MergeDict => {
            if let serde_json::Value::Object(new_map) = val {
                let existing = state.entry(key.to_string()).or_insert_with(|| serde_json::json!({}));
                if let Some(existing_map) = existing.as_object_mut() {
                    for (k, v) in new_map {
                        existing_map.insert(k, v);
                    }
                } else {
                    *existing = serde_json::Value::Object(new_map);
                }
            } else {
                state.insert(key.to_string(), val);
            }
        }
    }
}

/// Kernel-side tool subsystem. ACL lives on the consumer's `ToolRegistry`
/// via [`ToolRegistryBuilder::with_access_policy`]; only health tracking
/// hangs off the kernel.
///
/// [`ToolRegistryBuilder::with_access_policy`]: crate::tools::ToolRegistryBuilder::with_access_policy
#[derive(Debug)]
pub struct ToolDomain {
    pub(crate) health: crate::tools::ToolHealthTracker,
}

/// The kernel: orchestrates pipelines, owns lifecycle/resources/interrupts.
/// Driven through `KernelHandle` (the mpsc channel), never shared `&mut`.
#[derive(Debug)]
pub struct Kernel {
    /// Process lifecycle management
    pub(crate) lifecycle: RunRegistry,

    /// Resource tracking and quota enforcement
    pub(crate) resources: ResourceTracker,

    /// Interrupt handling (human-in-the-loop)
    pub(crate) interrupts: interrupts::InterruptService,

    /// Pipeline orchestration (kernel-driven execution)
    pub(crate) orchestrator: orchestrator::Orchestrator,

    /// Process run storage (run_id -> run).
    pub(crate) runs: HashMap<RunId, Run>,

    /// Tool subsystem (catalog, access, health).
    pub(crate) tools: ToolDomain,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            lifecycle: RunRegistry::default(),
            resources: ResourceTracker::default(),
            interrupts: interrupts::InterruptService::new(),
            orchestrator: orchestrator::Orchestrator::new(),
            runs: HashMap::new(),
            tools: ToolDomain {
                health: crate::tools::ToolHealthTracker::default(),
            },
        }
    }

    /// Register a routing function by name.
    pub fn register_routing_fn(&mut self, name: impl Into<String>, f: std::sync::Arc<dyn orchestrator::RoutingFn>) {
        self.orchestrator.register_routing_fn(name, f);
    }

    /// Create a Kernel wired from a Config struct.
    pub fn from_config(config: &crate::Config) -> Self {
        let default_quota = ResourceQuota {
            max_llm_calls: config.defaults.max_llm_calls,
            max_tool_calls: config.defaults.max_tool_calls,
            max_agent_hops: config.defaults.max_agent_hops,
            max_iterations: config.defaults.max_iterations,
            timeout_seconds: config.defaults.process_timeout.as_secs() as i32,
            ..ResourceQuota::default()
        };
        Self::with_quota(Some(default_quota))
    }

    /// Construct a Kernel with an optional default quota for new processes.
    pub fn with_quota(default_quota: Option<ResourceQuota>) -> Self {
        Self {
            lifecycle: RunRegistry::new(default_quota),
            resources: ResourceTracker::new(),
            interrupts: interrupts::InterruptService::new(),
            orchestrator: orchestrator::Orchestrator::new(),
            runs: HashMap::new(),
            tools: ToolDomain {
                health: crate::tools::ToolHealthTracker::default(),
            },
        }
    }
}

/// Remaining resource budget for a process.
#[derive(Debug, Clone)]
pub struct RemainingBudget {
    pub llm_calls_remaining: i32,
    pub iterations_remaining: i32,
    pub agent_hops_remaining: i32,
    pub tokens_in_remaining: i64,
    pub tokens_out_remaining: i64,
    pub time_remaining_seconds: f64,
}

/// Full system status snapshot returned by `Kernel::get_system_status()`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemStatus {
    pub processes_total: usize,
    pub processes_by_state: HashMap<RunStatus, usize>,
    pub active_orchestration_sessions: usize,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RequestId, SessionId, UserId};

    #[test]
    fn test_get_system_status_empty_kernel() {
        let kernel = Kernel::new();
        let status = kernel.get_system_status();

        assert_eq!(status.processes_total, 0);
        assert_eq!(status.active_orchestration_sessions, 0);

        for (_state, count) in &status.processes_by_state {
            assert_eq!(*count, 0);
        }
    }

    #[test]
    fn test_get_system_status_with_processes() {
        let mut kernel = Kernel::new();

        let pid1 = RunId::must("pid1");
        let pid2 = RunId::must("pid2");

        kernel.lifecycle.create(pid1.clone(), RequestId::must("req1"), UserId::must("user1"), SessionId::must("sess1"), None).unwrap();
        kernel.lifecycle.create(pid2.clone(), RequestId::must("req2"), UserId::must("user2"), SessionId::must("sess2"), None).unwrap();

        kernel.lifecycle.run(&pid1).unwrap();

        let status = kernel.get_system_status();

        assert_eq!(status.processes_total, 2);
        assert_eq!(*status.processes_by_state.get(&RunStatus::Ready).unwrap(), 1);
        assert_eq!(*status.processes_by_state.get(&RunStatus::Running).unwrap(), 1);
    }

    #[test]
    fn test_user_usage_recorded() {
        let mut kernel = Kernel::new();
        kernel.record_user_usage("user1", 3, 5, 1000, 500);

        let usage = kernel.resources.get_user_usage("user1").unwrap();
        assert_eq!(usage.llm_calls, 3);
        assert_eq!(usage.tool_calls, 5);
        assert_eq!(usage.tokens_in, 1000);
        assert_eq!(usage.tokens_out, 500);
    }

    #[test]
    fn test_process_lifecycle_create_and_destroy() {
        let mut kernel = Kernel::new();
        let pid = RunId::must("p1");

        kernel.create_process(pid.clone(), RequestId::must("req1"), UserId::must("user1"), SessionId::must("sess1"), None).unwrap();
        assert!(kernel.lifecycle.get(&pid).is_some());
        assert_eq!(kernel.lifecycle.count(), 1);

        kernel.lifecycle.run(&pid).unwrap();
        kernel.terminate_process(&pid).unwrap();

        assert!(kernel.lifecycle.get(&pid).is_none());
        assert_eq!(kernel.lifecycle.count(), 0);
    }

}

#[cfg(test)]
mod merge_tests {
    use super::merge_state_field;
    use crate::workflow::MergeStrategy;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn replace_overwrites_existing() {
        let mut state = HashMap::new();
        state.insert("key".to_string(), json!("old"));
        merge_state_field(&mut state, "key", json!("new"), MergeStrategy::Replace);
        assert_eq!(state["key"], json!("new"));
    }

    #[test]
    fn replace_creates_new() {
        let mut state = HashMap::new();
        merge_state_field(&mut state, "key", json!("value"), MergeStrategy::Replace);
        assert_eq!(state["key"], json!("value"));
    }

    #[test]
    fn append_creates_array() {
        let mut state = HashMap::new();
        merge_state_field(&mut state, "docs", json!({"a": 1}), MergeStrategy::Append);
        assert_eq!(state["docs"], json!([{"a": 1}]));
    }

    #[test]
    fn append_to_existing_array() {
        let mut state = HashMap::new();
        state.insert("docs".to_string(), json!([{"a": 1}]));
        merge_state_field(&mut state, "docs", json!({"b": 2}), MergeStrategy::Append);
        assert_eq!(state["docs"], json!([{"a": 1}, {"b": 2}]));
    }

    #[test]
    fn append_converts_non_array() {
        let mut state = HashMap::new();
        state.insert("docs".to_string(), json!("scalar"));
        merge_state_field(&mut state, "docs", json!("new"), MergeStrategy::Append);
        assert_eq!(state["docs"], json!(["scalar", "new"]));
    }

    #[test]
    fn merge_dict_shallow_merges() {
        let mut state = HashMap::new();
        state.insert("ctx".to_string(), json!({"a": 1, "b": 2}));
        merge_state_field(&mut state, "ctx", json!({"b": 3, "c": 4}), MergeStrategy::MergeDict);
        assert_eq!(state["ctx"], json!({"a": 1, "b": 3, "c": 4}));
    }

    #[test]
    fn merge_dict_creates_new() {
        let mut state = HashMap::new();
        merge_state_field(&mut state, "ctx", json!({"a": 1}), MergeStrategy::MergeDict);
        assert_eq!(state["ctx"], json!({"a": 1}));
    }

    #[test]
    fn merge_dict_non_object_falls_back_to_replace() {
        let mut state = HashMap::new();
        state.insert("ctx".to_string(), json!({"a": 1}));
        merge_state_field(&mut state, "ctx", json!("not_an_object"), MergeStrategy::MergeDict);
        assert_eq!(state["ctx"], json!("not_an_object"));
    }
}
