//! Periodic kernel cleanup — garbage collection of stale resources.
//!
//! Runs inside the kernel actor's select! loop (no separate service/lock).
//! Cleans: zombie processes, stale sessions, resolved interrupts, expired rate limits.

use crate::types::Result;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

/// Configuration for cleanup behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupConfig {
    /// How often to run cleanup (default: 5 minutes)
    pub interval_seconds: u64,
    /// How long to keep zombie processes (default: 24 hours)
    pub process_retention_seconds: i64,
    /// How long to keep stale sessions (default: 1 hour)
    pub session_retention_seconds: i64,
    /// How long to keep resolved interrupts (default: 24 hours)
    pub interrupt_retention_seconds: i64,
    /// Maximum user_usage entries to retain (default: 10,000).
    pub max_user_usage_entries: usize,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 300,              // 5 minutes
            process_retention_seconds: 86400,   // 24 hours
            session_retention_seconds: 3600,    // 1 hour
            interrupt_retention_seconds: 86400, // 24 hours
            max_user_usage_entries: 10_000,
        }
    }
}

/// Statistics from a cleanup cycle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanupStats {
    pub zombies_removed: usize,
    pub sessions_removed: usize,
    pub interrupts_removed: usize,
    pub rate_windows_cleaned: usize,
    pub envelopes_evicted: usize,
    pub user_usage_evicted: usize,
    pub completed_at: Option<chrono::DateTime<Utc>>,
}

/// Run a single cleanup cycle on the kernel.
///
/// Called from the kernel actor's select! loop with `&mut Kernel`.
pub fn run_cleanup_cycle(
    kernel: &mut crate::kernel::Kernel,
    config: &CleanupConfig,
) -> Result<CleanupStats> {
    let zombies_removed = kernel.lifecycle.cleanup_zombies(config.process_retention_seconds);
    let sessions_removed = kernel.cleanup_stale_sessions(config.session_retention_seconds);
    let interrupt_duration = Duration::seconds(config.interrupt_retention_seconds);
    let interrupts_removed = kernel.interrupts.cleanup_resolved(interrupt_duration);
    kernel.rate_limiter.cleanup_expired();
    let user_usage_evicted = kernel.cleanup_stale_user_usage(config.max_user_usage_entries);

    Ok(CleanupStats {
        zombies_removed,
        sessions_removed,
        interrupts_removed,
        rate_windows_cleaned: 0,
        envelopes_evicted: 0,
        user_usage_evicted,
        completed_at: Some(Utc::now()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::{Kernel, SchedulingPriority};
    use crate::types::{ProcessId, RequestId, SessionId, UserId};

    fn create_test_kernel() -> Kernel {
        Kernel::new()
    }

    #[test]
    fn test_cleanup_config_defaults() {
        let config = CleanupConfig::default();
        assert_eq!(config.interval_seconds, 300);
        assert_eq!(config.process_retention_seconds, 86400);
        assert_eq!(config.session_retention_seconds, 3600);
    }

    #[test]
    fn test_cleanup_zombies() {
        let mut kernel = create_test_kernel();

        let pcb = kernel
            .create_process(
                ProcessId::must("test1"),
                RequestId::must("req1"),
                UserId::must("user1"),
                SessionId::must("sess1"),
                SchedulingPriority::Normal,
                None,
            )
            .unwrap();

        kernel.terminate_process(&pcb.pid).unwrap();
        kernel.cleanup_process(&pcb.pid).unwrap();

        let pcb2 = kernel
            .create_process(
                ProcessId::must("test2"),
                RequestId::must("req2"),
                UserId::must("user1"),
                SessionId::must("sess1"),
                SchedulingPriority::Normal,
                None,
            )
            .unwrap();

        kernel.terminate_process(&pcb2.pid).unwrap();

        let config = CleanupConfig::default();
        let stats = run_cleanup_cycle(&mut kernel, &config).unwrap();
        assert_eq!(stats.zombies_removed, 0);
        assert!(stats.completed_at.is_some());
    }

    #[test]
    fn test_run_cleanup_cycle() {
        let mut kernel = create_test_kernel();
        let config = CleanupConfig::default();

        let stats = run_cleanup_cycle(&mut kernel, &config).unwrap();
        assert_eq!(stats.zombies_removed, 0);
        assert_eq!(stats.sessions_removed, 0);
        assert_eq!(stats.interrupts_removed, 0);
        assert!(stats.completed_at.is_some());
    }
}
