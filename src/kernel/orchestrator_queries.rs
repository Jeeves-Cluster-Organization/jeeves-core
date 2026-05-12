//! Orchestrator read-only queries — session state, stage config lookups.

use crate::run::Run;
use crate::types::{Error, RunId, Result};

use super::orchestrator::Orchestrator;
use crate::workflow::{Stage, StateField};
use crate::kernel::protocol::{RunSnapshot};

impl Orchestrator {
    /// Get a reference to a workflow session by run ID.
    pub fn get_session(&self, run_id: &RunId) -> Option<&super::orchestrator::Orchestration> {
        self.sessions.get(run_id)
    }

    /// Get a mutable reference to a workflow session by run ID.
    pub fn get_session_mut(&mut self, run_id: &RunId) -> Option<&mut super::orchestrator::Orchestration> {
        self.sessions.get_mut(run_id)
    }

    /// Get session state for external queries.
    ///
    /// Run is passed in by the Kernel (which owns it).
    pub fn get_session_state(
        &self,
        run_id: &RunId,
        run: &Run,
    ) -> Result<RunSnapshot> {
        let session = self
            .sessions
            .get(run_id)
            .ok_or_else(|| Error::not_found(format!("Unknown run: {}", run_id)))?;

        Ok(self.build_session_state(session, run))
    }

    /// Get the response_format for a specific stage. The kernel forwards this
    /// verbatim to the LLM provider; it does not interpret the value.
    pub fn get_stage_response_format(&self, run_id: &RunId, stage_name: &str) -> Option<serde_json::Value> {
        self.sessions.get(run_id)
            .and_then(|session| {
                session.workflow.stages.iter()
                    .find(|s| s.name.as_str() == stage_name)
                    .and_then(|s| s.response_format.clone())
            })
    }

    /// Get the state_schema for a run's workflow.
    pub fn get_state_schema(&self, run_id: &RunId) -> Option<&Vec<StateField>> {
        self.sessions.get(run_id)
            .map(|session| &session.workflow.state_schema)
    }

    /// Get the output_key for a stage, defaulting to the stage name.
    pub fn get_stage_output_key(&self, run_id: &RunId, stage_name: &str) -> Option<String> {
        self.sessions.get(run_id)
            .and_then(|session| {
                session.workflow.stages.iter()
                    .find(|s| s.name.as_str() == stage_name)
                    .map(|s| {
                        s.output_key
                            .as_ref()
                            .map(|k| k.as_str().to_string())
                            .unwrap_or_else(|| s.name.as_str().to_string())
                    })
            })
    }

    /// Get the full stage config for a stage by name.
    pub fn get_stage_config(&self, run_id: &RunId, stage_name: &str) -> Option<&Stage> {
        self.sessions.get(run_id)
            .and_then(|session| {
                session.workflow.stages.iter()
                    .find(|s| s.name.as_str() == stage_name)
            })
    }

    /// Get workflow session count.
    pub fn get_session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Take the last routing decision from a session (Option::take — returns and clears).
    pub fn take_last_routing_decision(&mut self, run_id: &RunId) -> Option<super::routing::RoutingDecision> {
        self.sessions.get_mut(run_id)
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
        let workflow = create_test_workflow();
        let mut run = create_test_run();

        let _state = orch.initialize_session(RunId::must("proc1"), workflow, &mut run, false)
            .unwrap();

        let state = orch.get_session_state(&RunId::must("proc1"), &run).unwrap();

        assert_eq!(state.run_id.as_str(), "proc1");
        assert_eq!(state.current_stage.as_str(), "stage1");
        assert!(state.run.is_object());
        assert!(!state.terminated);
    }

    #[test]
    fn test_get_session_state_not_found() {
        let orch = Orchestrator::new();
        let run = create_test_run();

        let result = orch.get_session_state(&RunId::must("nonexistent"), &run);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_session_count() {
        let mut orch = Orchestrator::new();
        assert_eq!(orch.get_session_count(), 0);

        let workflow = create_test_workflow();
        let mut run = create_test_run();
        let _state = orch.initialize_session(RunId::must("proc1"), workflow, &mut run, false).unwrap();
        assert_eq!(orch.get_session_count(), 1);
    }
}
