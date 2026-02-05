//! Resource tracking and quota enforcement.
//!
//! Tracks resource usage across processes and enforces quotas.

use std::collections::HashMap;

use super::types::{ProcessControlBlock, ResourceUsage};
use crate::types::{Error, Result};

/// Resource tracker - tracks usage across all processes.
///
/// NOT a separate actor - owned by Kernel and called via &mut self.
#[derive(Debug, Default)]
pub struct ResourceTracker {
    /// Per-user usage aggregation (optional, for multi-tenant quotas)
    user_usage: HashMap<String, ResourceUsage>,
}

impl ResourceTracker {
    pub fn new() -> Self {
        Self {
            user_usage: HashMap::new(),
        }
    }

    /// Check if process quota is exceeded.
    pub fn check_quota(&self, pcb: &ProcessControlBlock) -> Result<()> {
        if let Some(violation) = pcb.check_quota() {
            return Err(Error::quota_exceeded(format!(
                "Process {} quota exceeded: {}",
                pcb.pid, violation
            )));
        }
        Ok(())
    }

    /// Record resource usage for a process.
    pub fn record_usage(
        &mut self,
        user_id: &str,
        llm_calls: i32,
        tool_calls: i32,
        tokens_in: i64,
        tokens_out: i64,
    ) {
        let user_usage = self.user_usage.entry(user_id.to_string()).or_default();
        user_usage.llm_calls += llm_calls;
        user_usage.tool_calls += tool_calls;
        user_usage.tokens_in += tokens_in;
        user_usage.tokens_out += tokens_out;
    }

    /// Get usage for a user.
    pub fn get_user_usage(&self, user_id: &str) -> Option<&ResourceUsage> {
        self.user_usage.get(user_id)
    }

    /// Clear usage for a user (e.g., on quota reset).
    pub fn clear_user_usage(&mut self, user_id: &str) {
        self.user_usage.remove(user_id);
    }

    /// Get total usage across all users.
    pub fn total_usage(&self) -> ResourceUsage {
        let mut total = ResourceUsage::default();
        for usage in self.user_usage.values() {
            total.llm_calls += usage.llm_calls;
            total.tool_calls += usage.tool_calls;
            total.agent_hops += usage.agent_hops;
            total.iterations += usage.iterations;
            total.tokens_in += usage.tokens_in;
            total.tokens_out += usage.tokens_out;
            total.inference_requests += usage.inference_requests;
            total.inference_input_chars += usage.inference_input_chars;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::types::{ProcessControlBlock, ProcessState, ResourceQuota};
    use crate::types::{ProcessId, RequestId, SessionId, UserId};

    #[test]
    fn test_record_usage() {
        let mut tracker = ResourceTracker::new();
        tracker.record_usage("user1", 5, 10, 1000, 500);

        let usage = tracker.get_user_usage("user1").unwrap();
        assert_eq!(usage.llm_calls, 5);
        assert_eq!(usage.tool_calls, 10);
        assert_eq!(usage.tokens_in, 1000);
        assert_eq!(usage.tokens_out, 500);
    }

    #[test]
    fn test_check_quota_within_bounds() {
        let tracker = ResourceTracker::new();

        let mut pcb = ProcessControlBlock::new(
            ProcessId::must("env1"),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
        );
        pcb.state = ProcessState::Running;
        pcb.quota = ResourceQuota {
            max_llm_calls: 10,
            max_input_tokens: 10000,
            max_output_tokens: 5000,
            max_agent_hops: 5,
            max_iterations: 100,
            timeout_seconds: 300,
            ..ResourceQuota::default()
        };
        pcb.usage = ResourceUsage {
            llm_calls: 5,
            tokens_in: 5000,
            tokens_out: 2000,
            agent_hops: 2,
            iterations: 10,
            ..Default::default()
        };

        assert!(tracker.check_quota(&pcb).is_ok());
    }

    #[test]
    fn test_check_quota_exceeded_llm_calls() {
        let tracker = ResourceTracker::new();

        let mut pcb = ProcessControlBlock::new(
            ProcessId::must("env1"),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
        );
        pcb.state = ProcessState::Running;
        pcb.quota = ResourceQuota {
            max_llm_calls: 10,
            max_input_tokens: 10000,
            max_output_tokens: 5000,
            max_agent_hops: 5,
            max_iterations: 100,
            timeout_seconds: 300,
            ..ResourceQuota::default()
        };
        pcb.usage = ResourceUsage {
            llm_calls: 15, // Exceeds max_llm_calls
            tokens_in: 5000,
            tokens_out: 2000,
            agent_hops: 2,
            iterations: 10,
            ..Default::default()
        };

        let result = tracker.check_quota(&pcb);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("quota exceeded"));
    }

    #[test]
    fn test_check_quota_exceeded_tokens() {
        let tracker = ResourceTracker::new();

        let mut pcb = ProcessControlBlock::new(
            ProcessId::must("env1"),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
        );
        pcb.state = ProcessState::Running;
        pcb.quota = ResourceQuota {
            max_llm_calls: 10,
            max_input_tokens: 10000,
            max_output_tokens: 5000,
            max_agent_hops: 5,
            max_iterations: 100,
            timeout_seconds: 300,
            ..ResourceQuota::default()
        };
        pcb.usage = ResourceUsage {
            llm_calls: 5,
            tokens_in: 15000, // Exceeds max_input_tokens
            tokens_out: 2000,
            agent_hops: 2,
            iterations: 10,
            ..Default::default()
        };

        let result = tracker.check_quota(&pcb);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("quota exceeded"));
    }

    #[test]
    fn test_check_quota_exceeded_agent_hops() {
        let tracker = ResourceTracker::new();

        let mut pcb = ProcessControlBlock::new(
            ProcessId::must("env1"),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
        );
        pcb.state = ProcessState::Running;
        pcb.quota = ResourceQuota {
            max_llm_calls: 10,
            max_input_tokens: 10000,
            max_output_tokens: 5000,
            max_agent_hops: 5,
            max_iterations: 100,
            timeout_seconds: 300,
            ..ResourceQuota::default()
        };
        pcb.usage = ResourceUsage {
            llm_calls: 5,
            tokens_in: 5000,
            tokens_out: 2000,
            agent_hops: 10, // Exceeds max_agent_hops
            iterations: 10,
            ..Default::default()
        };

        let result = tracker.check_quota(&pcb);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("quota exceeded"));
    }

    #[test]
    fn test_per_user_aggregation() {
        let mut tracker = ResourceTracker::new();

        // Record usage for user1
        tracker.record_usage("user1", 5, 10, 1000, 500);
        tracker.record_usage("user1", 3, 5, 500, 250);

        // Record usage for user2
        tracker.record_usage("user2", 2, 4, 300, 150);

        let user1_usage = tracker.get_user_usage("user1").unwrap();
        assert_eq!(user1_usage.llm_calls, 8);
        assert_eq!(user1_usage.tool_calls, 15);
        assert_eq!(user1_usage.tokens_in, 1500);
        assert_eq!(user1_usage.tokens_out, 750);

        let user2_usage = tracker.get_user_usage("user2").unwrap();
        assert_eq!(user2_usage.llm_calls, 2);
        assert_eq!(user2_usage.tool_calls, 4);
        assert_eq!(user2_usage.tokens_in, 300);
        assert_eq!(user2_usage.tokens_out, 150);
    }

    #[test]
    fn test_system_usage_totals() {
        let mut tracker = ResourceTracker::new();

        tracker.record_usage("user1", 5, 10, 1000, 500);
        tracker.record_usage("user2", 3, 6, 600, 300);
        tracker.record_usage("user3", 2, 4, 400, 200);

        let total = tracker.total_usage();
        assert_eq!(total.llm_calls, 10);
        assert_eq!(total.tool_calls, 20);
        assert_eq!(total.tokens_in, 2000);
        assert_eq!(total.tokens_out, 1000);
    }

    #[test]
    fn test_clear_user_usage() {
        let mut tracker = ResourceTracker::new();

        tracker.record_usage("user1", 5, 10, 1000, 500);
        assert!(tracker.get_user_usage("user1").is_some());

        tracker.clear_user_usage("user1");
        assert!(tracker.get_user_usage("user1").is_none());

        // Total should be zero after clearing only user
        let total = tracker.total_usage();
        assert_eq!(total.llm_calls, 0);
    }
}

