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
        if let Some(reason) = pcb.check_quota() {
            return Err(Error::quota_exceeded(format!(
                "Process {} quota exceeded: {}",
                pcb.pid, reason
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

