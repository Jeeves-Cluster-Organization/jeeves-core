//! Workflow-level execution policies. `ContextOverflow` lives in
//! `crate::agent::policy` (it's consumed inside the agent loop); this module
//! owns retry-with-backoff which the runner consumes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Retry-with-backoff for transient agent failures (Temporal activity retry
/// pattern). Applied before routing to `error_next`; no retry on interrupt
/// requests.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RetryPolicy {
    /// Maximum retry attempts (0 = no retry, default).
    #[serde(default)]
    pub max_retries: u32,
    /// Initial backoff in milliseconds (default: 1000).
    #[serde(default = "default_initial_backoff_ms")]
    pub initial_backoff_ms: u64,
    /// Maximum backoff in milliseconds (default: 30000).
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,
    /// Backoff multiplier (default: 2.0).
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 0,
            initial_backoff_ms: 1000,
            max_backoff_ms: 30000,
            backoff_multiplier: 2.0,
        }
    }
}

fn default_initial_backoff_ms() -> u64 {
    1000
}
fn default_max_backoff_ms() -> u64 {
    30000
}
fn default_backoff_multiplier() -> f64 {
    2.0
}
