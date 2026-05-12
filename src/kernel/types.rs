//! Kernel types: RunStatus, RunRecord, Resource tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::types::{InterruptId, RunId, RequestId, SessionId, UserId};

/// Process lifecycle state.
///
/// 3-state machine. A process pauses on a tool-confirmation interrupt by
/// staying in `Running` and stamping `pending_interrupt` on its PCB; the
/// kernel doesn't have a separate Waiting/Blocked state for that case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunStatus {
    /// Created and ready to run; not yet started.
    Ready,
    /// Active execution. May be suspended on a pending interrupt; consult
    /// `RunRecord::pending_interrupt` to differentiate.
    Running,
    /// Terminated. Processes in this state are immediately removed by the
    /// kernel; they don't linger as zombies.
    Terminated,
}

impl RunStatus {
    /// Check if this is a terminal state.
    pub fn is_terminal(self) -> bool {
        self == RunStatus::Terminated
    }
}

/// Resource quota — bounds enforced per process.
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
            Self::Timeout { elapsed, limit } => write!(f, "elapsed_seconds {} > {}", elapsed, limit),
        }
    }
}

impl ResourceUsage {
    /// Check if any quota is exceeded.
    pub fn exceeds_quota(&self, quota: &ResourceQuota) -> Option<QuotaViolation> {
        if self.llm_calls > quota.max_llm_calls {
            return Some(QuotaViolation::LlmCalls { used: self.llm_calls, limit: quota.max_llm_calls });
        }
        if self.tool_calls > quota.max_tool_calls {
            return Some(QuotaViolation::ToolCalls { used: self.tool_calls, limit: quota.max_tool_calls });
        }
        if self.agent_hops > quota.max_agent_hops {
            return Some(QuotaViolation::AgentHops { used: self.agent_hops, limit: quota.max_agent_hops });
        }
        if self.iterations > quota.max_iterations {
            return Some(QuotaViolation::Iterations { used: self.iterations, limit: quota.max_iterations });
        }
        if self.tokens_in > quota.max_input_tokens as i64 {
            return Some(QuotaViolation::TokensIn { used: self.tokens_in, limit: quota.max_input_tokens as i64 });
        }
        if self.tokens_out > quota.max_output_tokens as i64 {
            return Some(QuotaViolation::TokensOut { used: self.tokens_out, limit: quota.max_output_tokens as i64 });
        }
        if quota.timeout_seconds > 0 && self.elapsed_seconds > quota.timeout_seconds as f64 {
            return Some(QuotaViolation::Timeout { elapsed: self.elapsed_seconds, limit: quota.timeout_seconds as f64 });
        }
        None
    }
}

/// Process Control Block — kernel's metadata about a running process.
///
/// The actual request state is in `Run`; this tracks scheduling state,
/// resource accounting, and a pointer to any pending tool-confirmation
/// interrupt by ID (resolved through the interrupts service, not the PCB).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunRecord {
    // Identity
    pub pid: RunId,
    pub request_id: RequestId,
    pub user_id: UserId,
    pub session_id: SessionId,

    // State
    pub state: RunStatus,

    /// Per-run quota. Live counters live on `Run.metrics`; check via
    /// `Kernel::check_quota`.
    pub quota: ResourceQuota,

    // Timestamps
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,

    /// ID of a tool-confirmation interrupt this process is currently
    /// suspended on. None when actively running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_interrupt: Option<InterruptId>,
}

impl RunRecord {
    pub fn new(pid: RunId, request_id: RequestId, user_id: UserId, session_id: SessionId) -> Self {
        Self {
            pid,
            request_id,
            user_id,
            session_id,
            state: RunStatus::Ready,
            quota: ResourceQuota::default(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            pending_interrupt: None,
        }
    }

    /// Transition to RUNNING state.
    pub(crate) fn start(&mut self) {
        self.state = RunStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Transition to TERMINATED state.
    pub(crate) fn complete(&mut self) {
        self.state = RunStatus::Terminated;
        self.completed_at = Some(Utc::now());
    }

    /// Elapsed wall-clock seconds since `start()`. `0.0` before start. Frozen
    /// after `complete()`.
    pub fn elapsed_seconds(&self) -> f64 {
        let Some(started) = self.started_at else {
            return 0.0;
        };
        let end = self.completed_at.unwrap_or_else(Utc::now);
        (end - started).num_milliseconds() as f64 / 1000.0
    }

    /// Check if process terminated.
    pub fn is_terminated(&self) -> bool {
        self.state.is_terminal()
    }
}
