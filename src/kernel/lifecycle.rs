//! Process lifecycle management.
//!
//! Three-state machine: `Ready → Running → Terminated`. Processes that pause
//! on a tool-confirmation interrupt stay in `Running`; the kernel doesn't
//! have a dedicated waiting/blocked state for that case (the pending interrupt
//! ID lives on `ProcessControlBlock::pending_interrupt`).

use std::collections::HashMap;

use crate::types::{Error, ProcessId, RequestId, Result, SessionId, UserId};

pub use super::types::{ProcessControlBlock, ProcessState, ResourceQuota};

/// Lifecycle manager — owns the process map and quota defaults.
///
/// Not a separate actor; held by `Kernel` and accessed via `&mut self`.
#[derive(Debug)]
pub struct LifecycleManager {
    default_quota: ResourceQuota,
    pub(crate) processes: HashMap<ProcessId, ProcessControlBlock>,
}

impl LifecycleManager {
    pub fn new(default_quota: Option<ResourceQuota>) -> Self {
        Self {
            default_quota: default_quota.unwrap_or_default(),
            processes: HashMap::new(),
        }
    }

    /// Create a new process in `Ready` state. If a PCB already exists for the
    /// pid, returns the existing one unchanged.
    pub fn create(
        &mut self,
        pid: ProcessId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
        quota: Option<ResourceQuota>,
    ) -> Result<ProcessControlBlock> {
        if let Some(existing) = self.processes.get(&pid) {
            return Ok(existing.clone());
        }
        let mut pcb = ProcessControlBlock::new(pid.clone(), request_id, user_id, session_id);
        pcb.quota = quota.unwrap_or_else(|| self.default_quota.clone());
        self.processes.insert(pid, pcb.clone());
        Ok(pcb)
    }

    /// Transition `Ready → Running`.
    pub fn run(&mut self, pid: &ProcessId) -> Result<()> {
        let pcb = self.processes.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("unknown pid: {}", pid)))?;
        if pcb.state != ProcessState::Ready {
            return Err(Error::state_transition(format!(
                "cannot run pid {}: state is {:?}, expected Ready",
                pid, pcb.state
            )));
        }
        pcb.start();
        Ok(())
    }

    /// Terminate a process and remove it from the map.
    /// Idempotent: if the pid is unknown, returns Ok(()).
    pub fn terminate(&mut self, pid: &ProcessId) -> Result<()> {
        if let Some(pcb) = self.processes.get_mut(pid) {
            if !pcb.state.is_terminal() {
                pcb.complete();
            }
        }
        self.processes.remove(pid);
        Ok(())
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

    /// Count processes.
    pub fn count(&self) -> usize {
        self.processes.len()
    }

    /// Count processes in a given state.
    pub fn count_by_state(&self, state: ProcessState) -> usize {
        self.processes.values().filter(|pcb| pcb.state == state).count()
    }

    /// Get the current default quota.
    pub fn get_default_quota(&self) -> &ResourceQuota {
        &self.default_quota
    }

    /// User IDs that have non-terminated processes.
    pub fn get_active_user_ids(&self) -> std::collections::HashSet<String> {
        self.processes
            .values()
            .filter(|pcb| !pcb.state.is_terminal())
            .map(|pcb| pcb.user_id.to_string())
            .collect()
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

    fn submit(lm: &mut LifecycleManager, pid: &str) -> ProcessControlBlock {
        lm.create(
            ProcessId::must(pid),
            RequestId::must(format!("req-{}", pid)),
            UserId::must(format!("user-{}", pid)),
            SessionId::must(format!("sess-{}", pid)),
            None,
        ).unwrap()
    }

    #[test]
    fn ready_run_terminate() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("p1");
        let pcb = submit(&mut lm, "p1");
        assert_eq!(pcb.state, ProcessState::Ready);

        lm.run(&pid).unwrap();
        assert_eq!(lm.get(&pid).unwrap().state, ProcessState::Running);

        lm.terminate(&pid).unwrap();
        assert!(lm.get(&pid).is_none(), "terminate removes the PCB immediately");
    }

    #[test]
    fn create_duplicate_returns_existing() {
        let mut lm = LifecycleManager::default();
        let _first = submit(&mut lm, "p1");
        let again = lm.create(
            ProcessId::must("p1"),
            RequestId::must("other-req"),
            UserId::must("other-user"),
            SessionId::must("other-sess"),
            None,
        ).unwrap();
        assert_eq!(again.request_id.as_str(), "req-p1", "duplicate create returns the original");
    }

    #[test]
    fn run_invalid_state_fails() {
        let mut lm = LifecycleManager::default();
        let pid = ProcessId::must("p1");
        submit(&mut lm, "p1");
        lm.run(&pid).unwrap();
        assert!(lm.run(&pid).is_err(), "cannot run a Running process");
    }

    #[test]
    fn terminate_idempotent_on_missing() {
        let mut lm = LifecycleManager::default();
        assert!(lm.terminate(&ProcessId::must("nonexistent")).is_ok());
    }

    #[test]
    fn count_and_list() {
        let mut lm = LifecycleManager::default();
        submit(&mut lm, "a");
        submit(&mut lm, "b");
        submit(&mut lm, "c");
        assert_eq!(lm.count(), 3);
        assert_eq!(lm.count_by_state(ProcessState::Ready), 3);
        lm.run(&ProcessId::must("a")).unwrap();
        assert_eq!(lm.count_by_state(ProcessState::Ready), 2);
        assert_eq!(lm.count_by_state(ProcessState::Running), 1);
    }

    #[test]
    fn active_user_ids_excludes_terminated() {
        let mut lm = LifecycleManager::default();
        submit(&mut lm, "x");
        submit(&mut lm, "y");
        let pid_x = ProcessId::must("x");
        lm.run(&pid_x).unwrap();
        lm.terminate(&pid_x).unwrap();
        let users = lm.get_active_user_ids();
        assert!(users.contains("user-y"));
        assert!(!users.contains("user-x"), "terminated user excluded");
    }
}
