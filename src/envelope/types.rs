//! Envelope sub-structs and supporting types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::types::{EnvelopeId, RequestId, SessionId, UserId};

use super::enums::InterruptKind;

/// Response to a flow interrupt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterruptResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, serde_json::Value>>,

    pub received_at: DateTime<Utc>,
}

/// Flow interrupt (clarification, confirmation, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowInterrupt {
    pub kind: InterruptKind,
    pub id: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, serde_json::Value>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<InterruptResponse>,

    pub created_at: DateTime<Utc>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
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
        self.expires_at = Some(Utc::now() + chrono::Duration::from_std(duration).unwrap_or(chrono::TimeDelta::MAX));
        self
    }
}

/// Status of a processing record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
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

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    pub duration_ms: i32,
    pub status: ProcessingStatus,

    #[serde(default, skip_serializing_if = "Option::is_none")]
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

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<super::enums::TerminalReason>,

    pub terminated: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub termination_reason: Option<String>,
}

/// Human-in-the-loop interrupt state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterruptState {
    pub interrupt_pending: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
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

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    pub metadata: HashMap<String, serde_json::Value>,
}
