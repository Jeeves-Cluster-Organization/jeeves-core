//! Process lifecycle management.
//!
//! Implements Unix-like process state machine with priority scheduling:
//! NEW → READY → RUNNING → {WAITING|BLOCKED|TERMINATED} → ZOMBIE

use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::types::{Error, Result};

pub use super::types::{ProcessControlBlock, ProcessState, ResourceQuota, SchedulingPriority};

/// Priority queue item (wraps for min-heap behavior).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PriorityItem {
    pid: String,
    priority: i32,      // Lower = higher priority
    created_at: DateTime<Utc>, // FIFO within same priority
}

impl Ord for PriorityItem {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is max-heap, so reverse priority
        other
            .priority
            .cmp(&self.priority)
            // Then FIFO for same priority
            .then_with(|| self.created_at.cmp(&other.created_at))
    }
}

impl PartialOrd for PriorityItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Lifecycle manager - kernel's process scheduler.
///
/// Manages process lifecycles with priority-based ready queue.
/// NOT a separate actor - owned by Kernel and called via &mut self.
#[derive(Debug)]
pub struct LifecycleManager {
    default_quota: ResourceQuota,
    processes: HashMap<String, ProcessControlBlock>,
    ready_queue: BinaryHeap<PriorityItem>,
}

impl LifecycleManager {
    pub fn new(default_quota: Option<ResourceQuota>) -> Self {
        Self {
            default_quota: default_quota.unwrap_or_default(),
            processes: HashMap::new(),
            ready_queue: BinaryHeap::new(),
        }
    }

    /// Submit creates a new process in NEW state.
    pub fn submit(
        &mut self,
        pid: String,
        request_id: String,
        user_id: String,
        session_id: String,
        priority: SchedulingPriority,
        quota: Option<ResourceQuota>,
    ) -> Result<ProcessControlBlock> {
        // Check for duplicate
        if let Some(existing) = self.processes.get(&pid) {
            return Ok(existing.clone());
        }

        // Create PCB
        let mut pcb = ProcessControlBlock::new(pid.clone(), request_id, user_id, session_id);
        pcb.priority = priority;
        pcb.quota = quota.unwrap_or_else(|| self.default_quota.clone());

        self.processes.insert(pid, pcb.clone());
        Ok(pcb)
    }

    /// Schedule transitions process from NEW to READY and adds to queue.
    pub fn schedule(&mut self, pid: &str) -> Result<()> {
        let pcb = self
            .processes
            .get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))?;

        if pcb.state != ProcessState::New {
            return Err(Error::state_transition(format!(
                "cannot schedule pid {}: state is {:?}, expected New",
                pid, pcb.state
            )));
        }

        // Transition to READY
        pcb.state = ProcessState::Ready;

        // Add to ready queue
        self.ready_queue.push(PriorityItem {
            pid: pid.to_string(),
            priority: pcb.priority.to_heap_value(),
            created_at: pcb.created_at,
        });

        Ok(())
    }

    /// Get next runnable process (highest priority from ready queue).
    pub fn get_next_runnable(&mut self) -> Option<ProcessControlBlock> {
        while let Some(item) = self.ready_queue.pop() {
            if let Some(pcb) = self.processes.get(&item.pid) {
                if pcb.state == ProcessState::Ready {
                    return Some(pcb.clone());
                }
            }
        }
        None
    }

    /// Transition process to RUNNING state.
    pub fn start(&mut self, pid: &str) -> Result<()> {
        let pcb = self
            .processes
            .get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))?;

        if pcb.state != ProcessState::Ready {
            return Err(Error::state_transition(format!(
                "cannot start pid {}: state is {:?}, expected Ready",
                pid, pcb.state
            )));
        }

        pcb.start();
        Ok(())
    }

    /// Transition process to WAITING state (e.g., awaiting clarification).
    pub fn wait(&mut self, pid: &str, interrupt_kind: crate::envelope::InterruptKind) -> Result<()> {
        let pcb = self
            .processes
            .get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))?;

        if pcb.state != ProcessState::Running {
            return Err(Error::state_transition(format!(
                "cannot wait pid {}: state is {:?}, expected Running",
                pid, pcb.state
            )));
        }

        pcb.wait(interrupt_kind);
        Ok(())
    }

    /// Transition process to BLOCKED state (e.g., resource exhausted).
    pub fn block(&mut self, pid: &str, reason: String) -> Result<()> {
        let pcb = self
            .processes
            .get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))?;

        if pcb.state != ProcessState::Running {
            return Err(Error::state_transition(format!(
                "cannot block pid {}: state is {:?}, expected Running",
                pid, pcb.state
            )));
        }

        pcb.block(reason);
        Ok(())
    }

    /// Resume process from WAITING/BLOCKED to READY.
    pub fn resume(&mut self, pid: &str) -> Result<()> {
        let pcb = self
            .processes
            .get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))?;

        if !matches!(pcb.state, ProcessState::Waiting | ProcessState::Blocked) {
            return Err(Error::state_transition(format!(
                "cannot resume pid {}: state is {:?}, expected Waiting or Blocked",
                pid, pcb.state
            )));
        }

        pcb.resume();

        // Re-add to ready queue
        self.ready_queue.push(PriorityItem {
            pid: pid.to_string(),
            priority: pcb.priority.to_heap_value(),
            created_at: pcb.created_at,
        });

        Ok(())
    }

    /// Terminate process.
    pub fn terminate(&mut self, pid: &str) -> Result<()> {
        let pcb = self
            .processes
            .get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))?;

        if pcb.state.is_terminal() {
            return Ok(()); // Already terminated
        }

        pcb.complete();
        Ok(())
    }

    /// Cleanup terminated process (transition to ZOMBIE).
    pub fn cleanup(&mut self, pid: &str) -> Result<()> {
        let pcb = self
            .processes
            .get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))?;

        if pcb.state != ProcessState::Terminated {
            return Err(Error::state_transition(format!(
                "cannot cleanup pid {}: state is {:?}, expected Terminated",
                pid, pcb.state
            )));
        }

        pcb.state = ProcessState::Zombie;
        Ok(())
    }

    /// Remove process completely (zombie collection).
    pub fn remove(&mut self, pid: &str) -> Result<ProcessControlBlock> {
        self.processes
            .remove(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))
    }

    /// Get process by PID.
    pub fn get(&self, pid: &str) -> Option<&ProcessControlBlock> {
        self.processes.get(pid)
    }

    /// Get mutable process by PID.
    pub fn get_mut(&mut self, pid: &str) -> Option<&mut ProcessControlBlock> {
        self.processes.get_mut(pid)
    }

    /// List all processes.
    pub fn list(&self) -> Vec<ProcessControlBlock> {
        self.processes.values().cloned().collect()
    }

    /// List processes by state.
    pub fn list_by_state(&self, state: ProcessState) -> Vec<ProcessControlBlock> {
        self.processes
            .values()
            .filter(|pcb| pcb.state == state)
            .cloned()
            .collect()
    }

    /// Count processes.
    pub fn count(&self) -> usize {
        self.processes.len()
    }

    /// Count processes by state.
    pub fn count_by_state(&self, state: ProcessState) -> usize {
        self.processes
            .values()
            .filter(|pcb| pcb.state == state)
            .count()
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        let mut lm = LifecycleManager::default();

        // Submit process
        let pcb = lm
            .submit(
                "pid1".to_string(),
                "req1".to_string(),
                "user1".to_string(),
                "sess1".to_string(),
                SchedulingPriority::Normal,
                None,
            )
            .unwrap();
        assert_eq!(pcb.state, ProcessState::New);

        // Schedule to READY
        lm.schedule("pid1").unwrap();
        assert_eq!(lm.get("pid1").unwrap().state, ProcessState::Ready);

        // Start to RUNNING
        lm.start("pid1").unwrap();
        assert_eq!(lm.get("pid1").unwrap().state, ProcessState::Running);

        // Terminate
        lm.terminate("pid1").unwrap();
        assert_eq!(lm.get("pid1").unwrap().state, ProcessState::Terminated);

        // Cleanup to ZOMBIE
        lm.cleanup("pid1").unwrap();
        assert_eq!(lm.get("pid1").unwrap().state, ProcessState::Zombie);
    }

    #[test]
    fn test_priority_queue() {
        let mut lm = LifecycleManager::default();

        // Submit 3 processes with different priorities
        lm.submit(
            "low".to_string(),
            "r1".to_string(),
            "u1".to_string(),
            "s1".to_string(),
            SchedulingPriority::Low,
            None,
        )
        .unwrap();
        lm.submit(
            "high".to_string(),
            "r2".to_string(),
            "u2".to_string(),
            "s2".to_string(),
            SchedulingPriority::High,
            None,
        )
        .unwrap();
        lm.submit(
            "normal".to_string(),
            "r3".to_string(),
            "u3".to_string(),
            "s3".to_string(),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();

        // Schedule all
        lm.schedule("low").unwrap();
        lm.schedule("high").unwrap();
        lm.schedule("normal").unwrap();

        // Should dequeue in priority order: high, normal, low
        let next = lm.get_next_runnable().unwrap();
        assert_eq!(next.pid, "high");

        let next = lm.get_next_runnable().unwrap();
        assert_eq!(next.pid, "normal");

        let next = lm.get_next_runnable().unwrap();
        assert_eq!(next.pid, "low");
    }

    #[test]
    fn test_state_validation() {
        assert!(ProcessState::New.can_transition_to(ProcessState::Ready));
        assert!(ProcessState::Ready.can_transition_to(ProcessState::Running));
        assert!(ProcessState::Running.can_transition_to(ProcessState::Waiting));
        assert!(ProcessState::Running.can_transition_to(ProcessState::Blocked));
        assert!(ProcessState::Running.can_transition_to(ProcessState::Terminated));
        assert!(ProcessState::Waiting.can_transition_to(ProcessState::Ready));
        assert!(ProcessState::Blocked.can_transition_to(ProcessState::Ready));
        assert!(ProcessState::Terminated.can_transition_to(ProcessState::Zombie));

        // Invalid transitions
        assert!(!ProcessState::New.can_transition_to(ProcessState::Running));
        assert!(!ProcessState::Ready.can_transition_to(ProcessState::Waiting));
        assert!(!ProcessState::Zombie.can_transition_to(ProcessState::Ready));
    }
}

