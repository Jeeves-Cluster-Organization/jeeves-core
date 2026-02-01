//! Envelope - the core state container.
//!
//! The Envelope represents the mutable state of a request as it flows through
//! the multi-agent pipeline. It tracks inputs, outputs, bounds, and interrupts.
//!
//! This is a direct port of Go's envelope.go with dynamic `Outputs` map for
//! flexible agent architecture.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod enums;
pub mod export;
pub mod import;

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
            id: format!("int_{}", uuid::Uuid::new_v4().simple().to_string()[..16].to_string()),
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

/// Processing record for audit trail.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessingRecord {
    pub agent: String,
    pub stage_order: i32,
    pub started_at: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    pub duration_ms: i32,
    pub status: String, // "running", "success", "error", "skipped"

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    pub llm_calls: i32,
}

/// Main envelope structure.
///
/// Unlike hardcoded per-agent output fields, Envelope uses a dynamic `Outputs` map
/// where any agent can write results keyed by agent name.
///
/// Example:
/// ```
/// envelope.outputs.insert("perception", {...});
/// envelope.outputs.insert("intent", {...});
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    // ===== Identification =====
    pub envelope_id: String,
    pub request_id: String,
    pub user_id: String,
    pub session_id: String,

    // ===== Original Input =====
    pub raw_input: String,
    pub received_at: DateTime<Utc>,

    // ===== Dynamic Agent Outputs =====
    /// Outputs from agents: map[agent_name] = map[output_key] = value
    pub outputs: HashMap<String, HashMap<String, serde_json::Value>>,

    // ===== Pipeline State (string-based, not enum) =====
    pub current_stage: String,
    pub stage_order: Vec<String>,
    pub iteration: i32,
    pub max_iterations: i32,

    // ===== Parallel Execution State =====
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_stages: Option<HashMap<String, bool>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_stage_set: Option<HashMap<String, bool>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_stages: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_mode: Option<bool>,

    // ===== Bounds Tracking =====
    pub llm_call_count: i32,
    pub max_llm_calls: i32,
    pub agent_hop_count: i32,
    pub max_agent_hops: i32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<TerminalReason>,

    // ===== Control Flow =====
    pub terminated: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub termination_reason: Option<String>,

    // ===== Unified Interrupt Handling =====
    pub interrupt_pending: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<FlowInterrupt>,

    // ===== Multi-Stage Execution =====
    pub completed_stages: Vec<HashMap<String, serde_json::Value>>,
    pub current_stage_number: i32,
    pub max_stages: i32,
    pub all_goals: Vec<String>,
    pub remaining_goals: Vec<String>,
    pub goal_completion_status: HashMap<String, String>,

    // ===== Retry Context =====
    pub prior_plans: Vec<HashMap<String, serde_json::Value>>,
    pub loop_feedback: Vec<String>,

    // ===== Audit Trail =====
    pub processing_history: Vec<ProcessingRecord>,
    pub errors: Vec<HashMap<String, serde_json::Value>>,

    // ===== Timing =====
    pub created_at: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    // ===== Metadata =====
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Envelope {
    /// Create a new envelope with default values (matches Go's NewEnvelope).
    pub fn new() -> Self {
        let now = Utc::now();
        let uuid_short = || uuid::Uuid::new_v4().simple().to_string()[..16].to_string();

        Self {
            // Identification
            envelope_id: format!("env_{}", uuid_short()),
            request_id: format!("req_{}", uuid_short()),
            user_id: "anonymous".to_string(),
            session_id: format!("sess_{}", uuid_short()),

            // Input
            raw_input: String::new(),
            received_at: now,

            // Outputs
            outputs: HashMap::new(),

            // Pipeline state
            current_stage: "start".to_string(),
            stage_order: Vec::new(),
            iteration: 0,
            max_iterations: 3,

            // Parallel execution
            active_stages: Some(HashMap::new()),
            completed_stage_set: Some(HashMap::new()),
            failed_stages: Some(HashMap::new()),
            parallel_mode: Some(false),

            // Bounds
            llm_call_count: 0,
            max_llm_calls: 10,
            agent_hop_count: 0,
            max_agent_hops: 21,
            terminal_reason: None,

            // Control flow
            terminated: false,
            termination_reason: None,

            // Interrupts
            interrupt_pending: false,
            interrupt: None,

            // Multi-stage
            completed_stages: Vec::new(),
            current_stage_number: 1,
            max_stages: 5,
            all_goals: Vec::new(),
            remaining_goals: Vec::new(),
            goal_completion_status: HashMap::new(),

            // Retry
            prior_plans: Vec::new(),
            loop_feedback: Vec::new(),

            // Audit
            processing_history: Vec::new(),
            errors: Vec::new(),

            // Timing
            created_at: now,
            completed_at: None,

            // Metadata
            metadata: HashMap::new(),
        }
    }

    /// Start a stage (mark as actively executing).
    pub fn start_stage(&mut self, stage_name: impl Into<String>) {
        if self.active_stages.is_none() {
            self.active_stages = Some(HashMap::new());
        }
        if let Some(ref mut stages) = self.active_stages {
            stages.insert(stage_name.into(), true);
        }
    }

    /// Complete a stage successfully.
    pub fn complete_stage(&mut self, stage_name: &str) {
        if self.completed_stage_set.is_none() {
            self.completed_stage_set = Some(HashMap::new());
        }
        if let Some(ref mut completed) = self.completed_stage_set {
            completed.insert(stage_name.to_string(), true);
        }
        if let Some(ref mut active) = self.active_stages {
            active.remove(stage_name);
        }
    }

    /// Mark a stage as failed.
    pub fn fail_stage(&mut self, stage_name: impl Into<String>, error_msg: impl Into<String>) {
        let stage_name_str = stage_name.into();
        if self.failed_stages.is_none() {
            self.failed_stages = Some(HashMap::new());
        }
        if let Some(ref mut failed) = self.failed_stages {
            failed.insert(stage_name_str.clone(), error_msg.into());
        }
        if let Some(ref mut active) = self.active_stages {
            active.remove(&stage_name_str);
        }
    }

    /// Check if a stage is completed.
    pub fn is_stage_completed(&self, stage_name: &str) -> bool {
        self.completed_stage_set
            .as_ref()
            .and_then(|s| s.get(stage_name))
            .copied()
            .unwrap_or(false)
    }

    /// Check if a stage failed.
    pub fn is_stage_failed(&self, stage_name: &str) -> bool {
        self.failed_stages
            .as_ref()
            .and_then(|s| s.get(stage_name))
            .is_some()
    }

    /// Check if at any bound limit.
    pub fn at_limit(&self) -> bool {
        self.llm_call_count >= self.max_llm_calls || self.agent_hop_count >= self.max_agent_hops
    }

    /// Increment LLM call counter.
    pub fn increment_llm_calls(&mut self, count: i32) {
        self.llm_call_count += count;
    }

    /// Increment agent hop counter.
    pub fn increment_agent_hops(&mut self) {
        self.agent_hop_count += 1;
    }

    /// Add processing record to history.
    pub fn add_processing_record(&mut self, record: ProcessingRecord) {
        self.processing_history.push(record);
    }

    /// Terminate envelope with reason.
    pub fn terminate(&mut self, reason: impl Into<String>) {
        self.terminated = true;
        self.termination_reason = Some(reason.into());
        self.completed_at = Some(Utc::now());
    }

    /// Set interrupt pending.
    pub fn set_interrupt(&mut self, interrupt: FlowInterrupt) {
        self.interrupt_pending = true;
        self.interrupt = Some(interrupt);
    }

    /// Clear interrupt.
    pub fn clear_interrupt(&mut self) {
        self.interrupt_pending = false;
        self.interrupt = None;
    }
}

impl Default for Envelope {
    fn default() -> Self {
        Self::new()
    }
}
