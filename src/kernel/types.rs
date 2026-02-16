//! Kernel types: ProcessState, ProcessControlBlock, Resource tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::envelope::InterruptKind;
use crate::types::{ProcessId, RequestId, SessionId, UserId};

/// Process lifecycle state (Unix-like).
///
/// State transitions:
/// ```text
/// NEW → READY → RUNNING → {WAITING | BLOCKED | TERMINATED}
///                    ↓         ↓
///                  READY     ZOMBIE
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcessState {
    New,
    Ready,
    Running,
    Waiting,
    Blocked,
    Terminated,
    Zombie,
}

impl ProcessState {
    /// Check if this is a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(self, ProcessState::Terminated | ProcessState::Zombie)
    }

    /// Check if process can be scheduled.
    pub fn can_schedule(self) -> bool {
        matches!(self, ProcessState::New | ProcessState::Ready)
    }

    /// Check if process is runnable.
    pub fn is_runnable(self) -> bool {
        self == ProcessState::Ready
    }

    /// Check if transition is valid.
    pub fn can_transition_to(self, to: ProcessState) -> bool {
        match (self, to) {
            // NEW
            (ProcessState::New, ProcessState::Ready) => true,
            (ProcessState::New, ProcessState::Terminated) => true,
            // READY
            (ProcessState::Ready, ProcessState::Running) => true,
            (ProcessState::Ready, ProcessState::Terminated) => true,
            // RUNNING
            (ProcessState::Running, ProcessState::Ready) => true, // Preempted
            (ProcessState::Running, ProcessState::Waiting) => true, // I/O
            (ProcessState::Running, ProcessState::Blocked) => true, // Resource exhausted
            (ProcessState::Running, ProcessState::Terminated) => true,
            // WAITING
            (ProcessState::Waiting, ProcessState::Ready) => true,
            (ProcessState::Waiting, ProcessState::Terminated) => true,
            // BLOCKED
            (ProcessState::Blocked, ProcessState::Ready) => true,
            (ProcessState::Blocked, ProcessState::Terminated) => true,
            // TERMINATED
            (ProcessState::Terminated, ProcessState::Zombie) => true,
            // ZOMBIE is terminal
            (ProcessState::Zombie, _) => false,
            // All other transitions invalid
            _ => false,
        }
    }
}

/// Scheduling priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SchedulingPriority {
    Realtime,
    High,
    #[default]
    Normal,
    Low,
    Idle,
}

impl SchedulingPriority {
    /// Get heap priority value (lower = higher priority).
    pub fn to_heap_value(self) -> i32 {
        match self {
            SchedulingPriority::Realtime => 0,
            SchedulingPriority::High => 1,
            SchedulingPriority::Normal => 2,
            SchedulingPriority::Low => 3,
            SchedulingPriority::Idle => 4,
        }
    }
}

// Default derived via #[default] attribute on Normal variant above.

/// Resource quota (cgroup-style limits).
/// Matches proto ResourceQuota exactly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceQuota {
    pub max_input_tokens: i32,
    pub max_output_tokens: i32,
    pub max_context_tokens: i32,
    pub max_llm_calls: i32,
    pub max_tool_calls: i32,
    pub max_agent_hops: i32,
    pub max_iterations: i32,
    pub timeout_seconds: i32,
    pub soft_timeout_seconds: i32,
    pub rate_limit_rpm: i32,
    pub rate_limit_rph: i32,
    pub rate_limit_burst: i32,
    pub max_inference_requests: i32,
    pub max_inference_input_chars: i32,
}

impl ResourceQuota {
    pub fn default_quota() -> Self {
        Self {
            max_input_tokens: 100_000,
            max_output_tokens: 50_000,
            max_context_tokens: 150_000,
            max_llm_calls: 100,
            max_tool_calls: 50,
            max_agent_hops: 10,
            max_iterations: 20,
            timeout_seconds: 300,
            soft_timeout_seconds: 240,
            rate_limit_rpm: 60,
            rate_limit_rph: 1000,
            rate_limit_burst: 10,
            max_inference_requests: 50,
            max_inference_input_chars: 500_000,
        }
    }
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self::default_quota()
    }
}

/// Resource usage tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResourceUsage {
    pub llm_calls: i32,
    pub tool_calls: i32,
    pub agent_hops: i32,
    pub iterations: i32,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub elapsed_seconds: f64,
    pub inference_requests: i32,
    pub inference_input_chars: i64,
}

/// Which quota was exceeded and by how much.
#[derive(Debug, Clone, PartialEq)]
pub enum QuotaViolation {
    LlmCalls { used: i32, limit: i32 },
    ToolCalls { used: i32, limit: i32 },
    AgentHops { used: i32, limit: i32 },
    Iterations { used: i32, limit: i32 },
    TokensIn { used: i64, limit: i64 },
    TokensOut { used: i64, limit: i64 },
    Timeout { elapsed: f64, limit: f64 },
    InferenceRequests { used: i32, limit: i32 },
    InferenceInputChars { used: i64, limit: i64 },
}

impl std::fmt::Display for QuotaViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LlmCalls { used, limit } => write!(f, "llm_calls {} > {}", used, limit),
            Self::ToolCalls { used, limit } => write!(f, "tool_calls {} > {}", used, limit),
            Self::AgentHops { used, limit } => write!(f, "agent_hops {} > {}", used, limit),
            Self::Iterations { used, limit } => write!(f, "iterations {} > {}", used, limit),
            Self::TokensIn { used, limit } => write!(f, "tokens_in {} > {}", used, limit),
            Self::TokensOut { used, limit } => write!(f, "tokens_out {} > {}", used, limit),
            Self::Timeout { elapsed, limit } => {
                write!(f, "elapsed_seconds {} > {}", elapsed, limit)
            }
            Self::InferenceRequests { used, limit } => {
                write!(f, "inference_requests {} > {}", used, limit)
            }
            Self::InferenceInputChars { used, limit } => {
                write!(f, "inference_input_chars {} > {}", used, limit)
            }
        }
    }
}

impl ResourceUsage {
    /// Check if any quota is exceeded.
    pub fn exceeds_quota(&self, quota: &ResourceQuota) -> Option<QuotaViolation> {
        if self.llm_calls > quota.max_llm_calls {
            return Some(QuotaViolation::LlmCalls {
                used: self.llm_calls,
                limit: quota.max_llm_calls,
            });
        }
        if self.tool_calls > quota.max_tool_calls {
            return Some(QuotaViolation::ToolCalls {
                used: self.tool_calls,
                limit: quota.max_tool_calls,
            });
        }
        if self.agent_hops > quota.max_agent_hops {
            return Some(QuotaViolation::AgentHops {
                used: self.agent_hops,
                limit: quota.max_agent_hops,
            });
        }
        if self.iterations > quota.max_iterations {
            return Some(QuotaViolation::Iterations {
                used: self.iterations,
                limit: quota.max_iterations,
            });
        }
        if self.tokens_in > quota.max_input_tokens as i64 {
            return Some(QuotaViolation::TokensIn {
                used: self.tokens_in,
                limit: quota.max_input_tokens as i64,
            });
        }
        if self.tokens_out > quota.max_output_tokens as i64 {
            return Some(QuotaViolation::TokensOut {
                used: self.tokens_out,
                limit: quota.max_output_tokens as i64,
            });
        }
        if self.elapsed_seconds > quota.timeout_seconds as f64 {
            return Some(QuotaViolation::Timeout {
                elapsed: self.elapsed_seconds,
                limit: quota.timeout_seconds as f64,
            });
        }
        if self.inference_requests > quota.max_inference_requests {
            return Some(QuotaViolation::InferenceRequests {
                used: self.inference_requests,
                limit: quota.max_inference_requests,
            });
        }
        if self.inference_input_chars > quota.max_inference_input_chars as i64 {
            return Some(QuotaViolation::InferenceInputChars {
                used: self.inference_input_chars,
                limit: quota.max_inference_input_chars as i64,
            });
        }
        None
    }
}

/// Process Control Block - kernel's metadata about a running process.
///
/// The actual request state is in Envelope; this tracks:
/// - Scheduling state
/// - Resource accounting
/// - Interrupt status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessControlBlock {
    // Identity (typed — validated at construction)
    pub pid: ProcessId,
    pub request_id: RequestId,
    pub user_id: UserId,
    pub session_id: SessionId,

    // State
    pub state: ProcessState,
    pub priority: SchedulingPriority,

    // Resource tracking
    pub quota: ResourceQuota,
    pub usage: ResourceUsage,

    // Scheduling timestamps
    pub created_at: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_scheduled_at: Option<DateTime<Utc>>,

    // Interrupt handling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_interrupt: Option<InterruptKind>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt_data: Option<HashMap<String, serde_json::Value>>,

    // Parent/child relationships
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<ProcessId>,

    pub child_pids: Vec<ProcessId>,
}

impl ProcessControlBlock {
    pub fn new(
        pid: ProcessId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
    ) -> Self {
        Self {
            pid,
            request_id,
            user_id,
            session_id,
            state: ProcessState::New,
            priority: SchedulingPriority::default(),
            quota: ResourceQuota::default(),
            usage: ResourceUsage::default(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            last_scheduled_at: None,
            pending_interrupt: None,
            interrupt_data: None,
            parent_pid: None,
            child_pids: Vec::new(),
        }
    }

    /// Transition to RUNNING state.
    pub(crate) fn start(&mut self) {
        let now = Utc::now();
        self.state = ProcessState::Running;
        self.started_at = Some(now);
        self.last_scheduled_at = Some(now);
    }

    /// Transition to TERMINATED state.
    pub(crate) fn complete(&mut self) {
        let now = Utc::now();
        self.state = ProcessState::Terminated;
        self.completed_at = Some(now);
        if let Some(started) = self.started_at {
            self.usage.elapsed_seconds = (now - started).num_milliseconds() as f64 / 1000.0;
        }
    }

    /// Transition to BLOCKED state.
    pub(crate) fn block(&mut self, reason: String) {
        self.state = ProcessState::Blocked;
        if self.interrupt_data.is_none() {
            self.interrupt_data = Some(HashMap::new());
        }
        if let Some(ref mut data) = self.interrupt_data {
            data.insert(
                "block_reason".to_string(),
                serde_json::Value::String(reason),
            );
        }
    }

    /// Transition to WAITING state.
    pub(crate) fn wait(&mut self, interrupt_kind: InterruptKind) {
        self.state = ProcessState::Waiting;
        self.pending_interrupt = Some(interrupt_kind);
    }

    /// Resume from WAITING/BLOCKED to READY.
    pub(crate) fn resume(&mut self) {
        if matches!(self.state, ProcessState::Waiting | ProcessState::Blocked) {
            self.state = ProcessState::Ready;
            self.pending_interrupt = None;
        }
    }

    /// Check if any quota exceeded.
    ///
    /// Computes elapsed time from `started_at` on the fly so that timeout
    /// enforcement works during execution, not only after completion.
    pub fn check_quota(&self) -> Option<QuotaViolation> {
        let mut usage = self.usage.clone();
        if let Some(started) = self.started_at {
            if self.completed_at.is_none() {
                // Process still running — compute live elapsed time
                let now = Utc::now();
                usage.elapsed_seconds = (now - started).num_milliseconds() as f64 / 1000.0;
            }
        }
        usage.exceeds_quota(&self.quota)
    }

    /// Check if process can be scheduled.
    pub fn can_schedule(&self) -> bool {
        self.state.can_schedule()
    }

    /// Check if process is runnable.
    pub fn is_runnable(&self) -> bool {
        self.state.is_runnable()
    }

    /// Check if process terminated.
    pub fn is_terminated(&self) -> bool {
        self.state.is_terminal()
    }
}
