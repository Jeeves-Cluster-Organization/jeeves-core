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
    /// Maximum LLM calls per run.
    pub max_llm_calls: i32,

    /// Maximum tool calls per run.
    pub max_tool_calls: i32,

    /// Maximum agent hops per run.
    pub max_agent_hops: i32,

    /// Maximum iterations per run.
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

/// Agent definition for config-driven agent registration via JEEVES_AGENTS env var.
///
/// Not to be confused with `kernel::orchestrator_types::AgentConfig` which is
/// the per-stage pipeline config (prompt_key, has_llm, temperature, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
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
        config
    }
}

