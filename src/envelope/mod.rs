//! Envelope - the core state container.
//!
//! The Envelope represents the mutable state of a request as it flows through
//! the multi-agent pipeline. It tracks inputs, outputs, bounds, and interrupts.
//!
//! Fields are organized into semantic sub-structs:
//! - **Identity**: envelope/request/user/session IDs
//! - **Pipeline**: stage sequencing and parallel execution
//! - **Bounds**: resource limits and counters
//! - **InterruptState**: human-in-the-loop flow control
//! - **Execution**: multi-stage goal tracking
//! - **Audit**: processing history, errors, timing, metadata

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::types::{EnvelopeId, RequestId, SessionId, UserId};

pub mod enums;

pub use enums::*;

/// Response to a flow interrupt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterruptResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, serde_json::Value>>,

    pub received_at: DateTime<Utc>,
}

/// Flow interrupt (clarification, confirmation, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowInterrupt {
    pub kind: InterruptKind,
    pub id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<InterruptResponse>,

    pub created_at: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl FlowInterrupt {
    pub fn new(kind: InterruptKind) -> Self {
        Self {
            kind,
            id: format!("int_{}", &uuid::Uuid::new_v4().simple().to_string()[..16]),
            question: None,
            message: None,
            data: None,
            response: None,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    pub fn with_question(mut self, q: String) -> Self {
        self.question = Some(q);
        self
    }

    pub fn with_message(mut self, m: String) -> Self {
        self.message = Some(m);
        self
    }

    pub fn with_data(mut self, d: HashMap<String, serde_json::Value>) -> Self {
        self.data = Some(d);
        self
    }

    pub fn with_expiry(mut self, duration: std::time::Duration) -> Self {
        self.expires_at = Some(Utc::now() + chrono::Duration::from_std(duration).unwrap_or_default());
        self
    }
}

/// Status of a processing record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProcessingStatus {
    Running,
    Success,
    Error,
    Skipped,
}

/// Processing record for audit trail.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessingRecord {
    pub agent: String,
    pub stage_order: i32,
    pub started_at: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    pub duration_ms: i32,
    pub status: ProcessingStatus,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    pub llm_calls: i32,
}

// =============================================================================
// Sub-structs
// =============================================================================

/// Envelope identity fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Identity {
    pub envelope_id: EnvelopeId,
    pub request_id: RequestId,
    pub user_id: UserId,
    pub session_id: SessionId,
}

/// Pipeline sequencing and parallel execution state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pipeline {
    pub current_stage: String,
    pub stage_order: Vec<String>,
    pub iteration: i32,
    pub max_iterations: i32,

    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub active_stages: HashSet<String>,

    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub completed_stage_set: HashSet<String>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub failed_stages: HashMap<String, String>,

    #[serde(default)]
    pub parallel_mode: bool,
}

/// Resource limits and usage counters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bounds {
    pub llm_call_count: i32,
    pub max_llm_calls: i32,
    pub tool_call_count: i32,
    pub agent_hop_count: i32,
    pub max_agent_hops: i32,
    pub tokens_in: i64,
    pub tokens_out: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<TerminalReason>,

    pub terminated: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub termination_reason: Option<String>,
}

/// Human-in-the-loop interrupt state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterruptState {
    pub interrupt_pending: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<FlowInterrupt>,
}

/// Multi-stage execution tracking (goals, retries).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Execution {
    pub completed_stages: Vec<HashMap<String, serde_json::Value>>,
    pub current_stage_number: i32,
    pub max_stages: i32,
    pub all_goals: Vec<String>,
    pub remaining_goals: Vec<String>,
    pub goal_completion_status: HashMap<String, String>,
    pub prior_plans: Vec<HashMap<String, serde_json::Value>>,
    pub loop_feedback: Vec<String>,
}

/// Audit trail: history, errors, timing, metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Audit {
    pub processing_history: Vec<ProcessingRecord>,
    pub errors: Vec<HashMap<String, serde_json::Value>>,
    pub created_at: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    pub metadata: HashMap<String, serde_json::Value>,
}

// =============================================================================
// Envelope
// =============================================================================

/// Main envelope structure.
///
/// Unlike hardcoded per-agent output fields, Envelope uses a dynamic `Outputs` map
/// where any agent can write results keyed by agent name.
///
/// Example (pseudo-code):
/// ```ignore
/// envelope.outputs.insert("perception", {...});
/// envelope.outputs.insert("intent", {...});
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    pub identity: Identity,
    pub raw_input: String,
    pub received_at: DateTime<Utc>,

    /// Outputs from agents: map[agent_name] = map[output_key] = value
    pub outputs: HashMap<String, HashMap<String, serde_json::Value>>,

    pub pipeline: Pipeline,
    pub bounds: Bounds,
    pub interrupts: InterruptState,
    pub execution: Execution,
    pub audit: Audit,
}

impl Envelope {
    /// Create a new envelope with default values (matches Go's NewEnvelope).
    pub fn new() -> Self {
        let now = Utc::now();
        let uuid_short = || uuid::Uuid::new_v4().simple().to_string()[..16].to_string();

        Self {
            identity: Identity {
                envelope_id: EnvelopeId::must(format!("env_{}", uuid_short())),
                request_id: RequestId::must(format!("req_{}", uuid_short())),
                user_id: UserId::must("anonymous"),
                session_id: SessionId::must(format!("sess_{}", uuid_short())),
            },

            raw_input: String::new(),
            received_at: now,
            outputs: HashMap::new(),

            pipeline: Pipeline {
                current_stage: "start".to_string(),
                stage_order: Vec::new(),
                iteration: 0,
                max_iterations: 3,
                active_stages: HashSet::new(),
                completed_stage_set: HashSet::new(),
                failed_stages: HashMap::new(),
                parallel_mode: false,
            },

            bounds: Bounds {
                llm_call_count: 0,
                max_llm_calls: 10,
                tool_call_count: 0,
                agent_hop_count: 0,
                max_agent_hops: 21,
                tokens_in: 0,
                tokens_out: 0,
                terminal_reason: None,
                terminated: false,
                termination_reason: None,
            },

            interrupts: InterruptState {
                interrupt_pending: false,
                interrupt: None,
            },

            execution: Execution {
                completed_stages: Vec::new(),
                current_stage_number: 1,
                max_stages: 5,
                all_goals: Vec::new(),
                remaining_goals: Vec::new(),
                goal_completion_status: HashMap::new(),
                prior_plans: Vec::new(),
                loop_feedback: Vec::new(),
            },

            audit: Audit {
                processing_history: Vec::new(),
                errors: Vec::new(),
                created_at: now,
                completed_at: None,
                metadata: HashMap::new(),
            },
        }
    }

    /// Start a stage (mark as actively executing).
    pub fn start_stage(&mut self, stage_name: impl Into<String>) {
        self.pipeline.active_stages.insert(stage_name.into());
    }

    /// Complete a stage successfully.
    pub fn complete_stage(&mut self, stage_name: &str) {
        self.pipeline.completed_stage_set.insert(stage_name.to_string());
        self.pipeline.active_stages.remove(stage_name);
    }

    /// Mark a stage as failed.
    pub fn fail_stage(&mut self, stage_name: impl Into<String>, error_msg: impl Into<String>) {
        let stage_name_str = stage_name.into();
        self.pipeline.failed_stages.insert(stage_name_str.clone(), error_msg.into());
        self.pipeline.active_stages.remove(&stage_name_str);
    }

    /// Check if a stage is completed.
    pub fn is_stage_completed(&self, stage_name: &str) -> bool {
        self.pipeline.completed_stage_set.contains(stage_name)
    }

    /// Check if a stage failed.
    pub fn is_stage_failed(&self, stage_name: &str) -> bool {
        self.pipeline.failed_stages.contains_key(stage_name)
    }

    /// Check if at any bound limit.
    pub fn at_limit(&self) -> bool {
        self.bounds.llm_call_count >= self.bounds.max_llm_calls
            || self.bounds.agent_hop_count >= self.bounds.max_agent_hops
    }

    /// Increment LLM call counter.
    pub fn increment_llm_calls(&mut self, count: i32) {
        self.bounds.llm_call_count += count;
    }

    /// Increment agent hop counter.
    pub fn increment_agent_hops(&mut self) {
        self.bounds.agent_hop_count += 1;
    }

    /// Add processing record to history.
    pub fn add_processing_record(&mut self, record: ProcessingRecord) {
        self.audit.processing_history.push(record);
    }

    /// Terminate envelope with reason.
    pub fn terminate(&mut self, reason: impl Into<String>) {
        self.bounds.terminated = true;
        self.bounds.termination_reason = Some(reason.into());
        self.audit.completed_at = Some(Utc::now());
    }

    /// Set interrupt pending.
    pub fn set_interrupt(&mut self, interrupt: FlowInterrupt) {
        self.interrupts.interrupt_pending = true;
        self.interrupts.interrupt = Some(interrupt);
    }

    /// Clear interrupt.
    pub fn clear_interrupt(&mut self) {
        self.interrupts.interrupt_pending = false;
        self.interrupts.interrupt = None;
    }

    /// Merge key-value updates into the envelope.
    ///
    /// Supports updating well-known fields: `raw_input`, `metadata` (merged into
    /// `audit.metadata`), and `outputs` (merged into `outputs`). Unknown keys
    /// are stored in `audit.metadata` as a catch-all.
    pub fn merge_updates(&mut self, updates: HashMap<String, serde_json::Value>) {
        for (key, value) in updates {
            match key.as_str() {
                "raw_input" => {
                    if let Some(s) = value.as_str() {
                        self.raw_input = s.to_string();
                    }
                }
                "metadata" => {
                    if let serde_json::Value::Object(map) = value {
                        for (k, v) in map {
                            self.audit.metadata.insert(k, v);
                        }
                    }
                }
                "outputs" => {
                    if let Ok(output_map) = serde_json::from_value::<HashMap<String, HashMap<String, serde_json::Value>>>(value) {
                        for (agent, output) in output_map {
                            self.outputs.entry(agent).or_default().extend(output);
                        }
                    }
                }
                _ => {
                    // Store unknown keys in audit metadata
                    self.audit.metadata.insert(key, value);
                }
            }
        }
    }
}

impl Default for Envelope {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1. new() defaults ───────────────────────────────────────────────

    #[test]
    fn test_new_envelope_defaults() {
        let env = Envelope::new();

        // Identity: generated UUIDs are prefixed
        assert!(env.identity.envelope_id.as_str().starts_with("env_"));
        assert!(env.identity.request_id.as_str().starts_with("req_"));
        assert_eq!(env.identity.user_id.as_str(), "anonymous");
        assert!(env.identity.session_id.as_str().starts_with("sess_"));

        // Raw input is empty
        assert!(env.raw_input.is_empty());

        // Outputs map is empty
        assert!(env.outputs.is_empty());

        // Pipeline defaults
        assert_eq!(env.pipeline.current_stage, "start");
        assert!(env.pipeline.stage_order.is_empty());
        assert_eq!(env.pipeline.iteration, 0);
        assert_eq!(env.pipeline.max_iterations, 3);
        assert!(env.pipeline.active_stages.is_empty());
        assert!(env.pipeline.completed_stage_set.is_empty());
        assert!(env.pipeline.failed_stages.is_empty());
        assert!(!env.pipeline.parallel_mode);

        // Bounds defaults
        assert_eq!(env.bounds.llm_call_count, 0);
        assert_eq!(env.bounds.max_llm_calls, 10);
        assert_eq!(env.bounds.tool_call_count, 0);
        assert_eq!(env.bounds.agent_hop_count, 0);
        assert_eq!(env.bounds.max_agent_hops, 21);
        assert_eq!(env.bounds.tokens_in, 0);
        assert_eq!(env.bounds.tokens_out, 0);
        assert!(env.bounds.terminal_reason.is_none());
        assert!(!env.bounds.terminated);
        assert!(env.bounds.termination_reason.is_none());

        // Interrupts defaults
        assert!(!env.interrupts.interrupt_pending);
        assert!(env.interrupts.interrupt.is_none());

        // Execution defaults
        assert!(env.execution.completed_stages.is_empty());
        assert_eq!(env.execution.current_stage_number, 1);
        assert_eq!(env.execution.max_stages, 5);
        assert!(env.execution.all_goals.is_empty());
        assert!(env.execution.remaining_goals.is_empty());
        assert!(env.execution.goal_completion_status.is_empty());
        assert!(env.execution.prior_plans.is_empty());
        assert!(env.execution.loop_feedback.is_empty());

        // Audit defaults
        assert!(env.audit.processing_history.is_empty());
        assert!(env.audit.errors.is_empty());
        assert!(env.audit.completed_at.is_none());
        assert!(env.audit.metadata.is_empty());
    }

    // ── 2. Stage lifecycle ──────────────────────────────────────────────

    #[test]
    fn test_stage_lifecycle() {
        let mut env = Envelope::new();

        // Start a stage
        env.start_stage("perception");
        assert!(env.pipeline.active_stages.contains("perception"));
        assert!(!env.is_stage_completed("perception"));

        // Complete the stage
        env.complete_stage("perception");
        assert!(env.is_stage_completed("perception"));
        assert!(!env.pipeline.active_stages.contains("perception"));
    }

    // ── 3. Stage failure ────────────────────────────────────────────────

    #[test]
    fn test_stage_failure() {
        let mut env = Envelope::new();

        env.start_stage("planning");
        assert!(env.pipeline.active_stages.contains("planning"));

        env.fail_stage("planning", "timeout after 30s");
        assert!(env.is_stage_failed("planning"));
        assert!(!env.pipeline.active_stages.contains("planning"));
        assert_eq!(
            env.pipeline.failed_stages.get("planning").unwrap(),
            "timeout after 30s"
        );
        // A failed stage is NOT counted as completed
        assert!(!env.is_stage_completed("planning"));
    }

    // ── 4. at_limit: LLM calls ─────────────────────────────────────────

    #[test]
    fn test_at_limit_llm_calls() {
        let mut env = Envelope::new();
        assert!(!env.at_limit());

        // Increment to exactly the max (default 10)
        env.increment_llm_calls(10);
        assert_eq!(env.bounds.llm_call_count, 10);
        assert!(env.at_limit());
    }

    // ── 5. at_limit: agent hops ─────────────────────────────────────────

    #[test]
    fn test_at_limit_agent_hops() {
        let mut env = Envelope::new();
        assert!(!env.at_limit());

        // Increment agent hops to max (default 21)
        for _ in 0..21 {
            env.increment_agent_hops();
        }
        assert_eq!(env.bounds.agent_hop_count, 21);
        assert!(env.at_limit());
    }

    // ── 6. not at limit when below max ──────────────────────────────────

    #[test]
    fn test_not_at_limit_below_max() {
        let mut env = Envelope::new();
        env.increment_llm_calls(5);
        for _ in 0..10 {
            env.increment_agent_hops();
        }
        assert_eq!(env.bounds.llm_call_count, 5);
        assert_eq!(env.bounds.agent_hop_count, 10);
        assert!(!env.at_limit());
    }

    // ── 7. terminate ────────────────────────────────────────────────────

    #[test]
    fn test_terminate() {
        let mut env = Envelope::new();

        assert!(!env.bounds.terminated);
        assert!(env.bounds.termination_reason.is_none());
        assert!(env.audit.completed_at.is_none());

        env.terminate("user cancelled the request");

        assert!(env.bounds.terminated);
        assert_eq!(
            env.bounds.termination_reason.as_deref(),
            Some("user cancelled the request")
        );
        assert!(env.audit.completed_at.is_some());
    }

    // ── 8. interrupt flow ───────────────────────────────────────────────

    #[test]
    fn test_interrupt_flow() {
        let mut env = Envelope::new();

        assert!(!env.interrupts.interrupt_pending);
        assert!(env.interrupts.interrupt.is_none());

        // Set an interrupt
        let interrupt = FlowInterrupt::new(InterruptKind::Clarification)
            .with_question("Which database?".to_string());
        env.set_interrupt(interrupt);

        assert!(env.interrupts.interrupt_pending);
        assert!(env.interrupts.interrupt.is_some());
        let int = env.interrupts.interrupt.as_ref().unwrap();
        assert_eq!(int.kind, InterruptKind::Clarification);
        assert_eq!(int.question.as_deref(), Some("Which database?"));

        // Clear the interrupt
        env.clear_interrupt();
        assert!(!env.interrupts.interrupt_pending);
        assert!(env.interrupts.interrupt.is_none());
    }

    // ── 9. processing record ────────────────────────────────────────────

    #[test]
    fn test_processing_record() {
        let mut env = Envelope::new();
        assert!(env.audit.processing_history.is_empty());

        let record = ProcessingRecord {
            agent: "test-agent".to_string(),
            stage_order: 1,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            duration_ms: 100,
            status: ProcessingStatus::Success,
            error: None,
            llm_calls: 1,
        };

        env.add_processing_record(record.clone());
        assert_eq!(env.audit.processing_history.len(), 1);
        assert_eq!(env.audit.processing_history[0].agent, "test-agent");
        assert_eq!(env.audit.processing_history[0].status, ProcessingStatus::Success);
        assert_eq!(env.audit.processing_history[0].llm_calls, 1);
    }

    // ── 10. serde: TerminalReason ───────────────────────────────────────

    #[test]
    fn test_serde_terminal_reason() {
        // TerminalReason uses SCREAMING_SNAKE_CASE
        let cases = vec![
            (TerminalReason::Completed, "\"COMPLETED\""),
            (TerminalReason::MaxIterationsExceeded, "\"MAX_ITERATIONS_EXCEEDED\""),
            (TerminalReason::MaxLlmCallsExceeded, "\"MAX_LLM_CALLS_EXCEEDED\""),
            (TerminalReason::MaxAgentHopsExceeded, "\"MAX_AGENT_HOPS_EXCEEDED\""),
            (TerminalReason::UserCancelled, "\"USER_CANCELLED\""),
            (TerminalReason::ToolFailedFatally, "\"TOOL_FAILED_FATALLY\""),
            (TerminalReason::LlmFailedFatally, "\"LLM_FAILED_FATALLY\""),
            (TerminalReason::PolicyViolation, "\"POLICY_VIOLATION\""),
        ];

        for (variant, expected_json) in cases {
            let serialized = serde_json::to_string(&variant).unwrap();
            assert_eq!(serialized, expected_json, "serialize {:?}", variant);
            let deserialized: TerminalReason = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized, variant, "round-trip {:?}", variant);
        }
    }

    // ── 11. serde: InterruptKind ────────────────────────────────────────

    #[test]
    fn test_serde_interrupt_kind() {
        // InterruptKind uses snake_case
        let cases = vec![
            (InterruptKind::Clarification, "\"clarification\""),
            (InterruptKind::Confirmation, "\"confirmation\""),
            (InterruptKind::AgentReview, "\"agent_review\""),
            (InterruptKind::Checkpoint, "\"checkpoint\""),
            (InterruptKind::ResourceExhausted, "\"resource_exhausted\""),
            (InterruptKind::Timeout, "\"timeout\""),
            (InterruptKind::SystemError, "\"system_error\""),
        ];

        for (variant, expected_json) in cases {
            let serialized = serde_json::to_string(&variant).unwrap();
            assert_eq!(serialized, expected_json, "serialize {:?}", variant);
            let deserialized: InterruptKind = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized, variant, "round-trip {:?}", variant);
        }
    }

    // ── 12. serde: ProcessingStatus ─────────────────────────────────────

    #[test]
    fn test_serde_processing_status() {
        // ProcessingStatus uses lowercase
        let cases = vec![
            (ProcessingStatus::Running, "\"running\""),
            (ProcessingStatus::Success, "\"success\""),
            (ProcessingStatus::Error, "\"error\""),
            (ProcessingStatus::Skipped, "\"skipped\""),
        ];

        for (variant, expected_json) in cases {
            let serialized = serde_json::to_string(&variant).unwrap();
            assert_eq!(serialized, expected_json, "serialize {:?}", variant);
            let deserialized: ProcessingStatus = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized, variant, "round-trip {:?}", variant);
        }
    }

    // ── 13. Default == new ──────────────────────────────────────────────

    #[test]
    fn test_default_equals_new() {
        // Both produce the same structural defaults (UUIDs differ, but the
        // shape is identical).  We verify by checking every non-UUID field.
        let a = Envelope::new();
        let b = Envelope::default();

        // Pipeline
        assert_eq!(a.pipeline.current_stage, b.pipeline.current_stage);
        assert_eq!(a.pipeline.max_iterations, b.pipeline.max_iterations);
        assert_eq!(a.pipeline.parallel_mode, b.pipeline.parallel_mode);

        // Bounds
        assert_eq!(a.bounds.max_llm_calls, b.bounds.max_llm_calls);
        assert_eq!(a.bounds.max_agent_hops, b.bounds.max_agent_hops);
        assert_eq!(a.bounds.terminated, b.bounds.terminated);

        // Interrupts
        assert_eq!(a.interrupts.interrupt_pending, b.interrupts.interrupt_pending);

        // Execution
        assert_eq!(a.execution.current_stage_number, b.execution.current_stage_number);
        assert_eq!(a.execution.max_stages, b.execution.max_stages);

        // Identity shape (prefixes match even if UUIDs differ)
        assert!(b.identity.envelope_id.as_str().starts_with("env_"));
        assert!(b.identity.request_id.as_str().starts_with("req_"));
        assert_eq!(b.identity.user_id.as_str(), "anonymous");
        assert!(b.identity.session_id.as_str().starts_with("sess_"));
    }
}
