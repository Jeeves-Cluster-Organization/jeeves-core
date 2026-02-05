//! Pipeline orchestration - kernel-driven pipeline execution control.
//!
//! The Orchestrator:
//!   - Owns the orchestration loop (moved from Python)
//!   - Evaluates routing rules
//!   - Tracks edge traversals
//!   - Enforces bounds (iterations, LLM calls, agent hops)
//!   - Returns Instructions to Python workers
//!
//! Python workers:
//!   - Execute agents (LLM calls, tool execution)
//!   - Report results back
//!   - Have NO control over what runs next

use crate::envelope::{Envelope, FlowInterrupt, TerminalReason};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Instruction Types
// =============================================================================

/// InstructionKind indicates what the Python worker should do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionKind {
    /// Execute specified agent
    RunAgent,
    /// End execution
    Terminate,
    /// Wait for interrupt resolution
    WaitInterrupt,
}

/// Instruction tells Python worker what to do next.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    pub kind: InstructionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_config: Option<serde_json::Value>, // Placeholder for AgentConfig
    #[serde(skip_serializing_if = "Option::is_none")]
    pub envelope: Option<Envelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<TerminalReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub termination_message: Option<String>,
    pub interrupt_pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<FlowInterrupt>,
}

/// AgentExecutionMetrics contains metrics from agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionMetrics {
    pub llm_calls: i32,
    pub tool_calls: i32,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub duration_ms: i64,
}

// =============================================================================
// Pipeline Config (Simplified)
// =============================================================================

/// Simplified PipelineConfig for orchestration.
/// Full implementation would come from config module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub name: String,
    pub agents: Vec<PipelineStage>,
    pub max_iterations: i32,
    pub max_llm_calls: i32,
    pub max_agent_hops: i32,
}

impl PipelineConfig {
    /// Get stage order from pipeline.
    pub fn get_stage_order(&self) -> Vec<String> {
        self.agents.iter().map(|s| s.name.clone()).collect()
    }

    /// Validate pipeline configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Pipeline name is required".to_string());
        }
        if self.agents.is_empty() {
            return Err("Pipeline must have at least one stage".to_string());
        }
        Ok(())
    }
}

/// Simplified PipelineStage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub name: String,
    pub agent: String,
    pub routing: Vec<RoutingRule>,
}

/// Simplified RoutingRule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub condition: String, // Simplified - full impl would parse this
    pub target: String,
    pub priority: i32,
}

// =============================================================================
// Orchestration Session
// =============================================================================

/// OrchestrationSession represents an active pipeline execution session.
#[derive(Debug, Clone)]
pub struct OrchestrationSession {
    pub process_id: String,
    pub pipeline_config: PipelineConfig,
    pub envelope: Envelope,
    pub edge_traversals: HashMap<String, i32>, // "from->to" -> count
    pub terminated: bool,
    pub terminal_reason: Option<TerminalReason>,
    pub created_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
}

/// SessionState is the external representation of session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub process_id: String,
    pub current_stage: String,
    pub stage_order: Vec<String>,
    pub envelope: HashMap<String, serde_json::Value>, // Simplified envelope representation
    pub edge_traversals: HashMap<String, i32>,
    pub terminated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<TerminalReason>,
}

// =============================================================================
// Orchestrator
// =============================================================================

/// Orchestrator manages kernel-side pipeline execution.
#[derive(Debug)]
pub struct Orchestrator {
    sessions: HashMap<String, OrchestrationSession>,
}

impl Orchestrator {
    /// Create a new Orchestrator.
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    // =============================================================================
    // Session Management
    // =============================================================================

    /// Initialize a new orchestration session.
    /// If force is false and a session already exists, returns an error.
    /// If force is true, replaces any existing session.
    pub fn initialize_session(
        &mut self,
        process_id: String,
        pipeline_config: PipelineConfig,
        mut envelope: Envelope,
        force: bool,
    ) -> Result<SessionState, String> {
        // Check for duplicate processID
        if self.sessions.contains_key(&process_id) && !force {
            return Err(format!(
                "Session already exists for process: {} (use force=true to replace)",
                process_id
            ));
        }

        // Validate pipeline config
        pipeline_config.validate()?;

        // Initialize envelope with pipeline bounds
        envelope.pipeline.max_iterations = pipeline_config.max_iterations;
        envelope.bounds.max_llm_calls = pipeline_config.max_llm_calls;
        envelope.bounds.max_agent_hops = pipeline_config.max_agent_hops;
        envelope.pipeline.stage_order = pipeline_config.get_stage_order();

        // Set initial stage if not set
        if envelope.pipeline.current_stage.is_empty() && !envelope.pipeline.stage_order.is_empty() {
            envelope.pipeline.current_stage = envelope.pipeline.stage_order[0].clone();
        }

        let now = Utc::now();
        let session = OrchestrationSession {
            process_id: process_id.clone(),
            pipeline_config,
            envelope,
            edge_traversals: HashMap::new(),
            terminated: false,
            terminal_reason: None,
            created_at: now,
            last_activity_at: now,
        };

        let state = self.build_session_state(&session);
        self.sessions.insert(process_id, session);

        Ok(state)
    }

    /// Get the next instruction for a process.
    pub fn get_next_instruction(&mut self, process_id: &str) -> Result<Instruction, String> {
        let session = self
            .sessions
            .get_mut(process_id)
            .ok_or_else(|| format!("Unknown process: {}", process_id))?;

        // Update activity timestamp
        session.last_activity_at = Utc::now();

        // Check if already terminated
        if session.terminated {
            return Ok(Instruction {
                kind: InstructionKind::Terminate,
                agent_name: None,
                agent_config: None,
                envelope: Some(session.envelope.clone()),
                terminal_reason: session.terminal_reason,
                termination_message: Some("Session already terminated".to_string()),
                interrupt_pending: false,
                interrupt: None,
            });
        }

        // Check for pending interrupt
        if session.envelope.interrupts.interrupt_pending {
            return Ok(Instruction {
                kind: InstructionKind::WaitInterrupt,
                agent_name: None,
                agent_config: None,
                envelope: Some(session.envelope.clone()),
                terminal_reason: None,
                termination_message: None,
                interrupt_pending: true,
                interrupt: session.envelope.interrupts.interrupt.clone(),
            });
        }

        // Check bounds
        if let Some(reason) = check_bounds(&session.envelope) {
            session.terminated = true;
            session.terminal_reason = Some(reason);
            return Ok(Instruction {
                kind: InstructionKind::Terminate,
                agent_name: None,
                agent_config: None,
                envelope: Some(session.envelope.clone()),
                terminal_reason: Some(reason),
                termination_message: Some(format!("Bounds exceeded: {:?}", reason)),
                interrupt_pending: false,
                interrupt: None,
            });
        }

        // Determine next agent to run
        let current_stage = &session.envelope.pipeline.current_stage;
        if current_stage.is_empty() {
            return Err("No current stage set".to_string());
        }

        let agent_name = get_agent_for_stage(&session.pipeline_config, current_stage)?;

        Ok(Instruction {
            kind: InstructionKind::RunAgent,
            agent_name: Some(agent_name),
            agent_config: None, // Placeholder
            envelope: Some(session.envelope.clone()),
            terminal_reason: None,
            termination_message: None,
            interrupt_pending: false,
            interrupt: None,
        })
    }

    /// Process agent execution result.
    pub fn report_agent_result(
        &mut self,
        process_id: &str,
        metrics: AgentExecutionMetrics,
        updated_envelope: Envelope,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(process_id)
            .ok_or_else(|| format!("Unknown process: {}", process_id))?;

        // Update envelope
        session.envelope = updated_envelope;

        // Update metrics in envelope
        session.envelope.bounds.llm_call_count += metrics.llm_calls;
        session.envelope.bounds.tool_call_count += metrics.tool_calls;
        session.envelope.bounds.tokens_in += metrics.tokens_in;
        session.envelope.bounds.tokens_out += metrics.tokens_out;

        // Increment iteration
        session.envelope.pipeline.iteration += 1;

        // ===== STAGE ADVANCEMENT LOGIC =====
        // Find current stage index in stage_order
        let stage_order = session.envelope.pipeline.stage_order.clone();
        let current_stage_idx = stage_order
            .iter()
            .position(|s| s == &session.envelope.pipeline.current_stage);

        match current_stage_idx {
            Some(idx) => {
                // Clone current_stage to avoid borrow checker issues
                let current_stage = session.envelope.pipeline.current_stage.clone();

                // Mark current stage as completed
                session.envelope.complete_stage(&current_stage);

                // Check if there's a next stage
                if idx + 1 < stage_order.len() {
                    // Advance to next stage
                    session.envelope.pipeline.current_stage = stage_order[idx + 1].clone();

                    // Increment agent_hop_count to track stage progression
                    session.envelope.bounds.agent_hop_count += 1;

                    // Update activity timestamp
                    session.last_activity_at = Utc::now();
                } else {
                    // Last stage completed - mark pipeline as complete
                    session.terminated = true;
                    session.terminal_reason = Some(TerminalReason::Completed);
                    session.envelope.bounds.terminated = true;
                    session.envelope.bounds.terminal_reason = Some(TerminalReason::Completed);

                    // Update activity timestamp
                    session.last_activity_at = Utc::now();
                }
            }
            None => {
                // Current stage not found in stage_order - error
                return Err(format!(
                    "Current stage '{}' not found in stage_order: {:?}",
                    session.envelope.pipeline.current_stage, stage_order
                ));
            }
        }

        Ok(())
    }

    /// Get session state for external queries.
    pub fn get_session_state(&self, process_id: &str) -> Result<SessionState, String> {
        let session = self
            .sessions
            .get(process_id)
            .ok_or_else(|| format!("Unknown process: {}", process_id))?;

        Ok(self.build_session_state(session))
    }

    /// Cleanup a session.
    pub fn cleanup_session(&mut self, process_id: &str) -> bool {
        self.sessions.remove(process_id).is_some()
    }

    /// Get session count.
    pub fn get_session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Cleanup stale sessions older than the given duration.
    pub fn cleanup_stale_sessions(&mut self, max_age_seconds: i64) -> usize {
        let cutoff = Utc::now() - chrono::Duration::seconds(max_age_seconds);
        let mut to_remove = Vec::new();

        for (pid, session) in &self.sessions {
            if session.last_activity_at < cutoff {
                to_remove.push(pid.clone());
            }
        }

        let count = to_remove.len();
        for pid in to_remove {
            self.sessions.remove(&pid);
        }

        count
    }

    /// Get the current envelope for a process.
    /// Returns None if the process does not exist.
    pub fn get_envelope_for_process(&self, process_id: &str) -> Option<&Envelope> {
        self.sessions
            .get(process_id)
            .map(|session| &session.envelope)
    }

    // =============================================================================
    // Internal Helpers
    // =============================================================================

    /// Build external session state representation.
    fn build_session_state(&self, session: &OrchestrationSession) -> SessionState {
        SessionState {
            process_id: session.process_id.clone(),
            current_stage: session.envelope.pipeline.current_stage.clone(),
            stage_order: session.envelope.pipeline.stage_order.clone(),
            envelope: HashMap::new(), // Simplified - full impl would serialize envelope
            edge_traversals: session.edge_traversals.clone(),
            terminated: session.terminated,
            terminal_reason: session.terminal_reason,
        }
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Helper Functions (outside impl to avoid borrow checker issues)
// =============================================================================

/// Check if envelope exceeds bounds.
fn check_bounds(envelope: &Envelope) -> Option<TerminalReason> {
    if envelope.bounds.llm_call_count >= envelope.bounds.max_llm_calls {
        return Some(TerminalReason::MaxLlmCallsExceeded);
    }
    if envelope.pipeline.iteration >= envelope.pipeline.max_iterations {
        return Some(TerminalReason::MaxIterationsExceeded);
    }
    if envelope.bounds.agent_hop_count >= envelope.bounds.max_agent_hops {
        return Some(TerminalReason::MaxAgentHopsExceeded);
    }
    None
}

/// Get agent name for a stage.
fn get_agent_for_stage(
    pipeline_config: &PipelineConfig,
    stage_name: &str,
) -> Result<String, String> {
    pipeline_config
        .agents
        .iter()
        .find(|s| s.name == stage_name)
        .map(|s| s.agent.clone())
        .ok_or_else(|| format!("Stage not found in pipeline: {}", stage_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_pipeline() -> PipelineConfig {
        PipelineConfig {
            name: "test_pipeline".to_string(),
            agents: vec![
                PipelineStage {
                    name: "stage1".to_string(),
                    agent: "agent1".to_string(),
                    routing: vec![],
                },
                PipelineStage {
                    name: "stage2".to_string(),
                    agent: "agent2".to_string(),
                    routing: vec![],
                },
            ],
            max_iterations: 10,
            max_llm_calls: 50,
            max_agent_hops: 5,
        }
    }

    fn create_test_envelope() -> Envelope {
        let mut env = Envelope::new();
        env.pipeline.current_stage = String::new(); // Clear so initialize_session sets it
        env
    }

    #[test]
    fn test_initialize_session() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        let state = orch
            .initialize_session("proc1".to_string(), pipeline.clone(), envelope, false)
            .unwrap();

        assert_eq!(state.process_id, "proc1");
        assert_eq!(state.current_stage, "stage1"); // First stage
        assert_eq!(state.stage_order, vec!["stage1", "stage2"]);
        assert!(!state.terminated);
    }

    #[test]
    fn test_initialize_session_duplicate_fails() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        // Initialize first time
        orch.initialize_session("proc1".to_string(), pipeline.clone(), envelope.clone(), false)
            .unwrap();

        // Try to initialize again without force
        let result =
            orch.initialize_session("proc1".to_string(), pipeline, envelope, false);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn test_initialize_session_force_replaces() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        // Initialize first time
        orch.initialize_session("proc1".to_string(), pipeline.clone(), envelope.clone(), false)
            .unwrap();

        // Initialize again with force=true should succeed
        let result =
            orch.initialize_session("proc1".to_string(), pipeline, envelope, true);

        assert!(result.is_ok());
    }

    #[test]
    fn test_get_next_instruction_run_agent() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        orch.initialize_session("proc1".to_string(), pipeline, envelope, false)
            .unwrap();

        let instruction = orch.get_next_instruction("proc1").unwrap();

        assert_eq!(instruction.kind, InstructionKind::RunAgent);
        assert_eq!(instruction.agent_name, Some("agent1".to_string()));
        assert!(!instruction.interrupt_pending);
    }

    #[test]
    fn test_get_next_instruction_terminate_after_terminated() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        orch.initialize_session("proc1".to_string(), pipeline, envelope, false)
            .unwrap();

        // Manually terminate session
        let session = orch.sessions.get_mut("proc1").unwrap();
        session.terminated = true;
        session.terminal_reason = Some(TerminalReason::MaxIterationsExceeded);

        let instruction = orch.get_next_instruction("proc1").unwrap();

        assert_eq!(instruction.kind, InstructionKind::Terminate);
        assert_eq!(
            instruction.terminal_reason,
            Some(TerminalReason::MaxIterationsExceeded)
        );
    }

    #[test]
    fn test_get_next_instruction_wait_interrupt() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let mut envelope = create_test_envelope();

        // Set interrupt pending
        envelope.interrupts.interrupt_pending = true;
        envelope.interrupts.interrupt = Some(
            FlowInterrupt::new(crate::envelope::InterruptKind::Clarification)
                .with_message("Need clarification".to_string())
        );

        orch.initialize_session("proc1".to_string(), pipeline, envelope, false)
            .unwrap();

        let instruction = orch.get_next_instruction("proc1").unwrap();

        assert_eq!(instruction.kind, InstructionKind::WaitInterrupt);
        assert!(instruction.interrupt_pending);
        assert!(instruction.interrupt.is_some());
    }

    #[test]
    fn test_check_bounds_llm_exceeded() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        orch.initialize_session("proc1".to_string(), pipeline, envelope, false)
            .unwrap();

        // Manually exceed LLM calls
        let session = orch.sessions.get_mut("proc1").unwrap();
        session.envelope.bounds.llm_call_count = 50; // At limit
        session.envelope.bounds.max_llm_calls = 50;

        let instruction = orch.get_next_instruction("proc1").unwrap();

        assert_eq!(instruction.kind, InstructionKind::Terminate);
        assert_eq!(
            instruction.terminal_reason,
            Some(TerminalReason::MaxLlmCallsExceeded)
        );
    }

    #[test]
    fn test_check_bounds_iterations_exceeded() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        orch.initialize_session("proc1".to_string(), pipeline, envelope, false)
            .unwrap();

        // Manually exceed iterations
        let session = orch.sessions.get_mut("proc1").unwrap();
        session.envelope.pipeline.iteration = 10; // At limit
        session.envelope.pipeline.max_iterations = 10;

        let instruction = orch.get_next_instruction("proc1").unwrap();

        assert_eq!(instruction.kind, InstructionKind::Terminate);
        assert_eq!(
            instruction.terminal_reason,
            Some(TerminalReason::MaxIterationsExceeded)
        );
    }

    #[test]
    fn test_report_agent_result_updates_metrics() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        orch.initialize_session("proc1".to_string(), pipeline, envelope, false)
            .unwrap();

        let metrics = AgentExecutionMetrics {
            llm_calls: 2,
            tool_calls: 5,
            tokens_in: 1000,
            tokens_out: 500,
            duration_ms: 1500,
        };

        // Use the initialized envelope (with stage_order set) for the report
        let initialized_envelope = orch.sessions.get("proc1").unwrap().envelope.clone();
        orch.report_agent_result("proc1", metrics, initialized_envelope)
            .unwrap();

        // Check metrics were updated
        let session = orch.sessions.get("proc1").unwrap();
        assert_eq!(session.envelope.bounds.llm_call_count, 2);
        assert_eq!(session.envelope.bounds.tool_call_count, 5);
        assert_eq!(session.envelope.bounds.tokens_in, 1000);
        assert_eq!(session.envelope.bounds.tokens_out, 500);
        assert_eq!(session.envelope.pipeline.iteration, 1); // Incremented
    }

    #[test]
    fn test_get_session_state() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        orch.initialize_session("proc1".to_string(), pipeline, envelope, false)
            .unwrap();

        let state = orch.get_session_state("proc1").unwrap();

        assert_eq!(state.process_id, "proc1");
        assert_eq!(state.current_stage, "stage1");
        assert!(!state.terminated);
    }

    #[test]
    fn test_cleanup_session() {
        let mut orch = Orchestrator::new();
        let pipeline = create_test_pipeline();
        let envelope = create_test_envelope();

        orch.initialize_session("proc1".to_string(), pipeline, envelope, false)
            .unwrap();

        assert_eq!(orch.get_session_count(), 1);

        let removed = orch.cleanup_session("proc1");
        assert!(removed);
        assert_eq!(orch.get_session_count(), 0);

        // Second cleanup should return false
        let removed = orch.cleanup_session("proc1");
        assert!(!removed);
    }

    #[test]
    fn test_get_session_state_not_found() {
        let orch = Orchestrator::new();

        let result = orch.get_session_state("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown process"));
    }

    #[test]
    fn test_pipeline_validation() {
        let mut pipeline = create_test_pipeline();

        // Valid pipeline
        assert!(pipeline.validate().is_ok());

        // Empty name
        pipeline.name = String::new();
        assert!(pipeline.validate().is_err());

        // Empty stages
        let mut pipeline2 = create_test_pipeline();
        pipeline2.agents = vec![];
        assert!(pipeline2.validate().is_err());
    }
}
