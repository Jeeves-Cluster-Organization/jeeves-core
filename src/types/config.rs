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

    /// IPC transport configuration.
    #[serde(default)]
    pub ipc: IpcConfig,
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// IPC server bind address (TCP).
    pub listen_addr: String,

    /// Metrics endpoint bind address.
    pub metrics_addr: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:50051".to_string(),
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

/// IPC transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcConfig {
    /// Maximum frame payload size in bytes.
    pub max_frame_bytes: u32,

    /// Maximum CommBus query timeout in milliseconds (caps client-requested timeouts).
    pub max_query_timeout_ms: u64,

    /// Default CommBus query timeout in milliseconds (when client omits timeout_ms).
    pub default_query_timeout_ms: u64,

    /// Bounded channel capacity for streaming responses (Subscribe).
    pub stream_channel_capacity: usize,

    /// Maximum concurrent TCP connections. New connections beyond this limit
    /// are held until a slot opens (backpressure via semaphore).
    pub max_connections: usize,

    /// Read timeout in seconds per frame. Connections idle beyond this
    /// duration are dropped (prevents slowloris-style resource exhaustion).
    pub read_timeout_secs: u64,

    /// Write timeout in seconds per frame. Slow consumers that cannot
    /// accept a response within this window are dropped.
    pub write_timeout_secs: u64,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            max_frame_bytes: 5 * 1024 * 1024,
            max_query_timeout_ms: 30_000,
            default_query_timeout_ms: 5_000,
            stream_channel_capacity: 64,
            max_connections: 1000,
            read_timeout_secs: 30,
            write_timeout_secs: 10,
        }
    }
}
