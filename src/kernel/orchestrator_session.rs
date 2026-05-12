//! Orchestrator session lifecycle — initialization, cleanup, state building.

use crate::envelope::Envelope;
use crate::types::{Error, ProcessId, Result};
use chrono::Utc;
use tracing::instrument;

use super::orchestrator::{Orchestrator, PipelineSession};
use crate::workflow::{PipelineConfig};
use crate::kernel::protocol::{SessionState};

impl Orchestrator {
    /// Initialize a new pipeline session.
    ///
    /// Takes a mutable envelope reference to set pipeline bounds (kernel owns the envelope).
    /// If force is false and a session already exists, returns an error.
    /// If force is true, replaces any existing session.
    #[instrument(skip(self, pipeline_config, envelope), fields(process_id = %process_id))]
    pub fn initialize_session(
        &mut self,
        process_id: ProcessId,
        pipeline_config: PipelineConfig,
        envelope: &mut Envelope,
        force: bool,
    ) -> Result<SessionState> {
        // Check for duplicate processID
        if self.pipelines.contains_key(&process_id) && !force {
            return Err(Error::validation(format!(
                "Session already exists for process: {} (use force=true to replace)",
                process_id
            )));
        }

        // Validate pipeline config
        pipeline_config.validate()?;

        // Initialize envelope with pipeline bounds
        envelope.pipeline.max_iterations = pipeline_config.max_iterations;
        envelope.bounds.max_llm_calls = pipeline_config.max_llm_calls;
        envelope.bounds.max_agent_hops = pipeline_config.max_agent_hops;
        envelope.pipeline.stage_order = pipeline_config.get_stage_order();

        // Initialize execution tracking
        envelope.pipeline.max_stages = pipeline_config.stages.len() as i32;
        envelope.pipeline.current_stage_number = 1;

        // Set initial stage if not set
        if envelope.pipeline.current_stage.is_empty() && !envelope.pipeline.stage_order.is_empty() {
            envelope.pipeline.current_stage = envelope.pipeline.stage_order[0].clone();
        }

        let now = Utc::now();
        let session = PipelineSession {
            process_id: process_id.clone(),
            pipeline_config,
            stage_visits: std::collections::HashMap::new(),
            created_at: now,
            last_activity_at: now,
            last_routing_decision: None,
        };

        let state = self.build_session_state(&session, envelope);
        self.pipelines.insert(process_id, session);

        Ok(state)
    }

    /// Check if a pipeline session exists for the given process.
    pub fn has_session(&self, process_id: &ProcessId) -> bool {
        self.pipelines.contains_key(process_id)
    }

    /// Cleanup a pipeline session.
    pub fn cleanup_session(&mut self, process_id: &ProcessId) -> bool {
        self.pipelines.remove(process_id).is_some()
    }

    /// Cleanup stale pipeline sessions older than the given duration.
    /// Returns the PIDs of removed sessions so the Kernel can also clean up
    /// the corresponding envelopes from `process_envelopes`.
    pub fn cleanup_stale_sessions(&mut self, max_age_seconds: i64) -> Vec<ProcessId> {
        let cutoff = Utc::now() - chrono::TimeDelta::seconds(max_age_seconds);
        let mut to_remove = Vec::new();

        for (pid, session) in &self.pipelines {
            if session.last_activity_at < cutoff {
                to_remove.push(pid.clone());
            }
        }

        for pid in &to_remove {
            self.pipelines.remove(pid);
        }

        to_remove
    }

    /// Build external session state representation.
    pub(crate) fn build_session_state(&self, session: &PipelineSession, envelope: &Envelope) -> SessionState {
        let envelope_value = serde_json::to_value(envelope)
            .unwrap_or_default();

        SessionState {
            process_id: session.process_id.clone(),
            current_stage: envelope.pipeline.current_stage.clone(),
            stage_order: envelope.pipeline.stage_order.clone(),
            envelope: envelope_value,
            terminated: envelope.bounds.is_terminated(),
            terminal_reason: envelope.bounds.terminal_reason(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::orchestrator::Orchestrator;
    use super::super::test_helpers::*;
    use crate::types::ProcessId;
    use chrono::Utc;

    #[test]
    fn test_initialize_session() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let state = orch
            .initialize_session(ProcessId::must("proc1"), pipeline.clone(), &mut envelope, false)
            .unwrap();

        assert_eq!(state.process_id.as_str(), "proc1");
        assert_eq!(state.current_stage, "stage1");
        assert_eq!(state.stage_order, vec!["stage1", "stage2"]);
        assert!(!state.terminated);
    }

    #[test]
    fn test_initialize_session_duplicate_fails() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline.clone(), &mut envelope, false)
            .unwrap();

        let mut envelope2 = create_test_envelope();
        let result =
            orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope2, false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_initialize_session_force_replaces() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline.clone(), &mut envelope, false)
            .unwrap();

        let mut envelope2 = create_test_envelope();
        let result =
            orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope2, true);

        assert!(result.is_ok());
    }

    #[test]
    fn test_has_session_true_after_init() {
        let mut orch = Orchestrator::new();
        let pid = ProcessId::must("p1");
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();
        let _state = orch.initialize_session(pid.clone(), pipeline, &mut envelope, false).unwrap();

        assert!(orch.has_session(&pid));
    }

    #[test]
    fn test_has_session_false_for_unknown_pid() {
        let orch = Orchestrator::new();
        assert!(!orch.has_session(&ProcessId::must("unknown")));
    }

    #[test]
    fn test_cleanup_session() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        assert_eq!(orch.get_session_count(), 1);

        let removed = orch.cleanup_session(&ProcessId::must("proc1"));
        assert!(removed);
        assert_eq!(orch.get_session_count(), 0);

        let removed = orch.cleanup_session(&ProcessId::must("proc1"));
        assert!(!removed);
    }

    #[test]
    fn test_cleanup_stale_sessions_removes_old_keeps_young() {
        let mut orch = Orchestrator::new();

        let pid_old = ProcessId::must("old");
        let pid_young = ProcessId::must("young");
        let pipeline = create_test_pipeline();

        let mut env1 = create_test_envelope();
        let _state = orch.initialize_session(pid_old.clone(), pipeline.clone(), &mut env1, false).unwrap();

        let mut env2 = create_test_envelope();
        let _state = orch.initialize_session(pid_young.clone(), pipeline, &mut env2, false).unwrap();

        // Manually set the old session's last_activity_at to the past
        if let Some(session) = orch.pipelines.get_mut(&pid_old) {
            session.last_activity_at = Utc::now() - chrono::TimeDelta::seconds(3600);
        }

        // Cleanup sessions older than 60 seconds
        let removed = orch.cleanup_stale_sessions(60);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], pid_old);

        assert!(!orch.has_session(&pid_old));
        assert!(orch.has_session(&pid_young));
    }
}
