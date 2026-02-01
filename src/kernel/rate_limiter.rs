//! Rate limiting and throttling.
//!
//! Simple sliding window rate limiter for API calls.

use chrono::{DateTime, Duration, Utc};
use std::collections::{HashMap, VecDeque};

use crate::types::{Error, Result};

/// Rate limit window configuration.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            requests_per_hour: 1000,
            burst_size: 10,
        }
    }
}

/// Sliding window for tracking requests.
#[derive(Debug)]
struct SlidingWindow {
    timestamps: VecDeque<DateTime<Utc>>,
    config: RateLimitConfig,
}

impl SlidingWindow {
    fn new(config: RateLimitConfig) -> Self {
        Self {
            timestamps: VecDeque::new(),
            config,
        }
    }

    /// Check if request is allowed under rate limits.
    fn check_and_record(&mut self, now: DateTime<Utc>) -> Result<()> {
        // Remove timestamps outside the hour window
        let hour_ago = now - Duration::hours(1);
        while let Some(&ts) = self.timestamps.front() {
            if ts < hour_ago {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }

        // Check hour limit
        if self.timestamps.len() >= self.config.requests_per_hour as usize {
            return Err(Error::quota_exceeded(format!(
                "Rate limit exceeded: {} requests per hour",
                self.config.requests_per_hour
            )));
        }

        // Check minute limit
        let minute_ago = now - Duration::minutes(1);
        let recent_count = self
            .timestamps
            .iter()
            .filter(|&&ts| ts >= minute_ago)
            .count();

        if recent_count >= self.config.requests_per_minute as usize {
            return Err(Error::quota_exceeded(format!(
                "Rate limit exceeded: {} requests per minute",
                self.config.requests_per_minute
            )));
        }

        // Check burst limit (last 10 seconds)
        let ten_seconds_ago = now - Duration::seconds(10);
        let burst_count = self
            .timestamps
            .iter()
            .filter(|&&ts| ts >= ten_seconds_ago)
            .count();

        if burst_count >= self.config.burst_size as usize {
            return Err(Error::quota_exceeded(format!(
                "Burst limit exceeded: {} requests per 10 seconds",
                self.config.burst_size
            )));
        }

        // Record this request
        self.timestamps.push_back(now);
        Ok(())
    }
}

/// Rate limiter - enforces request rate limits per user.
///
/// NOT a separate actor - owned by Kernel and called via &mut self.
#[derive(Debug)]
pub struct RateLimiter {
    default_config: RateLimitConfig,
    user_windows: HashMap<String, SlidingWindow>,
}

impl RateLimiter {
    pub fn new(default_config: Option<RateLimitConfig>) -> Self {
        Self {
            default_config: default_config.unwrap_or_default(),
            user_windows: HashMap::new(),
        }
    }

    /// Check rate limit for a user and record the request if allowed.
    pub fn check_rate_limit(&mut self, user_id: &str) -> Result<()> {
        let now = Utc::now();
        let window = self
            .user_windows
            .entry(user_id.to_string())
            .or_insert_with(|| SlidingWindow::new(self.default_config.clone()));

        window.check_and_record(now)
    }

    /// Get current request count for a user (last minute).
    pub fn get_current_rate(&self, user_id: &str) -> usize {
        if let Some(window) = self.user_windows.get(user_id) {
            let now = Utc::now();
            let minute_ago = now - Duration::minutes(1);
            window
                .timestamps
                .iter()
                .filter(|&&ts| ts >= minute_ago)
                .count()
        } else {
            0
        }
    }

    /// Clear rate limit window for a user.
    pub fn clear_user_limits(&mut self, user_id: &str) {
        self.user_windows.remove(user_id);
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(None)
    }
}

