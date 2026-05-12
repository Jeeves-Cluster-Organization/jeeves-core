//! Orchestrator session lifecycle — initialization, cleanup, state building.

use crate::run::Run;
use crate::types::{Error, RunId, Result};
use chrono::Utc;
use tracing::instrument;

use super::orchestrator::{Orchestrator, Orchestration};
use crate::workflow::{Workflow};
use crate::kernel::protocol::{RunSnapshot};

impl Orchestrator {
    /// Initialize a new pipeline session.
    ///
    /// Takes a mutable run reference to set pipeline bounds (kernel owns the run).
    /// If force is false and a session already exists, returns an error.
    /// If force is true, replaces any existing session.
    #[instrument(skip(self, pipeline_config, run), fields(run_id = %run_id))]
    pub fn initialize_session(
        &mut self,
        run_id: RunId,
        pipeline_config: Workflow,
        run: &mut Run,
        force: bool,
    ) -> Result<RunSnapshot> {
        // Check for duplicate processID
        if self.pipelines.contains_key(&run_id) && !force {
            return Err(Error::validation(format!(
                "Session already exists for process: {} (use force=true to replace)",
                run_id
            )));
        }

        // Validate pipeline config
        pipeline_config.validate()?;

        // Initialize run with pipeline bounds
        run.max_iterations = pipeline_config.max_iterations;
        run.limits.max_llm_calls = pipeline_config.max_llm_calls;
        run.limits.max_agent_hops = pipeline_config.max_agent_hops;
        run.stage_order = pipeline_config.get_stage_order();

        // Set initial stage if not set
        if run.current_stage.is_empty() && !run.stage_order.is_empty() {
            run.current_stage = run.stage_order[0].clone();
        }

        let now = Utc::now();
        let session = Orchestration {
            run_id: run_id.clone(),
            pipeline_config,
            stage_visits: std::collections::HashMap::new(),
            created_at: now,
            last_activity_at: now,
            last_routing_decision: None,
        };

        let state = self.build_session_state(&session, run);
        self.pipelines.insert(run_id, session);

        Ok(state)
    }

    /// Check if a pipeline session exists for the given process.
    pub fn has_session(&self, run_id: &RunId) -> bool {
        self.pipelines.contains_key(run_id)
    }

    /// Cleanup a pipeline session.
    pub fn cleanup_session(&mut self, run_id: &RunId) -> bool {
        self.pipelines.remove(run_id).is_some()
    }

    /// Cleanup stale pipeline sessions older than the given duration.
    /// Returns the PIDs of removed sessions so the Kernel can also clean up
    /// the corresponding envelopes from `runs`.
    pub fn cleanup_stale_sessions(&mut self, max_age_seconds: i64) -> Vec<RunId> {
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
    pub(crate) fn build_session_state(&self, session: &Orchestration, run: &Run) -> RunSnapshot {
        let run_value = serde_json::to_value(run).unwrap_or_default();

        RunSnapshot {
            run_id: session.run_id.clone(),
            current_stage: run.current_stage.clone(),
            stage_order: run.stage_order.clone(),
            run: run_value,
            terminated: run.is_terminated(),
            terminal_reason: run.terminal_reason(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::orchestrator::Orchestrator;
    use super::super::test_helpers::*;
    use crate::types::RunId;
    use chrono::Utc;

    #[test]
    fn test_initialize_session() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut run = create_test_envelope();

        let state = orch
            .initialize_session(RunId::must("proc1"), pipeline.clone(), &mut run, false)
            .unwrap();

        assert_eq!(state.run_id.as_str(), "proc1");
        assert_eq!(state.current_stage, "stage1");
        assert_eq!(state.stage_order, vec!["stage1", "stage2"]);
        assert!(!state.terminated);
    }

    #[test]
    fn test_initialize_session_duplicate_fails() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut run = create_test_envelope();

        let _state = orch.initialize_session(RunId::must("proc1"), pipeline.clone(), &mut run, false)
            .unwrap();

        let mut envelope2 = create_test_envelope();
        let result =
            orch.initialize_session(RunId::must("proc1"), pipeline, &mut envelope2, false);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_initialize_session_force_replaces() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut run = create_test_envelope();

        let _state = orch.initialize_session(RunId::must("proc1"), pipeline.clone(), &mut run, false)
            .unwrap();

        let mut envelope2 = create_test_envelope();
        let result =
            orch.initialize_session(RunId::must("proc1"), pipeline, &mut envelope2, true);

        assert!(result.is_ok());
    }

    #[test]
    fn test_has_session_true_after_init() {
        let mut orch = Orchestrator::new();
        let pid = RunId::must("p1");
        let pipeline = create_test_pipeline();
        let mut run = create_test_envelope();
        let _state = orch.initialize_session(pid.clone(), pipeline, &mut run, false).unwrap();

        assert!(orch.has_session(&pid));
    }

    #[test]
    fn test_has_session_false_for_unknown_pid() {
        let orch = Orchestrator::new();
        assert!(!orch.has_session(&RunId::must("unknown")));
    }

    #[test]
    fn test_cleanup_session() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut run = create_test_envelope();

        let _state = orch.initialize_session(RunId::must("proc1"), pipeline, &mut run, false)
            .unwrap();

        assert_eq!(orch.get_session_count(), 1);

        let removed = orch.cleanup_session(&RunId::must("proc1"));
        assert!(removed);
        assert_eq!(orch.get_session_count(), 0);

        let removed = orch.cleanup_session(&RunId::must("proc1"));
        assert!(!removed);
    }

    #[test]
    fn test_cleanup_stale_sessions_removes_old_keeps_young() {
        let mut orch = Orchestrator::new();

        let pid_old = RunId::must("old");
        let pid_young = RunId::must("young");
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
