//! Envelope sub-structs and supporting types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::types::{EnvelopeId, RequestId, SessionId, UserId};


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

/// Flow interrupt — pipeline pause awaiting consumer response.
///
/// Self-describes via `message`, `question`, and `data`. There is no
/// kind discriminator; if a future need arises for typed variants the
/// enum can be re-introduced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowInterrupt {
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
    pub fn new() -> Self {
        Self {
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

impl Default for FlowInterrupt {
    fn default() -> Self {
        Self::new()
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

    #[serde(default)]
    pub tool_calls: i32,

    #[serde(default)]
    pub tokens_in: i64,

    #[serde(default)]
    pub tokens_out: i64,
}

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

    /// Completed stage snapshots (audit trail, write-only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_stage_snapshots: Vec<HashMap<String, serde_json::Value>>,
    /// Current stage number (1-indexed position in stage_order).
    #[serde(default)]
    pub current_stage_number: i32,
    /// Total number of stages in the pipeline.
    #[serde(default)]
    pub max_stages: i32,
}

/// Represents a completed termination with reason and optional message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Termination {
    pub reason: super::enums::TerminalReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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
    pub termination: Option<Termination>,
}

impl Bounds {
    /// Check if terminated.
    pub fn is_terminated(&self) -> bool {
        self.termination.is_some()
    }

    /// Get the terminal reason, if terminated.
    pub fn terminal_reason(&self) -> Option<super::enums::TerminalReason> {
        self.termination.as_ref().map(|t| t.reason)
    }

    /// Terminate with a reason and optional message.
    pub fn terminate(&mut self, reason: super::enums::TerminalReason, message: Option<String>) {
        self.termination = Some(Termination { reason, message });
    }
}

/// Human-in-the-loop interrupt state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterruptState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<FlowInterrupt>,
}

impl InterruptState {
    pub fn is_pending(&self) -> bool {
        self.interrupt.is_some()
    }
}

/// Audit trail: history, timing, metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Audit {
    pub processing_history: Vec<ProcessingRecord>,
    pub created_at: DateTime<Utc>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    pub metadata: HashMap<String, serde_json::Value>,
}
