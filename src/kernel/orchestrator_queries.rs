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

    /// Get the output_schema for a specific stage in a process's pipeline.
    /// Returns a cloned Value so the caller can use it without holding a borrow.
    pub fn get_stage_output_schema(&self, process_id: &ProcessId, stage_name: &str) -> Option<serde_json::Value> {
        self.pipelines.get(process_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
                    .and_then(|s| s.output_schema.clone())
            })
    }

    /// Get the allowed_tools for a specific stage in a process's pipeline.
    pub fn get_stage_allowed_tools(&self, process_id: &ProcessId, stage_name: &str) -> Option<Vec<String>> {
        self.pipelines.get(process_id)
            .and_then(|session| {
                session.pipeline_config.stages.iter()
                    .find(|s| s.name == stage_name)
                    .and_then(|s| s.allowed_tools.clone())
            })
    }

    /// Get the state_schema for a process's pipeline.
    pub fn get_state_schema(&self, process_id: &ProcessId) -> Option<&Vec<StateField>> {
        self.pipelines.get(process_id)
            .map(|session| &session.pipeline_config.state_schema)
    }

    /// Get the publish event types for a process's pipeline.
    pub fn get_pipeline_publishes(&self, process_id: &ProcessId) -> Option<&[String]> {
        self.pipelines.get(process_id)
            .map(|session| session.pipeline_config.publishes.as_slice())
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
