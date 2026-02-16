//! Process lifecycle management.
//!
//! Implements Unix-like process state machine with priority scheduling:
//! NEW → READY → RUNNING → {WAITING|BLOCKED|TERMINATED} → ZOMBIE

use chrono::{DateTime, Utc};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::types::{Error, ProcessId, RequestId, Result, SessionId, UserId};

pub use super::types::{ProcessControlBlock, ProcessState, ResourceQuota, SchedulingPriority};

/// Priority queue item (wraps for min-heap behavior).
#[derive(Debug, Clone, PartialEq, Eq)]
struct PriorityItem {
    pid: ProcessId,
    priority: i32,             // Lower = higher priority
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
    pub(crate) processes: HashMap<ProcessId, ProcessControlBlock>,
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
        pid: ProcessId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
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
    pub fn schedule(&mut self, pid: &ProcessId) -> Result<()> {
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
            pid: pid.clone(),
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
    pub fn start(&mut self, pid: &ProcessId) -> Result<()> {
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
    pub fn wait(
        &mut self,
        pid: &ProcessId,
        interrupt_kind: crate::envelope::InterruptKind,
    ) -> Result<()> {
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
    pub fn block(&mut self, pid: &ProcessId, reason: String) -> Result<()> {
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
    pub fn resume(&mut self, pid: &ProcessId) -> Result<()> {
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
            pid: pid.clone(),
            priority: pcb.priority.to_heap_value(),
            created_at: pcb.created_at,
        });

        Ok(())
    }

    /// Terminate process.
    pub fn terminate(&mut self, pid: &ProcessId) -> Result<()> {
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
    pub fn cleanup(&mut self, pid: &ProcessId) -> Result<()> {
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
    pub fn remove(&mut self, pid: &ProcessId) -> Result<ProcessControlBlock> {
        self.processes
            .remove(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))
    }

    /// Get process by PID.
    pub fn get(&self, pid: &ProcessId) -> Option<&ProcessControlBlock> {
        self.processes.get(pid)
    }

    /// Get mutable process by PID.
    pub fn get_mut(&mut self, pid: &ProcessId) -> Option<&mut ProcessControlBlock> {
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

    /// Get the current default quota.
    pub fn get_default_quota(&self) -> &ResourceQuota {
        &self.default_quota
    }

    /// Set (merge) default quota. Only non-zero fields overwrite.
    pub fn set_default_quota(&mut self, overrides: &ResourceQuota) {
        let q = &mut self.default_quota;
        if overrides.max_llm_calls > 0 {
            q.max_llm_calls = overrides.max_llm_calls;
        }
        if overrides.max_tool_calls > 0 {
            q.max_tool_calls = overrides.max_tool_calls;
        }
        if overrides.max_agent_hops > 0 {
            q.max_agent_hops = overrides.max_agent_hops;
        }
        if overrides.max_iterations > 0 {
            q.max_iterations = overrides.max_iterations;
        }
        if overrides.timeout_seconds > 0 {
            q.timeout_seconds = overrides.timeout_seconds;
        }
        if overrides.soft_timeout_seconds > 0 {
            q.soft_timeout_seconds = overrides.soft_timeout_seconds;
        }
        if overrides.max_input_tokens > 0 {
            q.max_input_tokens = overrides.max_input_tokens;
        }
        if overrides.max_output_tokens > 0 {
            q.max_output_tokens = overrides.max_output_tokens;
        }
        if overrides.max_context_tokens > 0 {
            q.max_context_tokens = overrides.max_context_tokens;
        }
        if overrides.rate_limit_rpm > 0 {
            q.rate_limit_rpm = overrides.rate_limit_rpm;
        }
        if overrides.rate_limit_rph > 0 {
            q.rate_limit_rph = overrides.rate_limit_rph;
        }
        if overrides.rate_limit_burst > 0 {
            q.rate_limit_burst = overrides.rate_limit_burst;
        }
        if overrides.max_inference_requests > 0 {
            q.max_inference_requests = overrides.max_inference_requests;
        }
        if overrides.max_inference_input_chars > 0 {
            q.max_inference_input_chars = overrides.max_inference_input_chars;
        }
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
        let pid = ProcessId::must("pid1");

        let pcb = lm
            .submit(
                pid.clone(),
                RequestId::must("req1"),
                UserId::must("user1"),
                SessionId::must("sess1"),
                SchedulingPriority::Normal,
                None,
            )
            .unwrap();
        assert_eq!(pcb.state, ProcessState::New);

        lm.schedule(&pid).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Ready);

        lm.start(&pid).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Running);

        lm.terminate(&pid).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Terminated);

        lm.cleanup(&pid).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Zombie);
    }

    #[test]
    fn test_priority_queue() {
        let mut lm = LifecycleManager::default();

        let pid_low = ProcessId::must("low");
        let pid_high = ProcessId::must("high");
        let pid_normal = ProcessId::must("normal");

        lm.submit(
            pid_low.clone(),
            RequestId::must("r1"),
            UserId::must("u1"),
            SessionId::must("s1"),
            SchedulingPriority::Low,
            None,
        )
        .unwrap();
        lm.submit(
            pid_high.clone(),
            RequestId::must("r2"),
            UserId::must("u2"),
            SessionId::must("s2"),
            SchedulingPriority::High,
            None,
        )
        .unwrap();
        lm.submit(
            pid_normal.clone(),
            RequestId::must("r3"),
            UserId::must("u3"),
            SessionId::must("s3"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();

        lm.schedule(&pid_low).unwrap();
        lm.schedule(&pid_high).unwrap();
        lm.schedule(&pid_normal).unwrap();

        let next = lm.get_next_runnable().unwrap();
        assert_eq!(next.pid.as_str(), "high");

        let next = lm.get_next_runnable().unwrap();
        assert_eq!(next.pid.as_str(), "normal");

        let next = lm.get_next_runnable().unwrap();
        assert_eq!(next.pid.as_str(), "low");
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

        assert!(!ProcessState::New.can_transition_to(ProcessState::Running));
        assert!(!ProcessState::Ready.can_transition_to(ProcessState::Waiting));
        assert!(!ProcessState::Zombie.can_transition_to(ProcessState::Ready));
    }

    #[test]
    fn test_submit_duplicate_pid_returns_existing() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("pid1");

        let pcb1 = lm
            .submit(
                pid.clone(),
                RequestId::must("req1"),
                UserId::must("user1"),
                SessionId::must("sess1"),
                SchedulingPriority::Normal,
                None,
            )
            .unwrap();

        let pcb2 = lm
            .submit(
                pid,
                RequestId::must("req2"),
                UserId::must("user2"),
                SessionId::must("sess2"),
                SchedulingPriority::High,
                None,
            )
            .unwrap();

        assert_eq!(pcb1.pid, pcb2.pid);
        assert_eq!(pcb2.request_id.as_str(), "req1");
    }

    #[test]
    fn test_transition_invalid_state_fails() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("pid1");

        lm.submit(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();

        assert!(lm.start(&pid).is_err());

        lm.schedule(&pid).unwrap();

        assert!(lm
            .wait(&pid, crate::envelope::InterruptKind::Clarification)
            .is_err());
    }

    #[test]
    fn test_get_next_runnable_empty_queue() {
        let mut lm = LifecycleManager::default();
        assert!(lm.get_next_runnable().is_none());
    }

    #[test]
    fn test_count_by_state() {
        let mut lm = LifecycleManager::default();

        let pid1 = ProcessId::must("pid1");
        let pid2 = ProcessId::must("pid2");
        let pid3 = ProcessId::must("pid3");

        lm.submit(
            pid1.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();
        lm.submit(
            pid2.clone(),
            RequestId::must("req2"),
            UserId::must("user2"),
            SessionId::must("sess2"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();
        lm.submit(
            pid3.clone(),
            RequestId::must("req3"),
            UserId::must("user3"),
            SessionId::must("sess3"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();

        assert_eq!(lm.count_by_state(ProcessState::New), 3);
        assert_eq!(lm.count_by_state(ProcessState::Ready), 0);

        lm.schedule(&pid1).unwrap();
        lm.schedule(&pid2).unwrap();

        assert_eq!(lm.count_by_state(ProcessState::New), 1);
        assert_eq!(lm.count_by_state(ProcessState::Ready), 2);
    }

    #[test]
    fn test_wait_and_resume() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("pid1");

        lm.submit(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();
        lm.schedule(&pid).unwrap();
        lm.start(&pid).unwrap();

        lm.wait(&pid, crate::envelope::InterruptKind::Clarification)
            .unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Waiting);

        lm.resume(&pid).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Ready);

        let next = lm.get_next_runnable().unwrap();
        assert_eq!(next.pid.as_str(), "pid1");
    }

    #[test]
    fn test_block_and_resume() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("pid1");

        lm.submit(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();
        lm.schedule(&pid).unwrap();
        lm.start(&pid).unwrap();

        lm.block(&pid, "quota exceeded".to_string()).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Blocked);

        lm.resume(&pid).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Ready);

        let next = lm.get_next_runnable().unwrap();
        assert_eq!(next.pid.as_str(), "pid1");
    }

    #[test]
    fn test_process_not_found_errors() {
        let mut lm = LifecycleManager::default();
        let nonexistent = ProcessId::must("nonexistent");

        assert!(lm.schedule(&nonexistent).is_err());
        assert!(lm.start(&nonexistent).is_err());
        assert!(lm
            .wait(&nonexistent, crate::envelope::InterruptKind::Clarification)
            .is_err());
        assert!(lm.block(&nonexistent, "reason".to_string()).is_err());
        assert!(lm.resume(&nonexistent).is_err());
        assert!(lm.terminate(&nonexistent).is_err());
        assert!(lm.cleanup(&nonexistent).is_err());
        assert!(lm.remove(&nonexistent).is_err());

        assert!(lm.get(&nonexistent).is_none());
    }

    #[test]
    fn test_cleanup_and_remove() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("pid1");

        lm.submit(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();
        lm.schedule(&pid).unwrap();
        lm.start(&pid).unwrap();
        lm.terminate(&pid).unwrap();

        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Terminated);
        assert_eq!(lm.count(), 1);

        lm.cleanup(&pid).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Zombie);
        assert_eq!(lm.count(), 1);

        let removed = lm.remove(&pid).unwrap();
        assert_eq!(removed.pid.as_str(), "pid1");
        assert_eq!(lm.count(), 0);
        assert!(lm.get(&pid).is_none());
    }

    #[test]
    fn test_schedule_wrong_state_fails() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("pid1");

        lm.submit(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();
        lm.schedule(&pid).unwrap();

        assert!(lm.schedule(&pid).is_err());
    }

    #[test]
    fn test_list_by_state() {
        let mut lm = LifecycleManager::default();

        let pid1 = ProcessId::must("pid1");
        let pid2 = ProcessId::must("pid2");
        let pid3 = ProcessId::must("pid3");

        lm.submit(
            pid1.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();
        lm.submit(
            pid2.clone(),
            RequestId::must("req2"),
            UserId::must("user2"),
            SessionId::must("sess2"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();
        lm.submit(
            pid3.clone(),
            RequestId::must("req3"),
            UserId::must("user3"),
            SessionId::must("sess3"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();

        let new_processes = lm.list_by_state(ProcessState::New);
        assert_eq!(new_processes.len(), 3);

        lm.schedule(&pid1).unwrap();
        lm.schedule(&pid2).unwrap();

        let ready_processes = lm.list_by_state(ProcessState::Ready);
        assert_eq!(ready_processes.len(), 2);

        let new_processes = lm.list_by_state(ProcessState::New);
        assert_eq!(new_processes.len(), 1);
    }

    #[test]
    fn test_remove_process() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("pid1");

        lm.submit(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();

        assert_eq!(lm.count(), 1);

        let removed = lm.remove(&pid).unwrap();
        assert_eq!(removed.pid.as_str(), "pid1");

        assert_eq!(lm.count(), 0);
        assert!(lm.get(&pid).is_none());
    }

    #[test]
    fn test_terminate_idempotent() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("pid1");

        lm.submit(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        )
        .unwrap();
        lm.schedule(&pid).unwrap();
        lm.start(&pid).unwrap();

        lm.terminate(&pid).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Terminated);

        assert!(lm.terminate(&pid).is_ok());
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Terminated);
    }

    #[test]
    fn test_get_default_quota_returns_resource_quota_default() {
        let lm = LifecycleManager::default();
        let got = lm.get_default_quota();
        let expected = ResourceQuota::default_quota();
        assert_eq!(*got, expected);
    }

    #[test]
    fn test_set_default_quota_merges_nonzero_fields() {
        let mut lm = LifecycleManager::default();

        let overrides = ResourceQuota {
            max_llm_calls: 200,
            max_tool_calls: 0,
            max_agent_hops: 0,
            max_iterations: 0,
            timeout_seconds: 0,
            soft_timeout_seconds: 0,
            max_input_tokens: 0,
            max_output_tokens: 0,
            max_context_tokens: 0,
            rate_limit_rpm: 0,
            rate_limit_rph: 0,
            rate_limit_burst: 0,
            max_inference_requests: 0,
            max_inference_input_chars: 0,
        };

        lm.set_default_quota(&overrides);

        let q = lm.get_default_quota();
        assert_eq!(q.max_llm_calls, 200);
        assert_eq!(q.max_tool_calls, 50); // unchanged default
    }

    #[test]
    fn test_set_default_quota_skips_zero_fields() {
        let mut lm = LifecycleManager::default();
        let expected = ResourceQuota::default_quota();

        let all_zeros = ResourceQuota {
            max_llm_calls: 0,
            max_tool_calls: 0,
            max_agent_hops: 0,
            max_iterations: 0,
            timeout_seconds: 0,
            soft_timeout_seconds: 0,
            max_input_tokens: 0,
            max_output_tokens: 0,
            max_context_tokens: 0,
            rate_limit_rpm: 0,
            rate_limit_rph: 0,
            rate_limit_burst: 0,
            max_inference_requests: 0,
            max_inference_input_chars: 0,
        };

        lm.set_default_quota(&all_zeros);

        assert_eq!(*lm.get_default_quota(), expected);
    }
}
