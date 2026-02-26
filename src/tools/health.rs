//! Tool health tracking and circuit breaking.
//!
//! In-memory sliding-window health metrics per tool. Replaces Python's
//! 534-line ToolHealthService with compile-time-safe, configurable health tracking.
//! Python keeps SQLite persistence (ToolMetricsRepository); Rust owns computation.

use crate::envelope::enums::HealthStatus;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

// =============================================================================
// Configuration
// =============================================================================

/// Health assessment thresholds (configurable, not hardcoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Minimum success rate for HEALTHY status (default: 0.95).
    pub success_rate_healthy: f64,
    /// Minimum success rate for DEGRADED status (default: 0.80).
    pub success_rate_degraded: f64,
    /// Maximum avg latency (ms) for HEALTHY status (default: 2000).
    pub latency_healthy_ms: u64,
    /// Maximum avg latency (ms) for DEGRADED status (default: 5000).
    pub latency_degraded_ms: u64,
    /// Minimum calls before health assessment (default: 5).
    pub min_calls_for_assessment: usize,
    /// Circuit breaker: error threshold within window (default: 5).
    pub circuit_break_error_threshold: usize,
    /// Circuit breaker: time window (default: 5 minutes).
    pub circuit_break_window: Duration,
    /// Sliding window size for health metrics (default: 100).
    pub window_size: usize,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            success_rate_healthy: 0.95,
            success_rate_degraded: 0.80,
            latency_healthy_ms: 2000,
            latency_degraded_ms: 5000,
            min_calls_for_assessment: 5,
            circuit_break_error_threshold: 5,
            circuit_break_window: Duration::from_secs(300),
            window_size: 100,
        }
    }
}

// =============================================================================
// Execution record
// =============================================================================

/// Single tool execution record (in-memory, sliding window).
#[derive(Debug, Clone)]
struct ExecutionRecord {
    success: bool,
    latency_ms: u64,
    timestamp: Instant,
    error_type: Option<String>,
}

// =============================================================================
// Per-tool metrics
// =============================================================================

/// Sliding window metrics for a single tool.
#[derive(Debug)]
struct ToolMetrics {
    records: VecDeque<ExecutionRecord>,
    window_size: usize,
}

impl ToolMetrics {
    fn new(window_size: usize) -> Self {
        Self {
            records: VecDeque::with_capacity(window_size),
            window_size,
        }
    }

    fn record(&mut self, success: bool, latency_ms: u64, error_type: Option<String>) {
        if self.records.len() >= self.window_size {
            self.records.pop_front();
        }
        self.records.push_back(ExecutionRecord {
            success,
            latency_ms,
            timestamp: Instant::now(),
            error_type,
        });
    }

    fn total_calls(&self) -> usize {
        self.records.len()
    }

    fn success_count(&self) -> usize {
        self.records.iter().filter(|r| r.success).count()
    }

    fn error_count(&self) -> usize {
        self.records.iter().filter(|r| !r.success).count()
    }

    fn success_rate(&self) -> f64 {
        let total = self.total_calls();
        if total == 0 {
            return 0.0;
        }
        self.success_count() as f64 / total as f64
    }

    fn avg_latency_ms(&self) -> f64 {
        let total = self.total_calls();
        if total == 0 {
            return 0.0;
        }
        let sum: u64 = self.records.iter().map(|r| r.latency_ms).sum();
        sum as f64 / total as f64
    }

    fn recent_errors_in_window(&self, window: Duration) -> usize {
        let cutoff = Instant::now() - window;
        self.records
            .iter()
            .filter(|r| !r.success && r.timestamp >= cutoff)
            .count()
    }

    fn error_patterns(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for record in &self.records {
            if !record.success {
                let error_type = record
                    .error_type
                    .as_deref()
                    .unwrap_or("unknown")
                    .to_string();
                *counts.entry(error_type).or_default() += 1;
            }
        }
        let mut patterns: Vec<(String, usize)> = counts.into_iter().collect();
        patterns.sort_by(|a, b| b.1.cmp(&a.1));
        patterns
    }
}

// =============================================================================
// Health report
// =============================================================================

/// Health report for a single tool.
#[derive(Debug, Clone, Serialize)]
pub struct ToolHealthReport {
    pub tool_name: String,
    pub status: HealthStatus,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub total_calls: usize,
    pub recent_errors: usize,
    pub issues: Vec<String>,
    pub circuit_broken: bool,
}

/// System-wide health report.
#[derive(Debug, Clone, Serialize)]
pub struct SystemHealthReport {
    pub status: HealthStatus,
    pub tool_reports: Vec<ToolHealthReport>,
    pub summary: HealthSummary,
}

/// Counts by health status.
#[derive(Debug, Clone, Serialize)]
pub struct HealthSummary {
    pub healthy: usize,
    pub degraded: usize,
    pub unhealthy: usize,
    pub unknown: usize,
}

// =============================================================================
// Health tracker
// =============================================================================

/// In-memory tool health tracker with sliding-window metrics.
#[derive(Debug)]
pub struct ToolHealthTracker {
    config: HealthConfig,
    metrics: HashMap<String, ToolMetrics>,
    /// Tools that were registered but may not have executed yet.
    registered_tools: Vec<String>,
}

impl ToolHealthTracker {
    pub fn new(config: HealthConfig) -> Self {
        Self {
            config,
            metrics: HashMap::new(),
            registered_tools: Vec::new(),
        }
    }

    /// Register tool names for dashboard display.
    pub fn set_registered_tools(&mut self, tool_names: Vec<String>) {
        self.registered_tools = tool_names;
    }

    /// Record a tool execution.
    pub fn record_execution(
        &mut self,
        tool_name: &str,
        success: bool,
        latency_ms: u64,
        error_type: Option<String>,
    ) {
        let metrics = self
            .metrics
            .entry(tool_name.to_string())
            .or_insert_with(|| ToolMetrics::new(self.config.window_size));
        metrics.record(success, latency_ms, error_type);
    }

    /// Check health of a single tool.
    pub fn check_tool_health(&self, tool_name: &str) -> ToolHealthReport {
        let metrics = self.metrics.get(tool_name);

        match metrics {
            None => ToolHealthReport {
                tool_name: tool_name.to_string(),
                status: HealthStatus::Unknown,
                success_rate: 0.0,
                avg_latency_ms: 0.0,
                total_calls: 0,
                recent_errors: 0,
                issues: vec!["No execution history".to_string()],
                circuit_broken: false,
            },
            Some(m) => {
                let total = m.total_calls();
                if total < self.config.min_calls_for_assessment {
                    return ToolHealthReport {
                        tool_name: tool_name.to_string(),
                        status: HealthStatus::Unknown,
                        success_rate: m.success_rate(),
                        avg_latency_ms: m.avg_latency_ms(),
                        total_calls: total,
                        recent_errors: m.error_count(),
                        issues: vec![format!(
                            "Insufficient data ({}/{})",
                            total, self.config.min_calls_for_assessment
                        )],
                        circuit_broken: false,
                    };
                }

                let success_rate = m.success_rate();
                let avg_latency = m.avg_latency_ms();
                let recent_errors =
                    m.recent_errors_in_window(self.config.circuit_break_window);
                let circuit_broken =
                    recent_errors >= self.config.circuit_break_error_threshold;

                // Worst-of-two: success rate status vs latency status
                let rate_status = if success_rate >= self.config.success_rate_healthy {
                    HealthStatus::Healthy
                } else if success_rate >= self.config.success_rate_degraded {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Unhealthy
                };

                let latency_status =
                    if avg_latency <= self.config.latency_healthy_ms as f64 {
                        HealthStatus::Healthy
                    } else if avg_latency <= self.config.latency_degraded_ms as f64 {
                        HealthStatus::Degraded
                    } else {
                        HealthStatus::Unhealthy
                    };

                let status = worse_status(rate_status, latency_status);

                let mut issues = Vec::new();
                if success_rate < self.config.success_rate_healthy {
                    issues.push(format!(
                        "Success rate {:.1}% below {:.0}% threshold",
                        success_rate * 100.0,
                        self.config.success_rate_healthy * 100.0,
                    ));
                }
                if avg_latency > self.config.latency_healthy_ms as f64 {
                    issues.push(format!(
                        "Avg latency {:.0}ms exceeds {}ms threshold",
                        avg_latency, self.config.latency_healthy_ms,
                    ));
                }
                if circuit_broken {
                    issues.push(format!(
                        "Circuit breaker open: {} errors in last {}s",
                        recent_errors,
                        self.config.circuit_break_window.as_secs(),
                    ));
                }

                ToolHealthReport {
                    tool_name: tool_name.to_string(),
                    status,
                    success_rate,
                    avg_latency_ms: avg_latency,
                    total_calls: total,
                    recent_errors,
                    issues,
                    circuit_broken,
                }
            }
        }
    }

    /// Check if a tool's circuit breaker is open (should not execute).
    pub fn should_circuit_break(&self, tool_name: &str) -> bool {
        self.metrics
            .get(tool_name)
            .map(|m| {
                m.recent_errors_in_window(self.config.circuit_break_window)
                    >= self.config.circuit_break_error_threshold
            })
            .unwrap_or(false)
    }

    /// Check health of all tools (registered + executed).
    pub fn check_system_health(&self) -> SystemHealthReport {
        // Merge registered tools and tools with metrics
        let mut all_tools: Vec<String> = self.registered_tools.clone();
        for name in self.metrics.keys() {
            if !all_tools.contains(name) {
                all_tools.push(name.clone());
            }
        }
        all_tools.sort();

        let tool_reports: Vec<ToolHealthReport> = all_tools
            .iter()
            .map(|name| self.check_tool_health(name))
            .collect();

        let mut summary = HealthSummary {
            healthy: 0,
            degraded: 0,
            unhealthy: 0,
            unknown: 0,
        };

        for report in &tool_reports {
            match report.status {
                HealthStatus::Healthy => summary.healthy += 1,
                HealthStatus::Degraded => summary.degraded += 1,
                HealthStatus::Unhealthy => summary.unhealthy += 1,
                HealthStatus::Unknown => summary.unknown += 1,
            }
        }

        // System status = worst of all tool statuses
        let status = if summary.unhealthy > 0 {
            HealthStatus::Unhealthy
        } else if summary.degraded > 0 {
            HealthStatus::Degraded
        } else if summary.healthy > 0 {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unknown
        };

        SystemHealthReport {
            status,
            tool_reports,
            summary,
        }
    }

    /// Get error patterns for a tool.
    pub fn get_error_patterns(&self, tool_name: &str) -> Vec<(String, usize)> {
        self.metrics
            .get(tool_name)
            .map(|m| m.error_patterns())
            .unwrap_or_default()
    }

    /// Number of tracked tools.
    pub fn tool_count(&self) -> usize {
        self.metrics.len()
    }
}

impl Default for ToolHealthTracker {
    fn default() -> Self {
        Self::new(HealthConfig::default())
    }
}

fn worse_status(a: HealthStatus, b: HealthStatus) -> HealthStatus {
    let rank = |s: HealthStatus| -> u8 {
        match s {
            HealthStatus::Healthy => 0,
            HealthStatus::Degraded => 1,
            HealthStatus::Unhealthy => 2,
            HealthStatus::Unknown => 3,
        }
    };
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn default_tracker() -> ToolHealthTracker {
        ToolHealthTracker::new(HealthConfig {
            min_calls_for_assessment: 3,
            circuit_break_error_threshold: 3,
            circuit_break_window: Duration::from_secs(300),
            window_size: 100,
            ..Default::default()
        })
    }

    #[test]
    fn test_no_data_unknown() {
        let tracker = default_tracker();
        let report = tracker.check_tool_health("search_web");
        assert_eq!(report.status, HealthStatus::Unknown);
        assert_eq!(report.total_calls, 0);
    }

    #[test]
    fn test_insufficient_data_unknown() {
        let mut tracker = default_tracker();
        tracker.record_execution("search_web", true, 100, None);
        tracker.record_execution("search_web", true, 200, None);
        // Only 2 calls, need 3
        let report = tracker.check_tool_health("search_web");
        assert_eq!(report.status, HealthStatus::Unknown);
    }

    #[test]
    fn test_healthy_tool() {
        let mut tracker = default_tracker();
        for _ in 0..10 {
            tracker.record_execution("search_web", true, 100, None);
        }
        let report = tracker.check_tool_health("search_web");
        assert_eq!(report.status, HealthStatus::Healthy);
        assert!((report.success_rate - 1.0).abs() < f64::EPSILON);
        assert!((report.avg_latency_ms - 100.0).abs() < f64::EPSILON);
        assert!(report.issues.is_empty());
        assert!(!report.circuit_broken);
    }

    #[test]
    fn test_degraded_success_rate() {
        let mut tracker = default_tracker();
        for _ in 0..8 {
            tracker.record_execution("search_web", true, 100, None);
        }
        for _ in 0..2 {
            tracker.record_execution("search_web", false, 100, Some("TimeoutError".into()));
        }
        // 80% success rate → degraded (below 95% healthy, above 80% degraded)
        let report = tracker.check_tool_health("search_web");
        assert_eq!(report.status, HealthStatus::Degraded);
    }

    #[test]
    fn test_unhealthy_success_rate() {
        let mut tracker = default_tracker();
        for _ in 0..5 {
            tracker.record_execution("search_web", true, 100, None);
        }
        for _ in 0..5 {
            tracker.record_execution("search_web", false, 100, Some("Error".into()));
        }
        // 50% success rate → unhealthy (below 80%)
        let report = tracker.check_tool_health("search_web");
        assert_eq!(report.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_degraded_latency() {
        let mut tracker = default_tracker();
        for _ in 0..5 {
            tracker.record_execution("search_web", true, 3000, None);
        }
        // 100% success but 3000ms avg → degraded (>2000ms, <5000ms)
        let report = tracker.check_tool_health("search_web");
        assert_eq!(report.status, HealthStatus::Degraded);
    }

    #[test]
    fn test_unhealthy_latency() {
        let mut tracker = default_tracker();
        for _ in 0..5 {
            tracker.record_execution("search_web", true, 6000, None);
        }
        // 100% success but 6000ms avg → unhealthy (>5000ms)
        let report = tracker.check_tool_health("search_web");
        assert_eq!(report.status, HealthStatus::Unhealthy);
    }

    #[test]
    fn test_circuit_breaker() {
        let mut tracker = default_tracker();
        for _ in 0..3 {
            tracker.record_execution("search_web", false, 100, Some("Error".into()));
        }
        assert!(tracker.should_circuit_break("search_web"));
        assert!(!tracker.should_circuit_break("unknown_tool"));
    }

    #[test]
    fn test_sliding_window_eviction() {
        let mut tracker = ToolHealthTracker::new(HealthConfig {
            window_size: 5,
            min_calls_for_assessment: 3,
            ..Default::default()
        });

        // Fill window with failures
        for _ in 0..5 {
            tracker.record_execution("search_web", false, 100, None);
        }
        // Now add successes — old failures evicted
        for _ in 0..5 {
            tracker.record_execution("search_web", true, 100, None);
        }
        let report = tracker.check_tool_health("search_web");
        assert!((report.success_rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_system_health_report() {
        let mut tracker = default_tracker();
        tracker.set_registered_tools(vec!["tool_a".into(), "tool_b".into()]);

        for _ in 0..5 {
            tracker.record_execution("tool_a", true, 100, None);
        }
        // tool_b has no executions

        let report = tracker.check_system_health();
        assert_eq!(report.tool_reports.len(), 2);
        assert_eq!(report.summary.healthy, 1);
        assert_eq!(report.summary.unknown, 1);
    }

    #[test]
    fn test_error_patterns() {
        let mut tracker = default_tracker();
        tracker.record_execution("search_web", false, 100, Some("TimeoutError".into()));
        tracker.record_execution("search_web", false, 100, Some("TimeoutError".into()));
        tracker.record_execution("search_web", false, 100, Some("ValueError".into()));

        let patterns = tracker.get_error_patterns("search_web");
        assert_eq!(patterns[0].0, "TimeoutError");
        assert_eq!(patterns[0].1, 2);
        assert_eq!(patterns[1].0, "ValueError");
        assert_eq!(patterns[1].1, 1);
    }

    #[test]
    fn test_worst_of_two_status() {
        // Success rate healthy + latency degraded → degraded
        let mut tracker = default_tracker();
        for _ in 0..10 {
            tracker.record_execution("search_web", true, 3000, None);
        }
        let report = tracker.check_tool_health("search_web");
        assert_eq!(report.status, HealthStatus::Degraded);
    }
}
