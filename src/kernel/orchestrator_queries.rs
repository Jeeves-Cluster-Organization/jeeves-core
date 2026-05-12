//! Orchestrator read-only queries — session state, stage config lookups.

use crate::run::Run;
use crate::types::{Error, RunId, Result};

use super::orchestrator::Orchestrator;
use crate::workflow::{Stage, StateField};
use crate::kernel::protocol::{RunSnapshot};

impl Orchestrator {
    /// Get a reference to a pipeline session by process ID.
    pub fn get_session(&self, run_id: &RunId) -> Option<&super::orchestrator::Orchestration> {
        self.pipelines.get(run_id)
    }

    /// Get a mutable reference to a pipeline session by process ID.
    pub fn get_session_mut(&mut self, run_id: &RunId) -> Option<&mut super::orchestrator::Orchestration> {
        self.pipelines.get_mut(run_id)
    }

    /// Get session state for external queries.
    ///
    /// Run is passed in by the Kernel (which owns it).
    pub fn get_session_state(
        &self,
        run_id: &RunId,
        envelope: &Run,
    ) -> Result<RunSnapshot> {
        let session = self
            .pipelines
            .get(run_id)
            .ok_or_else(|| Error::not_found(format!("Unknown process: {}", run_id)))?;

        Ok(self.build_session_state(session, envelope))
    }

    /// Get the response_format for a specific stage. The kernel forwards this
    /// verbatim to the LLM provider; it does not interpret the value.
    pub fn get_stage_response_format(&self, run_id: &RunId, stage_name: &str) -> Option<serde_json::Value> {
        self.pipelines.get(run_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
                    .and_then(|s| s.response_format.clone())
            })
    }

    /// Get the state_schema for a process's pipeline.
    pub fn get_state_schema(&self, run_id: &RunId) -> Option<&Vec<StateField>> {
        self.pipelines.get(run_id)
            .map(|session| &session.pipeline_config.state_schema)
    }

    /// Get the output_key for a stage, defaulting to the stage name.
    pub fn get_stage_output_key(&self, run_id: &RunId, stage_name: &str) -> Option<String> {
        self.pipelines.get(run_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
                    .map(|s| s.output_key.clone().unwrap_or_else(|| s.name.clone()))
            })
    }

    /// Get the full stage config for a stage by name.
    pub fn get_stage_config(&self, run_id: &RunId, stage_name: &str) -> Option<&Stage> {
        self.pipelines.get(run_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
            })
    }

    /// Get pipeline session count.
    pub fn get_session_count(&self) -> usize {
        self.pipelines.len()
    }

    /// Take the last routing decision from a session (Option::take — returns and clears).
    pub fn take_last_routing_decision(&mut self, run_id: &RunId) -> Option<super::routing::RoutingDecision> {
        self.pipelines.get_mut(run_id)
            .and_then(|session| session.last_routing_decision.take())
    }
}

#[cfg(test)]
mod tests {
    use super::super::orchestrator::Orchestrator;
    use super::super::test_helpers::*;
    use crate::types::RunId;

    #[test]
    fn test_get_session_state() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(RunId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let state = orch.get_session_state(&RunId::must("proc1"), &envelope).unwrap();

        assert_eq!(state.run_id.as_str(), "proc1");
        assert_eq!(state.current_stage, "stage1");
        assert!(state.run.is_object());
        assert!(!state.terminated);
    }

    #[test]
    fn test_get_session_state_not_found() {
        let orch = Orchestrator::new();
        let envelope = create_test_envelope();

        let result = orch.get_session_state(&RunId::must("nonexistent"), &envelope);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_session_count() {
        let mut orch = Orchestrator::new();
        assert_eq!(orch.get_session_count(), 0);

        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();
        let _state = orch.initialize_session(RunId::must("proc1"), pipeline, &mut envelope, false).unwrap();
        assert_eq!(orch.get_session_count(), 1);
    }
}
