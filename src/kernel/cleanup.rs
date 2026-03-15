//! Background cleanup for resource management.
//!
//! CleanupService provides periodic garbage collection of:
//! - Zombie processes (terminated but not removed)
//! - Stale orchestration sessions (expired sessions)
//! - Resolved interrupts (old interrupt records)
//! - Expired rate limit windows (old tracking data)
//!
//! This prevents memory leaks in long-running production deployments.

use crate::types::Result;
use chrono::{Duration, Utc};
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
    pub completed_at: Option<chrono::DateTime<Utc>>,
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
            let interval_secs = config.interval_seconds.max(10); // minimum 10 seconds
            let mut ticker = interval(TokioDuration::from_secs(interval_secs));

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
    /// Releases the kernel between phases so command processing isn't
    /// blocked for the entire cleanup duration.
    async fn run_cleanup_cycle_async(
        kernel: &Arc<Mutex<crate::kernel::Kernel>>,
        config: &CleanupConfig,
    ) -> Result<CleanupStats> {
        let mut stats = CleanupStats::default();

        // Phase 1: Zombie processes
        {
            let mut k = kernel.lock().await;
            stats.zombies_removed = k.lifecycle.cleanup_zombies(config.process_retention_seconds);
        }

        // Phase 2: Stale sessions (also removes associated process envelopes)
        {
            let mut k = kernel.lock().await;
            stats.sessions_removed = k.cleanup_stale_sessions(config.session_retention_seconds);
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
            stats.user_usage_evicted = k.cleanup_stale_user_usage(config.max_user_usage_entries);
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
        let zombies_removed = kernel.lifecycle.cleanup_zombies(config.process_retention_seconds);

        // Clean up stale sessions (also removes associated process envelopes)
        let sessions_removed = kernel.cleanup_stale_sessions(config.session_retention_seconds);

        // Clean up resolved interrupts
        let interrupt_duration = Duration::seconds(config.interrupt_retention_seconds);
        let interrupts_removed = kernel.interrupts.cleanup_resolved(interrupt_duration);

        // Clean up rate limit windows
        kernel.rate_limiter.cleanup_expired();

        // Evict stale user_usage entries (users with no active processes)
        let user_usage_evicted = kernel.cleanup_stale_user_usage(config.max_user_usage_entries);

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
        kernel.cleanup_process(&pcb.pid).unwrap();

        // Re-create and terminate to get a zombie we can age
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

        // We can't easily age a zombie through the facade since completed_at is set
        // during termination. Instead, test via the sync cycle which exercises the
        // facade method.
        let config = CleanupConfig::default();
        let stats = CleanupService::run_cleanup_cycle_sync(&mut kernel, &config).unwrap();
        // No old zombies → zombies_removed should be 0
        assert_eq!(stats.zombies_removed, 0);
        assert!(stats.completed_at.is_some());
    }

    #[test]
    fn test_run_cleanup_cycle() {
        let mut kernel = create_test_kernel();
        let config = CleanupConfig::default();

        // Run full cleanup cycle on empty kernel
        let stats = CleanupService::run_cleanup_cycle_sync(&mut kernel, &config).unwrap();

        assert_eq!(stats.zombies_removed, 0);
        assert_eq!(stats.sessions_removed, 0);
        assert_eq!(stats.interrupts_removed, 0);
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
