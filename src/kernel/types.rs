//! Kernel types: ProcessState, ProcessControlBlock, Resource tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::envelope::InterruptKind;

/// Process lifecycle state (Unix-like).
///
/// State transitions:
/// ```text
/// NEW → READY → RUNNING → {WAITING | BLOCKED | TERMINATED}
///                    ↓         ↓
///                  READY     ZOMBIE
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum SchedulingPriority {
    Realtime,
    High,
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

impl Default for SchedulingPriority {
    fn default() -> Self {
        SchedulingPriority::Normal
    }
}

/// Resource quota (cgroup-style limits).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceQuota {
    pub max_llm_calls: i32,
    pub max_tool_calls: i32,
    pub max_agent_hops: i32,
    pub max_iterations: i32,
    pub max_tokens_in: i64,
    pub max_tokens_out: i64,
    pub max_elapsed_seconds: i32,
    pub max_inference_requests: i32,
    pub max_inference_input_chars: i64,
}

impl ResourceQuota {
    pub fn default_quota() -> Self {
        Self {
            max_llm_calls: 100,
            max_tool_calls: 50,
            max_agent_hops: 10,
            max_iterations: 20,
            max_tokens_in: 100_000,
            max_tokens_out: 50_000,
            max_elapsed_seconds: 300,
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

impl ResourceUsage {
    /// Check if any quota is exceeded.
    pub fn exceeds_quota(&self, quota: &ResourceQuota) -> Option<String> {
        if self.llm_calls > quota.max_llm_calls {
            return Some(format!(
                "llm_calls {} > {}",
                self.llm_calls, quota.max_llm_calls
            ));
        }
        if self.tool_calls > quota.max_tool_calls {
            return Some(format!(
                "tool_calls {} > {}",
                self.tool_calls, quota.max_tool_calls
            ));
        }
        if self.agent_hops > quota.max_agent_hops {
            return Some(format!(
                "agent_hops {} > {}",
                self.agent_hops, quota.max_agent_hops
            ));
        }
        if self.iterations > quota.max_iterations {
            return Some(format!(
                "iterations {} > {}",
                self.iterations, quota.max_iterations
            ));
        }
        if self.tokens_in > quota.max_tokens_in {
            return Some(format!(
                "tokens_in {} > {}",
                self.tokens_in, quota.max_tokens_in
            ));
        }
        if self.tokens_out > quota.max_tokens_out {
            return Some(format!(
                "tokens_out {} > {}",
                self.tokens_out, quota.max_tokens_out
            ));
        }
        if self.elapsed_seconds > quota.max_elapsed_seconds as f64 {
            return Some(format!(
                "elapsed_seconds {} > {}",
                self.elapsed_seconds, quota.max_elapsed_seconds
            ));
        }
        if self.inference_requests > quota.max_inference_requests {
            return Some(format!(
                "inference_requests {} > {}",
                self.inference_requests, quota.max_inference_requests
            ));
        }
        if self.inference_input_chars > quota.max_inference_input_chars {
            return Some(format!(
                "inference_input_chars {} > {}",
                self.inference_input_chars, quota.max_inference_input_chars
            ));
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
    // Identity
    pub pid: String,
    pub request_id: String,
    pub user_id: String,
    pub session_id: String,

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

    // Current execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_stage: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_service: Option<String>,

    // Interrupt handling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_interrupt: Option<InterruptKind>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt_data: Option<HashMap<String, serde_json::Value>>,

    // Parent/child relationships
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<String>,

    pub child_pids: Vec<String>,
}

impl ProcessControlBlock {
    pub fn new(pid: String, request_id: String, user_id: String, session_id: String) -> Self {
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
            current_stage: None,
            current_service: None,
            pending_interrupt: None,
            interrupt_data: None,
            parent_pid: None,
            child_pids: Vec::new(),
        }
    }

    /// Transition to RUNNING state.
    pub fn start(&mut self) {
        let now = Utc::now();
        self.state = ProcessState::Running;
        self.started_at = Some(now);
        self.last_scheduled_at = Some(now);
    }

    /// Transition to TERMINATED state.
    pub fn complete(&mut self) {
        let now = Utc::now();
        self.state = ProcessState::Terminated;
        self.completed_at = Some(now);
        if let Some(started) = self.started_at {
            self.usage.elapsed_seconds = (now - started).num_milliseconds() as f64 / 1000.0;
        }
    }

    /// Transition to BLOCKED state.
    pub fn block(&mut self, reason: String) {
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
    pub fn wait(&mut self, interrupt_kind: InterruptKind) {
        self.state = ProcessState::Waiting;
        self.pending_interrupt = Some(interrupt_kind);
    }

    /// Resume from WAITING/BLOCKED to READY.
    pub fn resume(&mut self) {
        if matches!(self.state, ProcessState::Waiting | ProcessState::Blocked) {
            self.state = ProcessState::Ready;
            self.pending_interrupt = None;
        }
    }

    /// Check if any quota exceeded.
    pub fn check_quota(&self) -> Option<String> {
        self.usage.exceeds_quota(&self.quota)
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
