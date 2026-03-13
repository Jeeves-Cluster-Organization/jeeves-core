//! Configuration structures.
//!
//! Configuration is loaded from environment variables and config files.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Global kernel configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Server configuration.
    #[serde(default)]
    pub server: ServerConfig,

    /// Observability configuration.
    #[serde(default)]
    pub observability: ObservabilityConfig,

    /// Default resource limits.
    #[serde(default)]
    pub defaults: DefaultLimits,

    /// Rate limiting configuration.
    #[serde(default)]
    pub rate_limit: RateLimitConfig,

    /// Background cleanup configuration.
    #[serde(default)]
    pub cleanup: CleanupConfig,
}

/// Rate limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per minute per user.
    pub requests_per_minute: u32,
    /// Maximum requests per hour per user.
    pub requests_per_hour: u32,
    /// Maximum burst size (requests per 10-second window).
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

/// Background cleanup configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupConfig {
    /// How often to run cleanup in seconds.
    pub interval_seconds: u64,
    /// How long to keep zombie processes in seconds.
    pub process_retention_seconds: i64,
    /// How long to keep stale sessions in seconds.
    pub session_retention_seconds: i64,
    /// How long to keep resolved interrupts in seconds.
    pub interrupt_retention_seconds: i64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 300,
            process_retention_seconds: 86400,
            session_retention_seconds: 3600,
            interrupt_retention_seconds: 86400,
        }
    }
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// HTTP server bind address.
    pub listen_addr: String,

    /// Metrics endpoint bind address.
    pub metrics_addr: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:8080".to_string(),
            metrics_addr: "127.0.0.1:9090".to_string(),
        }
    }
}

/// Observability configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    /// Tracing log level (trace, debug, info, warn, error).
    pub log_level: String,

    /// Enable JSON log formatting.
    pub json_logs: bool,

    /// OTLP exporter endpoint (optional).
    pub otlp_endpoint: Option<String>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            json_logs: false,
            otlp_endpoint: None,
        }
    }
}

/// Default resource limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultLimits {
    /// Maximum LLM calls per envelope.
    pub max_llm_calls: i32,

    /// Maximum tool calls per envelope.
    pub max_tool_calls: i32,

    /// Maximum agent hops per envelope.
    pub max_agent_hops: i32,

    /// Maximum iterations per envelope.
    pub max_iterations: i32,

    /// Default process timeout.
    #[serde(with = "humantime_serde")]
    pub process_timeout: Duration,
}

impl Default for DefaultLimits {
    fn default() -> Self {
        Self {
            max_llm_calls: 100,
            max_tool_calls: 50,
            max_agent_hops: 10,
            max_iterations: 20,
            process_timeout: Duration::from_secs(300),
        }
    }
}

/// Agent configuration for config-driven agent registration via JEEVES_AGENTS env var.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent name (used as key in AgentRegistry).
    pub name: String,
    /// Agent type: "llm", "mcp_delegate", "deterministic", "gate".
    #[serde(rename = "type")]
    pub agent_type: String,
    /// Prompt template key (for LLM agents).
    #[serde(default)]
    pub prompt_key: Option<String>,
    /// LLM temperature override.
    #[serde(default)]
    pub temperature: Option<f64>,
    /// LLM max_tokens override.
    #[serde(default)]
    pub max_tokens: Option<i32>,
    /// LLM model override.
    #[serde(default)]
    pub model: Option<String>,
    /// MCP tool name (for mcp_delegate agents).
    #[serde(default)]
    pub tool_name: Option<String>,
}

/// MCP server configuration for auto-connect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Falls back to defaults for any unset variable.
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(addr) = std::env::var("JEEVES_HTTP_ADDR") {
            config.server.listen_addr = addr;
        }
        if let Ok(addr) = std::env::var("JEEVES_METRICS_ADDR") {
            config.server.metrics_addr = addr;
        }
        if let Ok(level) = std::env::var("RUST_LOG") {
            config.observability.log_level = level;
        }
        if let Ok(fmt) = std::env::var("JEEVES_LOG_FORMAT") {
            config.observability.json_logs = fmt.eq_ignore_ascii_case("json");
        }
        if let Ok(ep) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            config.observability.otlp_endpoint = Some(ep);
        }
        if let Ok(v) = std::env::var("CORE_MAX_LLM_CALLS") {
            if let Ok(n) = v.parse() { config.defaults.max_llm_calls = n; }
        }
        if let Ok(v) = std::env::var("CORE_MAX_ITERATIONS") {
            if let Ok(n) = v.parse() { config.defaults.max_iterations = n; }
        }
        if let Ok(v) = std::env::var("CORE_MAX_AGENT_HOPS") {
            if let Ok(n) = v.parse() { config.defaults.max_agent_hops = n; }
        }
        if let Ok(v) = std::env::var("CORE_RATE_LIMIT_RPM") {
            if let Ok(n) = v.parse() { config.rate_limit.requests_per_minute = n; }
        }
        if let Ok(v) = std::env::var("CORE_RATE_LIMIT_RPH") {
            if let Ok(n) = v.parse() { config.rate_limit.requests_per_hour = n; }
        }
        if let Ok(v) = std::env::var("CORE_RATE_LIMIT_BURST") {
            if let Ok(n) = v.parse() { config.rate_limit.burst_size = n; }
        }
        if let Ok(v) = std::env::var("CORE_CLEANUP_INTERVAL") {
            if let Ok(n) = v.parse() { config.cleanup.interval_seconds = n; }
        }
        if let Ok(v) = std::env::var("CORE_SESSION_RETENTION") {
            if let Ok(n) = v.parse() { config.cleanup.session_retention_seconds = n; }
        }

        config
    }
}

