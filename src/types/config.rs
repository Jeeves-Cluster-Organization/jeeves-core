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
    /// gRPC server bind address.
    pub grpc_addr: String,

    /// Metrics endpoint bind address.
    pub metrics_addr: String,

    /// Maximum concurrent connections.
    pub max_connections: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            grpc_addr: "127.0.0.1:50051".to_string(),
            metrics_addr: "127.0.0.1:9090".to_string(),
            max_connections: 1000,
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
    pub max_llm_calls: u32,

    /// Maximum tool calls per envelope.
    pub max_tool_calls: u32,

    /// Maximum agent hops per envelope.
    pub max_agent_hops: u32,

    /// Maximum iterations per envelope.
    pub max_iterations: u32,

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
