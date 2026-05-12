//! Run lifecycle management.
//!
//! Three-state machine: `Ready → Running → Terminated`. Runs that pause on a
//! tool-confirmation interrupt stay in `Running`; the kernel doesn't have a
//! dedicated waiting/blocked state for that case (the pending interrupt ID
//! lives on `RunRecord::pending_interrupt`).

use std::collections::HashMap;

use crate::types::{Error, RunId, RequestId, Result, SessionId, UserId};

pub use super::types::{RunRecord, RunStatus, ResourceQuota};

/// Lifecycle manager — owns the run-record map and quota defaults.
///
/// Not a separate actor; held by `Kernel` and accessed via `&mut self`.
#[derive(Debug)]
pub struct RunRegistry {
    default_quota: ResourceQuota,
    pub(crate) records: HashMap<RunId, RunRecord>,
}

impl RunRegistry {
    pub fn new(default_quota: Option<ResourceQuota>) -> Self {
        Self {
            default_quota: default_quota.unwrap_or_default(),
            records: HashMap::new(),
        }
    }

    /// Create a new run record in `Ready` state. If a record already exists
    /// for the run_id, returns the existing one unchanged.
    pub fn create(
        &mut self,
        run_id: RunId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
        quota: Option<ResourceQuota>,
    ) -> Result<RunRecord> {
        if let Some(existing) = self.records.get(&run_id) {
            return Ok(existing.clone());
        }
        let mut record = RunRecord::new(run_id.clone(), request_id, user_id, session_id);
        record.quota = quota.unwrap_or_else(|| self.default_quota.clone());
        self.records.insert(run_id, record.clone());
        Ok(record)
    }

    /// Transition `Ready → Running`.
    pub fn run(&mut self, run_id: &RunId) -> Result<()> {
        let record = self.records.get_mut(run_id)
            .ok_or_else(|| Error::not_found(format!("unknown run_id: {}", run_id)))?;
        if record.state != RunStatus::Ready {
            return Err(Error::state_transition(format!(
                "cannot run {}: state is {:?}, expected Ready",
                run_id, record.state
            )));
        }
        record.start();
        Ok(())
    }

    /// Terminate a run and remove its record from the map.
    /// Idempotent: if the run_id is unknown, returns Ok(()).
    pub fn terminate(&mut self, run_id: &RunId) -> Result<()> {
        if let Some(record) = self.records.get_mut(run_id) {
            if !record.state.is_terminal() {
                record.complete();
            }
        }
        self.records.remove(run_id);
        Ok(())
    }

    /// Get run record by ID.
    pub fn get(&self, run_id: &RunId) -> Option<&RunRecord> {
        self.records.get(run_id)
    }

    /// Get mutable run record by ID.
    pub fn get_mut(&mut self, run_id: &RunId) -> Option<&mut RunRecord> {
        self.records.get_mut(run_id)
    }

    /// List all run records.
    pub fn list(&self) -> Vec<RunRecord> {
        self.records.values().cloned().collect()
    }

    /// Count run records.
    pub fn count(&self) -> usize {
        self.records.len()
    }

    /// Count records in a given state.
    pub fn count_by_state(&self, state: RunStatus) -> usize {
        self.records.values().filter(|r| r.state == state).count()
    }

    /// Get the current default quota.
    pub fn get_default_quota(&self) -> &ResourceQuota {
        &self.default_quota
    }

    /// User IDs that have non-terminated runs.
    pub fn get_active_user_ids(&self) -> std::collections::HashSet<String> {
        self.records
            .values()
            .filter(|r| !r.state.is_terminal())
            .map(|r| r.user_id.to_string())
            .collect()
    }
}

impl Default for RunRegistry {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submit(lm: &mut RunRegistry, run_id: &str) -> RunRecord {
        lm.create(
            RunId::must(run_id),
            RequestId::must(format!("req-{}", run_id)),
            UserId::must(format!("user-{}", run_id)),
            SessionId::must(format!("sess-{}", run_id)),
            None,
        ).unwrap()
    }

    #[test]
    fn ready_run_terminate() {
        let mut lm = RunRegistry::default();
        let run_id = RunId::must("p1");
        let record = submit(&mut lm, "p1");
        assert_eq!(record.state, RunStatus::Ready);

        lm.run(&run_id).unwrap();
        assert_eq!(lm.get(&run_id).unwrap().state, RunStatus::Running);

        lm.terminate(&run_id).unwrap();
        assert!(lm.get(&run_id).is_none(), "terminate removes the record immediately");
    }

    #[test]
    fn create_duplicate_returns_existing() {
        let mut lm = RunRegistry::default();
        let _first = submit(&mut lm, "p1");
        let again = lm.create(
            RunId::must("p1"),
            RequestId::must("other-req"),
            UserId::must("other-user"),
            SessionId::must("other-sess"),
            None,
        ).unwrap();
        assert_eq!(again.request_id.as_str(), "req-p1", "duplicate create returns the original");
    }

    #[test]
    fn run_invalid_state_fails() {
        let mut lm = RunRegistry::default();
        let run_id = RunId::must("p1");
        submit(&mut lm, "p1");
        lm.run(&run_id).unwrap();
        assert!(lm.run(&run_id).is_err(), "cannot run a Running run");
    }

    #[test]
    fn terminate_idempotent_on_missing() {
        let mut lm = RunRegistry::default();
        assert!(lm.terminate(&RunId::must("nonexistent")).is_ok());
    }

    #[test]
    fn count_and_list() {
        let mut lm = RunRegistry::default();
        submit(&mut lm, "a");
        submit(&mut lm, "b");
        submit(&mut lm, "c");
        assert_eq!(lm.count(), 3);
        assert_eq!(lm.count_by_state(RunStatus::Ready), 3);
        lm.run(&RunId::must("a")).unwrap();
        assert_eq!(lm.count_by_state(RunStatus::Ready), 2);
        assert_eq!(lm.count_by_state(RunStatus::Running), 1);
    }

    #[test]
    fn active_user_ids_excludes_terminated() {
        let mut lm = RunRegistry::default();
        submit(&mut lm, "x");
        submit(&mut lm, "y");
        let run_x = RunId::must("x");
        lm.run(&run_x).unwrap();
        lm.terminate(&run_x).unwrap();
        let users = lm.get_active_user_ids();
        assert!(users.contains("user-y"));
        assert!(!users.contains("user-x"), "terminated user excluded");
    }
}
