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

// =============================================================================
// Sub-structs
// =============================================================================

/// Envelope identity fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Identity {
    pub envelope_id: String,
    pub request_id: String,
    pub user_id: String,
    pub session_id: String,
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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_stages: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_mode: Option<bool>,
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
                envelope_id: format!("env_{}", uuid_short()),
                request_id: format!("req_{}", uuid_short()),
                user_id: "anonymous".to_string(),
                session_id: format!("sess_{}", uuid_short()),
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
                failed_stages: Some(HashMap::new()),
                parallel_mode: Some(false),
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
        if self.pipeline.failed_stages.is_none() {
            self.pipeline.failed_stages = Some(HashMap::new());
        }
        if let Some(ref mut failed) = self.pipeline.failed_stages {
            failed.insert(stage_name_str.clone(), error_msg.into());
        }
        self.pipeline.active_stages.remove(&stage_name_str);
    }

    /// Check if a stage is completed.
    pub fn is_stage_completed(&self, stage_name: &str) -> bool {
        self.pipeline.completed_stage_set.contains(stage_name)
    }

    /// Check if a stage failed.
    pub fn is_stage_failed(&self, stage_name: &str) -> bool {
        self.pipeline
            .failed_stages
            .as_ref()
            .and_then(|s| s.get(stage_name))
            .is_some()
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
}

impl Default for Envelope {
    fn default() -> Self {
        Self::new()
    }
}
