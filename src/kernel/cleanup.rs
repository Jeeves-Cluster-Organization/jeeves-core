//! Background cleanup for resource management.
//!
//! CleanupService provides periodic garbage collection of:
//! - Zombie processes (terminated but not removed)
//! - Stale orchestration sessions (expired sessions)
//! - Resolved interrupts (old interrupt records)
//! - Expired rate limit windows (old tracking data)
//!
//! This prevents memory leaks in long-running production deployments.

use crate::kernel::types::ProcessState;
use crate::types::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration as TokioDuration};

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
    /// Entries for users with no active processes are evicted first.
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
    /// Number of zombie processes removed
    pub zombies_removed: usize,
    /// Number of stale sessions removed
    pub sessions_removed: usize,
    /// Number of resolved interrupts removed
    pub interrupts_removed: usize,
    /// Number of rate limit windows cleaned
    pub rate_windows_cleaned: usize,
    /// Number of stale envelopes evicted
    pub envelopes_evicted: usize,
    /// Number of stale user_usage entries evicted
    pub user_usage_evicted: usize,
    /// When cleanup cycle completed
    pub completed_at: Option<DateTime<Utc>>,
}

/// CleanupService handles background garbage collection.
#[derive(Debug)]
pub struct CleanupService {
    kernel: Arc<Mutex<crate::kernel::Kernel>>,
    config: CleanupConfig,
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl CleanupService {
    /// Create a new cleanup service.
    pub fn new(kernel: Arc<Mutex<crate::kernel::Kernel>>, config: CleanupConfig) -> Self {
        Self {
            kernel,
            config,
            stop_tx: None,
        }
    }

    /// Start the cleanup loop in the background.
    /// Returns immediately; cleanup runs in a spawned task.
    pub fn start(&mut self) -> tokio::task::JoinHandle<()> {
        let kernel = self.kernel.clone();
        let config = self.config.clone();
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
        self.stop_tx = Some(stop_tx);

        tokio::spawn(async move {
            let mut ticker = interval(TokioDuration::from_secs(config.interval_seconds));

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(e) = Self::run_cleanup_cycle_async(&kernel, &config).await {
                            tracing::error!("cleanup_cycle_failed: {}", e);
                        }
                    }
                    _ = &mut stop_rx => {
                        tracing::info!("cleanup_service_stopped");
                        break;
                    }
                }
            }
        })
    }

    /// Stop the cleanup loop.
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Run a single cleanup cycle (async version for background task).
    ///
    /// Releases the kernel mutex between phases so IPC handlers aren't
    /// blocked for the entire cleanup duration.
    async fn run_cleanup_cycle_async(
        kernel: &Arc<Mutex<crate::kernel::Kernel>>,
        config: &CleanupConfig,
    ) -> Result<CleanupStats> {
        let mut stats = CleanupStats::default();

        // Phase 1: Zombie processes
        {
            let mut k = kernel.lock().await;
            stats.zombies_removed = Self::cleanup_zombies(&mut k, config.process_retention_seconds)?;
        }

        // Phase 2: Stale sessions (also removes associated process envelopes)
        {
            let mut k = kernel.lock().await;
            let removed_pids = k
                .orchestrator
                .cleanup_stale_sessions(config.session_retention_seconds);
            stats.sessions_removed = removed_pids.len();
            for pid in &removed_pids {
                k.process_envelopes.remove(pid);
            }
        }

        // Phase 3: Resolved interrupts
        {
            let mut k = kernel.lock().await;
            let interrupt_duration = Duration::seconds(config.interrupt_retention_seconds);
            stats.interrupts_removed = k.interrupts.cleanup_resolved(interrupt_duration);
        }

        // Phase 4: Rate limit windows + user usage
        {
            let mut k = kernel.lock().await;
            k.rate_limiter.cleanup_expired();

            let active_user_ids: std::collections::HashSet<String> = k
                .lifecycle
                .processes
                .values()
                .filter(|pcb| !matches!(pcb.state, ProcessState::Terminated | ProcessState::Zombie))
                .map(|pcb| pcb.user_id.to_string())
                .collect();
            stats.user_usage_evicted = k
                .resources
                .cleanup_stale_users(&active_user_ids, config.max_user_usage_entries);
        }

        tracing::debug!(
            "cleanup_cycle_completed: zombies={}, sessions={}, interrupts={}, envelopes={}, user_usage={}",
            stats.zombies_removed,
            stats.sessions_removed,
            stats.interrupts_removed,
            stats.envelopes_evicted,
            stats.user_usage_evicted,
        );

        stats.completed_at = Some(Utc::now());
        Ok(stats)
    }

    /// Run a single cleanup cycle (synchronous version).
    pub fn run_cleanup_cycle_sync(
        kernel: &mut crate::kernel::Kernel,
        config: &CleanupConfig,
    ) -> Result<CleanupStats> {
        // Clean up zombie processes
        let zombies_removed = Self::cleanup_zombies(kernel, config.process_retention_seconds)?;

        // Clean up stale sessions (also removes associated process envelopes)
        let removed_pids = kernel
            .orchestrator
            .cleanup_stale_sessions(config.session_retention_seconds);
        let sessions_removed = removed_pids.len();
        for pid in &removed_pids {
            kernel.process_envelopes.remove(pid);
        }

        // Clean up resolved interrupts
        let interrupt_duration = Duration::seconds(config.interrupt_retention_seconds);
        let interrupts_removed = kernel.interrupts.cleanup_resolved(interrupt_duration);

        // Clean up rate limit windows
        kernel.rate_limiter.cleanup_expired();

        // Evict stale user_usage entries (users with no active processes)
        let active_user_ids: std::collections::HashSet<String> = kernel
            .lifecycle
            .processes
            .values()
            .filter(|pcb| !matches!(pcb.state, ProcessState::Terminated | ProcessState::Zombie))
            .map(|pcb| pcb.user_id.to_string())
            .collect();
        let user_usage_evicted = kernel
            .resources
            .cleanup_stale_users(&active_user_ids, config.max_user_usage_entries);

        tracing::debug!(
            "cleanup_cycle_completed: zombies={}, sessions={}, interrupts={}, user_usage={}",
            zombies_removed,
            sessions_removed,
            interrupts_removed,
            user_usage_evicted,
        );

        Ok(CleanupStats {
            zombies_removed,
            sessions_removed,
            interrupts_removed,
            rate_windows_cleaned: 0, // RateLimiter doesn't report count
            envelopes_evicted: 0,
            user_usage_evicted,
            completed_at: Some(Utc::now()),
        })
    }

    /// Clean up zombie processes older than retention threshold.
    fn cleanup_zombies(kernel: &mut crate::kernel::Kernel, max_age_seconds: i64) -> Result<usize> {
        let cutoff = Utc::now() - Duration::seconds(max_age_seconds);
        let mut to_remove = Vec::new();

        // Find zombies older than cutoff
        for (pid, pcb) in &kernel.lifecycle.processes {
            if pcb.state == ProcessState::Zombie {
                if let Some(completed_at) = pcb.completed_at {
                    if completed_at < cutoff {
                        to_remove.push(pid.clone());
                    }
                }
            }
        }

        let count = to_remove.len();

        // Remove them
        for pid in to_remove {
            if let Err(e) = kernel.lifecycle.remove(&pid) {
                tracing::warn!("failed_to_remove_zombie: pid={}, error={}", pid, e);
            }
        }

        Ok(count)
    }
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

        // Create a process and terminate it
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
        kernel.lifecycle.cleanup(&pcb.pid).unwrap(); // Moves to Zombie

        // Verify it's a zombie
        let zombie = kernel.get_process(&pcb.pid).unwrap();
        assert_eq!(zombie.state, ProcessState::Zombie);

        // Set completed_at to old timestamp (simulate aging)
        {
            let zombie_pcb = kernel.lifecycle.processes.get_mut(&pcb.pid).unwrap();
            zombie_pcb.completed_at = Some(Utc::now() - Duration::hours(25));
        }

        // Run cleanup with 24h retention
        let config = CleanupConfig::default();
        let removed = CleanupService::cleanup_zombies(&mut kernel, config.process_retention_seconds)
            .unwrap();

        assert_eq!(removed, 1);
        assert!(kernel.get_process(&pcb.pid).is_none());
    }

    #[test]
    fn test_cleanup_preserves_recent_zombies() {
        let mut kernel = create_test_kernel();

        // Create and terminate process
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
        kernel.lifecycle.cleanup(&pcb.pid).unwrap();

        // Recent zombie (just created) - should NOT be removed
        let config = CleanupConfig::default();
        let removed = CleanupService::cleanup_zombies(&mut kernel, config.process_retention_seconds)
            .unwrap();

        assert_eq!(removed, 0);
        assert!(kernel.get_process(&pcb.pid).is_some());
    }

    #[test]
    fn test_run_cleanup_cycle() {
        let mut kernel = create_test_kernel();
        let config = CleanupConfig::default();

        // Create old zombie
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
        kernel.lifecycle.cleanup(&pcb.pid).unwrap();

        // Age it
        {
            let zombie = kernel.lifecycle.processes.get_mut(&pcb.pid).unwrap();
            zombie.completed_at = Some(Utc::now() - Duration::hours(25));
        }

        // Run full cleanup cycle
        let stats = CleanupService::run_cleanup_cycle_sync(&mut kernel, &config).unwrap();

        assert_eq!(stats.zombies_removed, 1);
        assert!(stats.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_cleanup_service_start_stop() {
        let kernel = Arc::new(Mutex::new(create_test_kernel()));
        let config = CleanupConfig {
            interval_seconds: 1, // Fast for testing
            ..Default::default()
        };

        let mut service = CleanupService::new(kernel, config);
        let handle = service.start();

        // Let it run briefly
        tokio::time::sleep(TokioDuration::from_millis(100)).await;

        // Stop it
        service.stop();

        // Wait for task to complete
        let _ = tokio::time::timeout(TokioDuration::from_secs(2), handle)
            .await
            .expect("cleanup service should stop");
    }
}
