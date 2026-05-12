//! Observability utilities.

use std::sync::OnceLock;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static TRACING_INIT: OnceLock<()> = OnceLock::new();

/// Initialize tracing subscriber once for the process.
///
/// Reads `RUST_LOG` for filter level and `JEEVES_LOG_FORMAT` for output format.
/// If a `Config` is available, prefer `init_tracing_from_config` which uses
/// the config values directly (falling back to env vars).
pub fn init_tracing() {
    init_tracing_with("info", false);
}

/// Initialize tracing from parsed config values.
pub fn init_tracing_from_config(config: &crate::types::ObservabilityConfig) {
    init_tracing_with(&config.log_level, config.json_logs);
}

fn init_tracing_with(default_level: &str, json_from_config: bool) {
    TRACING_INIT.get_or_init(|| {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
        let json = std::env::var("JEEVES_LOG_FORMAT")
            .map(|v| v.eq_ignore_ascii_case("json"))
            .unwrap_or(json_from_config);

        let result = if json {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json())
                .try_init()
        } else {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().compact())
                .try_init()
        };

        if let Err(err) = result {
            eprintln!("tracing init skipped: {err}");
        }
    });
}

/// OpenTelemetry tracing layer for production observability.
///
/// Bridges all existing `#[instrument]` and `tracing::info!` calls to
/// OpenTelemetry spans, exportable to Jaeger, Datadog, Grafana Tempo, etc.
///
/// # Usage
/// ```text
/// use tracing_subscriber::prelude::*;
/// tracing_subscriber::registry()
///     .with(tracing_subscriber::fmt::layer())
///     .with(jeeves_core::observability::otel_tracing_layer())
///     .init();
/// ```
#[cfg(feature = "otel")]
pub fn otel_tracing_layer<S>() -> tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
    let tracer = opentelemetry::trace::TracerProvider::tracer(&provider, "jeeves-core");
    tracing_opentelemetry::layer().with_tracer(tracer)
}

#[cfg(test)]
mod tests {
    use super::init_tracing;

    #[test]
    fn init_tracing_is_idempotent() {
        init_tracing();
        init_tracing();
    }
}
