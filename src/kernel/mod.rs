//! Kernel - the main orchestration actor.
//!
//! The Kernel owns all mutable state and processes commands via a single
//! message channel. Subsystems (lifecycle, resources, interrupts) are plain
//! structs owned by the Kernel, not separate actors.

use std::collections::HashMap;

// Core types
pub mod types;

// Subsystem modules
pub mod interrupts;
pub mod lifecycle;
pub mod orchestrator;
mod orchestrator_session;   // impl Orchestrator — session lifecycle
mod orchestrator_queries;   // impl Orchestrator — read-only queries
mod orchestrator_helpers;   // Free functions — routing helpers
#[cfg(test)]
pub(crate) mod test_helpers; // Shared test utilities for orchestrator tests
pub mod orchestrator_types;
pub mod resources;
pub mod routing;

// Kernel impl split across focused files
mod kernel_orchestration;

// Re-export key types
pub use interrupts::{InterruptService, PendingInterrupt};
pub use lifecycle::LifecycleManager;
pub use resources::ResourceTracker;
pub use types::{
    ProcessControlBlock, ProcessState, QuotaViolation, ResourceQuota, ResourceUsage,
};

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::MergeStrategy;
use crate::types::ProcessId;

/// Merge a value into envelope.state according to the configured strategy.
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
/// [`ToolRegistryBuilder::with_access_policy`]: crate::worker::tools::ToolRegistryBuilder::with_access_policy
#[derive(Debug)]
pub struct ToolDomain {
    pub(crate) health: crate::tools::ToolHealthTracker,
}

/// The kernel: orchestrates pipelines, owns lifecycle/resources/interrupts.
/// Driven through `KernelHandle` (the mpsc channel), never shared `&mut`.
#[derive(Debug)]
pub struct Kernel {
    /// Process lifecycle management
    pub(crate) lifecycle: LifecycleManager,

    /// Resource tracking and quota enforcement
    pub(crate) resources: ResourceTracker,

    /// Interrupt handling (human-in-the-loop)
    pub(crate) interrupts: interrupts::InterruptService,

    /// Pipeline orchestration (kernel-driven execution)
    pub(crate) orchestrator: orchestrator::Orchestrator,

    /// Process envelope storage (process_id -> envelope).
    pub(crate) process_envelopes: HashMap<ProcessId, Envelope>,

    /// Tool subsystem (catalog, access, health).
    pub(crate) tools: ToolDomain,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            lifecycle: LifecycleManager::default(),
            resources: ResourceTracker::default(),
            interrupts: interrupts::InterruptService::new(),
            orchestrator: orchestrator::Orchestrator::new(),
            process_envelopes: HashMap::new(),
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
            lifecycle: LifecycleManager::new(default_quota),
            resources: ResourceTracker::new(),
            interrupts: interrupts::InterruptService::new(),
            orchestrator: orchestrator::Orchestrator::new(),
            process_envelopes: HashMap::new(),
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
    pub processes_by_state: HashMap<ProcessState, usize>,
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

        let pid1 = ProcessId::must("pid1");
        let pid2 = ProcessId::must("pid2");

        kernel.lifecycle.create(pid1.clone(), RequestId::must("req1"), UserId::must("user1"), SessionId::must("sess1"), None).unwrap();
        kernel.lifecycle.create(pid2.clone(), RequestId::must("req2"), UserId::must("user2"), SessionId::must("sess2"), None).unwrap();

        kernel.lifecycle.run(&pid1).unwrap();

        let status = kernel.get_system_status();

        assert_eq!(status.processes_total, 2);
        assert_eq!(*status.processes_by_state.get(&ProcessState::Ready).unwrap(), 1);
        assert_eq!(*status.processes_by_state.get(&ProcessState::Running).unwrap(), 1);
    }

    #[test]
    fn test_record_tool_call_missing_pid_returns_err() {
        let mut kernel = Kernel::new();
        let result = kernel.record_tool_call(&ProcessId::must("nonexistent"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_record_agent_hop_missing_pid_returns_err() {
        let mut kernel = Kernel::new();
        let result = kernel.record_agent_hop(&ProcessId::must("nonexistent"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_pcb_sync_after_agent_result() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("p1");

        kernel.create_process(pid.clone(), RequestId::must("req1"), UserId::must("user1"), SessionId::must("sess1"), None).unwrap();
        kernel.record_usage(&pid, "user1", 3, 5, 1000, 500);

        let pcb = kernel.lifecycle.get(&pid).unwrap();
        assert_eq!(pcb.usage.llm_calls, 3);
        assert_eq!(pcb.usage.tool_calls, 5);
        assert_eq!(pcb.usage.tokens_in, 1000);
        assert_eq!(pcb.usage.tokens_out, 500);
    }

    #[test]
    fn test_process_lifecycle_create_and_destroy() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("p1");

        kernel.create_process(pid.clone(), RequestId::must("req1"), UserId::must("user1"), SessionId::must("sess1"), None).unwrap();
        assert!(kernel.lifecycle.get(&pid).is_some());
        assert_eq!(kernel.lifecycle.count(), 1);

        kernel.lifecycle.run(&pid).unwrap();
        kernel.terminate_process(&pid).unwrap();

        assert!(kernel.lifecycle.get(&pid).is_none());
        assert_eq!(kernel.lifecycle.count(), 0);
    }

    #[test]
    fn test_record_tool_call_increments_counter() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("p1");

        kernel.create_process(pid.clone(), RequestId::must("req1"), UserId::must("user1"), SessionId::must("sess1"), None).unwrap();

        kernel.record_tool_call(&pid).unwrap();
        kernel.record_tool_call(&pid).unwrap();
        kernel.record_tool_call(&pid).unwrap();

        let pcb = kernel.lifecycle.get(&pid).unwrap();
        assert_eq!(pcb.usage.tool_calls, 3);
    }
}

#[cfg(test)]
mod merge_tests {
    use super::merge_state_field;
    use crate::kernel::orchestrator_types::MergeStrategy;
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
