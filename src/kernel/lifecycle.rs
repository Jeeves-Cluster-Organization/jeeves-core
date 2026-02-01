//! Process lifecycle management.
//!
//! Implements Unix-like process state machine:
//! NEW → READY → RUNNING → {WAITING|BLOCKED|TERMINATED} → ZOMBIE

use serde::{Deserialize, Serialize};

/// Process state (checkpoint 3 - stub for now).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessState {
    New,
    Ready,
    Running,
    Waiting,
    Blocked,
    Terminated,
    Zombie,
}

/// Process control block (checkpoint 3 - stub for now).
#[derive(Debug)]
pub struct ProcessControlBlock {
    pub state: ProcessState,
}

/// Lifecycle manager (checkpoint 3 - stub for now).
#[derive(Debug)]
pub struct LifecycleManager {
    // State machine will be implemented in checkpoint 3
}

impl LifecycleManager {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}
