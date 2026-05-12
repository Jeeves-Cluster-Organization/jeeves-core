//! Orchestrator read-only queries — session state, stage config lookups.

use crate::envelope::Envelope;
use crate::types::{Error, ProcessId, Result};

use super::orchestrator::Orchestrator;
use super::orchestrator_types::{PipelineStage, SessionState, StateField};

impl Orchestrator {
    /// Get a reference to a pipeline session by process ID.
    pub fn get_session(&self, process_id: &ProcessId) -> Option<&super::orchestrator::PipelineSession> {
        self.pipelines.get(process_id)
    }

    /// Get a mutable reference to a pipeline session by process ID.
    pub fn get_session_mut(&mut self, process_id: &ProcessId) -> Option<&mut super::orchestrator::PipelineSession> {
        self.pipelines.get_mut(process_id)
    }

    /// Get session state for external queries.
    ///
    /// Envelope is passed in by the Kernel (which owns it).
    pub fn get_session_state(
        &self,
        process_id: &ProcessId,
        envelope: &Envelope,
    ) -> Result<SessionState> {
        let session = self
            .pipelines
            .get(process_id)
            .ok_or_else(|| Error::not_found(format!("Unknown process: {}", process_id)))?;

        Ok(self.build_session_state(session, envelope))
    }

    /// Get the response_format for a specific stage. The kernel forwards this
    /// verbatim to the LLM provider; it does not interpret the value.
    pub fn get_stage_response_format(&self, process_id: &ProcessId, stage_name: &str) -> Option<serde_json::Value> {
        self.pipelines.get(process_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
                    .and_then(|s| s.response_format.clone())
            })
    }

    /// Get the state_schema for a process's pipeline.
    pub fn get_state_schema(&self, process_id: &ProcessId) -> Option<&Vec<StateField>> {
        self.pipelines.get(process_id)
            .map(|session| &session.pipeline_config.state_schema)
    }

    /// Get the output_key for a stage, defaulting to the stage name.
    pub fn get_stage_output_key(&self, process_id: &ProcessId, stage_name: &str) -> Option<String> {
        self.pipelines.get(process_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
                    .map(|s| s.output_key.clone().unwrap_or_else(|| s.name.clone()))
            })
    }

    /// Get the full stage config for a stage by name.
    pub fn get_stage_config(&self, process_id: &ProcessId, stage_name: &str) -> Option<&PipelineStage> {
        self.pipelines.get(process_id)
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
    pub fn take_last_routing_decision(&mut self, process_id: &ProcessId) -> Option<super::routing::RoutingDecision> {
        self.pipelines.get_mut(process_id)
            .and_then(|session| session.last_routing_decision.take())
    }
}

#[cfg(test)]
mod tests {
    use super::super::orchestrator::Orchestrator;
    use super::super::test_helpers::*;
    use crate::types::ProcessId;

    #[test]
    fn test_get_session_state() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false)
            .unwrap();

        let state = orch.get_session_state(&ProcessId::must("proc1"), &envelope).unwrap();

        assert_eq!(state.process_id.as_str(), "proc1");
        assert_eq!(state.current_stage, "stage1");
        assert!(state.envelope.is_object());
        assert!(!state.terminated);
    }

    #[test]
    fn test_get_session_state_not_found() {
        let orch = Orchestrator::new();
        let envelope = create_test_envelope();

        let result = orch.get_session_state(&ProcessId::must("nonexistent"), &envelope);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_session_count() {
        let mut orch = Orchestrator::new();
        assert_eq!(orch.get_session_count(), 0);

        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();
        let _state = orch.initialize_session(ProcessId::must("proc1"), pipeline, &mut envelope, false).unwrap();
        assert_eq!(orch.get_session_count(), 1);
    }
}
