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

    /// Get the current default config.
    pub fn get_default_config(&self) -> &RateLimitConfig {
        &self.default_config
    }

    /// Set the default rate limit config. Clears existing windows.
    pub fn set_default_config(&mut self, config: RateLimitConfig) {
        self.default_config = config;
        self.user_windows.clear();
    }

    /// Clean up expired rate limit windows.
    ///
    /// Removes windows that have no recent requests (older than window duration).
    pub fn cleanup_expired(&mut self) {
        let now = Utc::now();
        let window_cutoff = now - Duration::minutes(2); // Keep windows with activity in last 2 min

        self.user_windows.retain(|_, window| {
            // Keep window if it has any recent timestamps
            window.timestamps.iter().any(|&ts| ts >= window_cutoff)
        });
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_allows_within_limit() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 1000,
            burst_size: 10,
        };
        let mut limiter = RateLimiter::new(Some(config));

        // First request should succeed
        assert!(limiter.check_rate_limit("user1").is_ok());

        // Second request should also succeed (well within limits)
        assert!(limiter.check_rate_limit("user1").is_ok());
    }

    #[test]
    fn test_rate_limit_blocks_per_minute() {
        let config = RateLimitConfig {
            requests_per_minute: 3,
            requests_per_hour: 1000,
            burst_size: 10,
        };
        let mut limiter = RateLimiter::new(Some(config));

        // First 3 requests should succeed
        for i in 0..3 {
            assert!(limiter.check_rate_limit("user1").is_ok(), "Request {} should succeed", i);
        }

        // 4th request should fail (exceeds per-minute limit)
        let result = limiter.check_rate_limit("user1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requests per minute"));
    }

    #[test]
    fn test_rate_limit_blocks_per_hour() {
        let config = RateLimitConfig {
            requests_per_minute: 1000, // Set high to not hit minute limit
            requests_per_hour: 5,
            burst_size: 1000, // Set high to not hit burst limit
        };
        let mut limiter = RateLimiter::new(Some(config));

        // First 5 requests should succeed
        for i in 0..5 {
            assert!(limiter.check_rate_limit("user1").is_ok(), "Request {} should succeed", i);
        }

        // 6th request should fail (exceeds per-hour limit)
        let result = limiter.check_rate_limit("user1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requests per hour"));
    }

    #[test]
    fn test_burst_allows_initial_requests() {
        let config = RateLimitConfig {
            requests_per_minute: 100,
            requests_per_hour: 1000,
            burst_size: 5,
        };
        let mut limiter = RateLimiter::new(Some(config));

        // First 5 requests (burst) should succeed
        for i in 0..5 {
            assert!(limiter.check_rate_limit("user1").is_ok(), "Request {} failed", i);
        }
    }

    #[test]
    fn test_burst_blocks_after_exhausted() {
        let config = RateLimitConfig {
            requests_per_minute: 100,
            requests_per_hour: 1000,
            burst_size: 3,
        };
        let mut limiter = RateLimiter::new(Some(config));

        // First 3 requests (burst size) should succeed
        for i in 0..3 {
            assert!(limiter.check_rate_limit("user1").is_ok(), "Request {} should succeed", i);
        }

        // 4th should fail (burst exhausted)
        let result = limiter.check_rate_limit("user1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Burst limit exceeded"));
    }

    #[test]
    fn test_per_user_isolation() {
        let config = RateLimitConfig {
            requests_per_minute: 2,
            requests_per_hour: 1000,
            burst_size: 10,
        };
        let mut limiter = RateLimiter::new(Some(config));

        // User1 makes 2 requests (at limit)
        limiter.check_rate_limit("user1").unwrap();
        limiter.check_rate_limit("user1").unwrap();

        // User1's 3rd request should fail
        assert!(limiter.check_rate_limit("user1").is_err());

        // User2's first request should still succeed (separate window)
        assert!(limiter.check_rate_limit("user2").is_ok());
    }

    #[test]
    fn test_get_current_rate() {
        let config = RateLimitConfig {
            requests_per_minute: 100,
            requests_per_hour: 1000,
            burst_size: 10,
        };
        let mut limiter = RateLimiter::new(Some(config));

        // Initially zero
        assert_eq!(limiter.get_current_rate("user1"), 0);

        // After 3 requests
        for _ in 0..3 {
            limiter.check_rate_limit("user1").unwrap();
        }
        assert_eq!(limiter.get_current_rate("user1"), 3);
    }

    #[test]
    fn test_clear_user_limits() {
        let config = RateLimitConfig {
            requests_per_minute: 2,
            requests_per_hour: 1000,
            burst_size: 10,
        };
        let mut limiter = RateLimiter::new(Some(config));

        // User1 hits limit
        limiter.check_rate_limit("user1").unwrap();
        limiter.check_rate_limit("user1").unwrap();
        assert!(limiter.check_rate_limit("user1").is_err());

        // Clear limits
        limiter.clear_user_limits("user1");

        // Should succeed again
        assert!(limiter.check_rate_limit("user1").is_ok());
    }

    #[test]
    fn test_multiple_users_independent() {
        let config = RateLimitConfig {
            requests_per_minute: 2,
            requests_per_hour: 1000,
            burst_size: 10,
        };
        let mut limiter = RateLimiter::new(Some(config));

        // User1 uses 2 requests
        limiter.check_rate_limit("user1").unwrap();
        limiter.check_rate_limit("user1").unwrap();

        // User2 uses 1 request
        limiter.check_rate_limit("user2").unwrap();

        // User1 blocked
        assert!(limiter.check_rate_limit("user1").is_err());

        // User2 still has capacity
        assert!(limiter.check_rate_limit("user2").is_ok());

        // User3 fresh
        assert!(limiter.check_rate_limit("user3").is_ok());
    }
}

