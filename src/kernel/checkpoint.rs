//! Checkpoint/resume support for pipeline durability.
//!
//! Checkpoints capture kernel state at instruction boundaries (after agent
//! result processing, before next instruction). This enables crash recovery
//! and pipeline migration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::instrument;

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::{EdgeKey, ParallelGroupState, PipelineConfig, SessionState};
use crate::kernel::types::ProcessControlBlock;
use crate::types::{Error, ProcessId, Result};

use super::Kernel;

/// A single edge traversal count entry in a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeTraversalEntry {
    pub edge: EdgeKey,
    pub count: i32,
}

/// Serializable snapshot of a running pipeline process.
///
/// Captured at instruction boundaries — after `process_agent_result()`,
/// before the next `get_next_instruction()`.
#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSnapshot {
    /// Schema version for forward compatibility
    pub version: String,
    /// When the checkpoint was taken
    pub timestamp: DateTime<Utc>,
    /// Process being checkpointed
    pub process_id: ProcessId,
    /// Full execution state (outputs, bounds, audit trail, interrupts)
    pub envelope: Envelope,
    /// Scheduling metadata (state, priority, quota, usage)
    pub pcb: ProcessControlBlock,
    /// Orchestration state (current stage, stage order, termination)
    pub session_state: SessionState,
    /// Per-edge traversal counts (type-safe EdgeKey, not string keys)
    pub edge_traversals: Vec<EdgeTraversalEntry>,
    /// Per-stage visit counts
    pub stage_visits: HashMap<String, i32>,
    /// Active parallel group state (if mid-fork)
    pub parallel_group: Option<ParallelGroupState>,
}

impl CheckpointSnapshot {
    pub const VERSION: &'static str = "1.0";
}

impl Kernel {
    /// Capture a checkpoint of a running process at the current instruction boundary.
    #[instrument(skip(self), fields(process_id = %process_id))]
    pub fn checkpoint(&self, process_id: &ProcessId) -> Result<CheckpointSnapshot> {
        let envelope = self.process_envelopes.get(process_id)
            .ok_or_else(|| Error::not_found(format!("Envelope not found: {}", process_id)))?;

        let pcb = self.lifecycle.get(process_id)
            .ok_or_else(|| Error::not_found(format!("Process not found: {}", process_id)))?
            .clone();

        let session = self.orchestrator.get_session(process_id)
            .ok_or_else(|| Error::not_found(format!("Session not found: {}", process_id)))?;

        let session_state = self.orchestrator.build_session_state(session, envelope);

        let edge_traversals: Vec<EdgeTraversalEntry> = session.edge_traversals.iter()
            .map(|(k, &v)| EdgeTraversalEntry { edge: k.clone(), count: v })
            .collect();

        let stage_visits = session.stage_visits.clone();
        let parallel_group = session.active_parallel.clone();

        Ok(CheckpointSnapshot {
            version: CheckpointSnapshot::VERSION.to_string(),
            timestamp: Utc::now(),
            process_id: process_id.clone(),
            envelope: envelope.clone(),
            pcb,
            session_state,
            edge_traversals,
            stage_visits,
            parallel_group,
        })
    }

    /// Resume a process from a checkpoint snapshot.
    ///
    /// Creates a new process with restored state. The pipeline_config must match
    /// the one used when the checkpoint was taken.
    #[instrument(skip(self, snapshot, pipeline_config))]
    pub fn resume_from_checkpoint(
        &mut self,
        snapshot: CheckpointSnapshot,
        pipeline_config: PipelineConfig,
    ) -> Result<ProcessId> {
        // Validate snapshot integrity
        if snapshot.version != CheckpointSnapshot::VERSION {
            return Err(Error::validation(format!(
                "Checkpoint version mismatch: expected {}, got {}",
                CheckpointSnapshot::VERSION, snapshot.version
            )));
        }
        if snapshot.envelope.event_inbox.len() > 1024 {
            return Err(Error::validation(format!(
                "Checkpoint event_inbox too large: {} (max 1024)",
                snapshot.envelope.event_inbox.len()
            )));
        }

        let pid = snapshot.process_id.clone();

        // Restore envelope
        self.process_envelopes.insert(pid.clone(), snapshot.envelope);

        // Restore PCB via lifecycle manager
        self.lifecycle.restore_process(snapshot.pcb)?;

        // Initialize orchestrator session from config
        let envelope = self.process_envelopes.get_mut(&pid)
            .ok_or_else(|| Error::not_found("Just-inserted envelope missing"))?;
        let _state = self.orchestrator.initialize_session(pid.clone(), pipeline_config, envelope, true)?;

        // Restore edge traversals, stage visits, and parallel state
        if let Some(session) = self.orchestrator.get_session_mut(&pid) {
            for entry in snapshot.edge_traversals {
                session.edge_traversals.insert(entry.edge, entry.count);
            }
            session.stage_visits = snapshot.stage_visits;
            session.active_parallel = snapshot.parallel_group;
        }

        Ok(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Envelope;
    use crate::kernel::orchestrator_types::{PipelineConfig, PipelineStage, AgentConfig};
    use crate::kernel::{Kernel, SchedulingPriority};
    use crate::types::{ProcessId, RequestId, SessionId, UserId};

    fn test_stage(name: &str) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            agent_config: AgentConfig { has_llm: false, ..Default::default() },
            ..Default::default()
        }
    }

    fn test_config() -> PipelineConfig {
        PipelineConfig::test_default("test", vec![
            test_stage("stage1"),
            test_stage("stage2"),
        ])
    }

    fn setup_kernel_with_session() -> (Kernel, ProcessId) {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("cp-proc-1");

        let _ = kernel.create_process(
            pid.clone(),
            RequestId::must("req-1"),
            UserId::must("user-1"),
            SessionId::must("sess-1"),
            SchedulingPriority::Normal,
            None,
        );

        let config = test_config();
        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);
        let _state = kernel.initialize_orchestration(pid.clone(), config, envelope, false).unwrap();

        (kernel, pid)
    }

    #[test]
    fn test_checkpoint_captures_state() {
        let (kernel, pid) = setup_kernel_with_session();

        let snapshot = kernel.checkpoint(&pid).unwrap();

        assert_eq!(snapshot.version, "1.0");
        assert_eq!(snapshot.process_id, pid);
        assert_eq!(snapshot.session_state.current_stage, "stage1");
        assert!(!snapshot.session_state.terminated);
    }

    #[test]
    fn test_checkpoint_not_found() {
        let kernel = Kernel::new();
        let pid = ProcessId::must("nonexistent");

        let result = kernel.checkpoint(&pid);
        assert!(result.is_err());
    }

    #[test]
    fn test_checkpoint_serialization_roundtrip() {
        let (kernel, pid) = setup_kernel_with_session();

        let snapshot = kernel.checkpoint(&pid).unwrap();

        // Serialize to JSON and back
        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: CheckpointSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.version, snapshot.version);
        assert_eq!(restored.process_id, snapshot.process_id);
        assert_eq!(restored.session_state.current_stage, snapshot.session_state.current_stage);
    }

    #[test]
    fn test_resume_from_checkpoint() {
        let (kernel, pid) = setup_kernel_with_session();
        let snapshot = kernel.checkpoint(&pid).unwrap();

        // Resume into a fresh kernel
        let mut kernel2 = Kernel::new();
        let config = test_config();
        let restored_pid = kernel2.resume_from_checkpoint(snapshot, config).unwrap();

        assert_eq!(restored_pid, pid);

        // Verify state was restored
        assert!(kernel2.process_envelopes.contains_key(&restored_pid));
        assert!(kernel2.lifecycle.get(&restored_pid).is_some());
        assert!(kernel2.orchestrator.has_session(&restored_pid));
    }

    #[test]
    fn test_checkpoint_preserves_edge_traversals() {
        let (kernel, pid) = setup_kernel_with_session();
        let snapshot = kernel.checkpoint(&pid).unwrap();

        // Roundtrip
        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: CheckpointSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.edge_traversals, snapshot.edge_traversals);
    }

    #[test]
    fn test_edge_traversal_entry_roundtrip() {
        let entry = EdgeTraversalEntry {
            edge: EdgeKey::new("stage_a", "stage_b"),
            count: 5,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let restored: EdgeTraversalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, entry);
    }
}
