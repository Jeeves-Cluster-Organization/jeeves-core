//! Run: mutable state for a request flowing through the pipeline.
//! Fields group into `Identity`, `Pipeline`, `Bounds`, `InterruptState`, `Audit`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::{AgentName, EnvelopeId, OutputKey, RequestId, SessionId, UserId};

pub mod enums;
pub mod events;
pub mod types;

pub use enums::*;
pub use events::{AggregateMetrics, RunEvent, StageMetrics};
pub use types::*;

#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Run {
    pub identity: Identity,
    pub raw_input: String,
    pub received_at: DateTime<Utc>,

    /// `agent_name → output_key → value`. Any agent can write here.
    pub outputs: HashMap<AgentName, HashMap<OutputKey, serde_json::Value>>,

    /// Accumulator merged across loop-backs per `state_schema`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state: HashMap<String, serde_json::Value>,

    pub current_stage: String,
    pub stage_order: Vec<String>,
    pub iteration: i32,
    pub max_iterations: i32,

    pub limits: Limits,
    pub metrics: Metrics,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub termination: Option<Termination>,
    pub interrupts: InterruptState,
    pub audit: Audit,
}

impl Run {
    /// Anonymous Run with a generated identity. Test convenience.
    pub fn anonymous() -> Self {
        let uuid_short = || uuid::Uuid::new_v4().simple().to_string()[..16].to_string();
        Self::new(
            "anonymous",
            &format!("sess_{}", uuid_short()),
            "",
            None,
        )
    }

    /// Run instance with the given identity and inputs. Pipeline bounds are
    /// safe placeholders that `initialize_orchestration` overwrites from the
    /// `Workflow`.
    pub fn new(
        user_id: &str,
        session_id: &str,
        raw_input: &str,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        let now = Utc::now();
        let uuid_short = || uuid::Uuid::new_v4().simple().to_string()[..16].to_string();

        let mut audit_metadata = HashMap::new();
        if let Some(serde_json::Value::Object(map)) = metadata {
            for (k, v) in map {
                audit_metadata.insert(k, v);
            }
        }

        Self {
            identity: Identity {
                envelope_id: EnvelopeId::must(format!("env_{}", uuid_short())),
                request_id: RequestId::must(format!("req_{}", uuid_short())),
                user_id: UserId::must(user_id),
                session_id: SessionId::must(session_id),
            },
            raw_input: raw_input.to_string(),
            received_at: now,
            outputs: HashMap::new(),
            state: HashMap::new(),
            current_stage: String::new(),
            stage_order: Vec::new(),
            iteration: 0,
            max_iterations: 100,
            limits: Limits {
                max_llm_calls: 100,
                max_agent_hops: 100,
            },
            metrics: Metrics::default(),
            termination: None,
            interrupts: InterruptState {
                interrupt: None,
            },
            audit: Audit {
                processing_history: Vec::new(),
                created_at: now,
                completed_at: None,
                metadata: audit_metadata,
            },
        }
    }

    /// Returns the terminal reason if this Run has exceeded any bound.
    /// Called pre-flight in `get_next_instruction` and post-iteration in
    /// `report_agent_result`.
    pub fn check_bounds(&self) -> Option<TerminalReason> {
        if self.metrics.llm_calls >= self.limits.max_llm_calls {
            return Some(TerminalReason::MaxLlmCallsExceeded);
        }
        if self.iteration >= self.max_iterations {
            return Some(TerminalReason::MaxIterationsExceeded);
        }
        if self.metrics.agent_hops >= self.limits.max_agent_hops {
            return Some(TerminalReason::MaxAgentHopsExceeded);
        }
        None
    }

    pub fn at_limit(&self) -> bool {
        self.check_bounds().is_some()
    }

    pub fn is_terminated(&self) -> bool {
        self.termination.is_some()
    }

    pub fn terminal_reason(&self) -> Option<TerminalReason> {
        self.termination.as_ref().map(|t| t.reason)
    }

    pub fn terminate_with(&mut self, reason: TerminalReason, message: Option<String>) {
        self.termination = Some(Termination { reason, message });
    }

    pub fn add_processing_record(&mut self, record: ProcessingRecord) {
        self.audit.processing_history.push(record);
    }

    /// Terminate this Run as completed, recording `reason` as the message.
    pub fn complete(&mut self, reason: impl Into<String>) {
        self.terminate_with(TerminalReason::Completed, Some(reason.into()));
        self.audit.completed_at = Some(Utc::now());
    }

    /// Set interrupt pending.
    pub fn set_interrupt(&mut self, interrupt: FlowInterrupt) {
        self.interrupts.interrupt = Some(interrupt);
    }

    /// Clear interrupt.
    pub fn clear_interrupt(&mut self) {
        self.interrupts.interrupt = None;
    }

    /// Validate run invariants.
    ///
    /// Called after deserialization from external input to catch
    /// malformed state before it enters the kernel.
    pub fn validate(&self) -> crate::types::Result<()> {
        // Identity: must be non-empty
        if self.identity.envelope_id.as_str().is_empty() {
            return Err(crate::types::Error::validation("envelope_id must not be empty"));
        }
        if self.identity.request_id.as_str().is_empty() {
            return Err(crate::types::Error::validation("request_id must not be empty"));
        }
        if self.identity.user_id.as_str().is_empty() {
            return Err(crate::types::Error::validation("user_id must not be empty"));
        }
        if self.identity.session_id.as_str().is_empty() {
            return Err(crate::types::Error::validation("session_id must not be empty"));
        }

        // Bounds: non-negative counters
        if self.metrics.llm_calls < 0 {
            return Err(crate::types::Error::validation("bounds.llm_call_count must be non-negative"));
        }
        if self.metrics.tool_calls < 0 {
            return Err(crate::types::Error::validation("bounds.tool_call_count must be non-negative"));
        }
        if self.metrics.agent_hops < 0 {
            return Err(crate::types::Error::validation("bounds.agent_hop_count must be non-negative"));
        }

        // Bounds: positive maximums
        if self.limits.max_llm_calls <= 0 {
            return Err(crate::types::Error::validation("bounds.max_llm_calls must be positive"));
        }
        if self.limits.max_agent_hops <= 0 {
            return Err(crate::types::Error::validation("bounds.max_agent_hops must be positive"));
        }

        // Pipeline: current_stage must be in stage_order (if stage_order is populated)
        if !self.stage_order.is_empty()
            && !self.stage_order.contains(&self.current_stage)
        {
            return Err(crate::types::Error::validation(format!(
                "pipeline.current_stage '{}' not in stage_order",
                self.current_stage
            )));
        }

        // Pipeline: positive max_iterations
        if self.max_iterations <= 0 {
            return Err(crate::types::Error::validation("pipeline.max_iterations must be positive"));
        }

        Ok(())
    }

    /// Merge key-value updates into the run.
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
                    if let Ok(output_map) = serde_json::from_value::<HashMap<AgentName, HashMap<OutputKey, serde_json::Value>>>(value) {
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

impl Default for Run {
    fn default() -> Self {
        Self::anonymous()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1. new() defaults ───────────────────────────────────────────────

    #[test]
    fn test_new_envelope_defaults() {
        let env = Run::anonymous();

        assert!(env.identity.envelope_id.as_str().starts_with("env_"));
        assert!(env.identity.request_id.as_str().starts_with("req_"));
        assert_eq!(env.identity.user_id.as_str(), "anonymous");
        assert!(env.identity.session_id.as_str().starts_with("sess_"));

        assert!(env.raw_input.is_empty());
        assert!(env.outputs.is_empty());

        assert_eq!(env.current_stage, "");
        assert!(env.stage_order.is_empty());
        assert_eq!(env.iteration, 0);
        assert_eq!(env.max_iterations, 100);

        assert_eq!(env.metrics.llm_calls, 0);
        assert_eq!(env.limits.max_llm_calls, 100);
        assert_eq!(env.metrics.tool_calls, 0);
        assert_eq!(env.metrics.agent_hops, 0);
        assert_eq!(env.limits.max_agent_hops, 100);
        assert_eq!(env.metrics.tokens_in, 0);
        assert_eq!(env.metrics.tokens_out, 0);
        assert!(!env.is_terminated());

        assert!(!env.interrupts.is_pending());
        assert!(env.interrupts.interrupt.is_none());

        assert!(env.audit.processing_history.is_empty());
        assert!(env.audit.completed_at.is_none());
        assert!(env.audit.metadata.is_empty());
    }

    // ── 4. at_limit: LLM calls ─────────────────────────────────────────

    #[test]
    fn test_at_limit_llm_calls() {
        let mut env = Run::anonymous();
        env.limits.max_llm_calls = 10;
        assert!(!env.at_limit());

        env.metrics.llm_calls = 10;
        assert!(env.at_limit());
    }

    // ── 5. at_limit: agent hops ─────────────────────────────────────────

    #[test]
    fn test_at_limit_agent_hops() {
        let mut env = Run::anonymous();
        env.limits.max_agent_hops = 21;
        assert!(!env.at_limit());

        env.metrics.agent_hops = 21;
        assert!(env.at_limit());
    }

    // ── 5b. at_limit: iterations ─────────────────────────────────────────

    #[test]
    fn test_at_limit_iterations() {
        let mut env = Run::anonymous();
        env.max_iterations = 3;
        assert!(!env.at_limit());

        // Increment iterations to max
        env.iteration = 3;
        assert!(env.at_limit());

        // check_bounds returns the specific reason
        assert_eq!(env.check_bounds(), Some(TerminalReason::MaxIterationsExceeded));
    }

    // ── 6. not at limit when below max ──────────────────────────────────

    #[test]
    fn test_not_at_limit_below_max() {
        let mut env = Run::anonymous();
        env.metrics.llm_calls = 5;
        env.metrics.agent_hops = 10;
        assert!(!env.at_limit());
    }

    // ── 7. terminate ────────────────────────────────────────────────────

    #[test]
    fn test_terminate() {
        let mut env = Run::anonymous();

        assert!(!env.is_terminated());
        assert!(env.audit.completed_at.is_none());

        env.complete("user cancelled the request");

        assert!(env.is_terminated());
        assert_eq!(
            env.termination.as_ref().unwrap().message.as_deref(),
            Some("user cancelled the request")
        );
        assert!(env.audit.completed_at.is_some());
    }

    // ── 8. interrupt flow ───────────────────────────────────────────────

    #[test]
    fn test_interrupt_flow() {
        let mut env = Run::anonymous();

        assert!(!env.interrupts.is_pending());
        assert!(env.interrupts.interrupt.is_none());

        // Set an interrupt
        let interrupt = FlowInterrupt::new()
            .with_question("Which database?".to_string());
        env.set_interrupt(interrupt);

        assert!(env.interrupts.is_pending());
        assert!(env.interrupts.interrupt.is_some());
        let int = env.interrupts.interrupt.as_ref().unwrap();
        assert_eq!(int.question.as_deref(), Some("Which database?"));

        // Clear the interrupt
        env.clear_interrupt();
        assert!(!env.interrupts.is_pending());
        assert!(env.interrupts.interrupt.is_none());
    }

    // ── 9. processing record ────────────────────────────────────────────

    #[test]
    fn test_processing_record() {
        let mut env = Run::anonymous();
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
            tool_calls: 0,
            tokens_in: 0,
            tokens_out: 0,
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
        let cases = vec![
            (TerminalReason::Completed, "\"COMPLETED\""),
            (TerminalReason::MaxIterationsExceeded, "\"MAX_ITERATIONS_EXCEEDED\""),
            (TerminalReason::MaxLlmCallsExceeded, "\"MAX_LLM_CALLS_EXCEEDED\""),
            (TerminalReason::MaxAgentHopsExceeded, "\"MAX_AGENT_HOPS_EXCEEDED\""),
            (TerminalReason::UserCancelled, "\"USER_CANCELLED\""),
            (TerminalReason::ToolFailedFatally, "\"TOOL_FAILED_FATALLY\""),
            (TerminalReason::LlmFailedFatally, "\"LLM_FAILED_FATALLY\""),
            (TerminalReason::PolicyViolation, "\"POLICY_VIOLATION\""),
            (TerminalReason::BreakRequested, "\"BREAK_REQUESTED\""),
        ];

        for (variant, expected_json) in cases {
            let serialized = serde_json::to_string(&variant).unwrap();
            assert_eq!(serialized, expected_json, "serialize {:?}", variant);
            let deserialized: TerminalReason = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized, variant, "round-trip {:?}", variant);
        }
    }

    // ── 11. serde: ProcessingStatus ─────────────────────────────────────

    #[test]
    fn test_serde_processing_status() {
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
        let a = Run::anonymous();
        let b = Run::default();

        assert_eq!(a.current_stage, b.current_stage);
        assert_eq!(a.max_iterations, b.max_iterations);
        assert_eq!(a.iteration, b.iteration);

        assert_eq!(a.limits.max_llm_calls, b.limits.max_llm_calls);
        assert_eq!(a.limits.max_agent_hops, b.limits.max_agent_hops);
        assert_eq!(a.is_terminated(), b.is_terminated());

        assert_eq!(a.interrupts.is_pending(), b.interrupts.is_pending());

        // Identity shape (prefixes match even if UUIDs differ)
        assert!(b.identity.envelope_id.as_str().starts_with("env_"));
        assert!(b.identity.request_id.as_str().starts_with("req_"));
        assert_eq!(b.identity.user_id.as_str(), "anonymous");
        assert!(b.identity.session_id.as_str().starts_with("sess_"));
    }

    // ── 14. merge_updates: empty outputs ─────────────────────────────────

    #[test]
    fn test_merge_updates_empty_outputs() {
        let mut env = Run::anonymous();
        let mut agent_out: HashMap<OutputKey, serde_json::Value> = HashMap::new();
        agent_out.insert("key1".into(), serde_json::json!("value1"));
        env.outputs.insert("agent1".into(), agent_out);

        let updates = HashMap::new();
        env.merge_updates(updates);

        assert_eq!(env.outputs.len(), 1);
        assert_eq!(
            env.outputs.get("agent1").unwrap().get("key1").unwrap(),
            &serde_json::json!("value1")
        );
    }

    // ── 15. merge_updates: metadata merge ────────────────────────────────

    #[test]
    fn test_merge_updates_metadata_merge() {
        let mut env = Run::anonymous();
        env.audit.metadata.insert("existing_key".to_string(), serde_json::json!("original"));
        env.audit.metadata.insert("shared_key".to_string(), serde_json::json!("old_value"));

        let mut updates = HashMap::new();
        updates.insert("metadata".to_string(), serde_json::json!({
            "new_key": "new_value",
            "shared_key": "overwritten"
        }));
        env.merge_updates(updates);

        assert_eq!(
            env.audit.metadata.get("existing_key").unwrap(),
            &serde_json::json!("original")
        );
        assert_eq!(
            env.audit.metadata.get("new_key").unwrap(),
            &serde_json::json!("new_value")
        );
        assert_eq!(
            env.audit.metadata.get("shared_key").unwrap(),
            &serde_json::json!("overwritten")
        );
    }

    // ── 16. merge_updates: bounds preservation ───────────────────────────

    #[test]
    fn test_merge_updates_bounds_preservation() {
        let mut env = Run::anonymous();
        env.metrics.llm_calls = 5;
        env.limits.max_llm_calls = 10;

        let mut updates = HashMap::new();
        updates.insert("raw_input".to_string(), serde_json::json!("new input text"));

        env.merge_updates(updates);

        assert_eq!(env.raw_input, "new input text");
        assert_eq!(env.metrics.llm_calls, 5);
        assert_eq!(env.limits.max_llm_calls, 10);
    }

    // ── 17. validate: valid run passes ───────────────────────────────

    #[test]
    fn test_validate_default_passes() {
        let env = Run::anonymous();
        assert!(env.validate().is_ok());
    }

    // ── 18. validate: terminated coherence ────────────────────────────────

    #[test]
    fn test_validate_terminated_passes() {
        let mut env = Run::anonymous();
        env.terminate_with(TerminalReason::Completed, None);
        assert!(env.validate().is_ok());
    }

    // ── 19. validate: negative counters ───────────────────────────────────

    #[test]
    fn test_validate_negative_llm_count_fails() {
        let mut env = Run::anonymous();
        env.metrics.llm_calls = -1;
        assert!(env.validate().is_err());
    }

    // ── 20. validate: zero max fails ──────────────────────────────────────

    #[test]
    fn test_validate_zero_max_llm_calls_fails() {
        let mut env = Run::anonymous();
        env.limits.max_llm_calls = 0;
        assert!(env.validate().is_err());
    }

    // ── 22. validate: pipeline stage_order ────────────────────────────────

    #[test]
    fn test_validate_current_stage_not_in_order_fails() {
        let mut env = Run::anonymous();
        env.stage_order = vec!["understand".to_string(), "respond".to_string()];
        env.current_stage = "missing_stage".to_string();
        assert!(env.validate().is_err());
    }

    #[test]
    fn test_validate_current_stage_in_order_passes() {
        let mut env = Run::anonymous();
        env.stage_order = vec!["understand".to_string(), "respond".to_string()];
        env.current_stage = "understand".to_string();
        assert!(env.validate().is_ok());
    }
}
