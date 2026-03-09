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

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Falls back to defaults for any unset variable.
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(addr) = std::env::var("AIRFRAME_KERNEL_ADDRESS") {
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

    /// Bounded queue capacity for requests entering the kernel actor.
    /// Requests beyond this limit are rejected immediately.
    pub kernel_queue_capacity: usize,

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
            kernel_queue_capacity: 2048,
            read_timeout_secs: 30,
            write_timeout_secs: 10,
        }
    }
}
