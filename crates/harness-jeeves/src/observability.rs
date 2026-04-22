//! Observability helpers for the jeeves harness.

pub use agent_core::observability::init_tracing;

#[cfg(feature = "otel")]
pub fn otel_tracing_layer<S>(
) -> tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
    let tracer = opentelemetry::trace::TracerProvider::tracer(&provider, "agent-core");
    tracing_opentelemetry::layer().with_tracer(tracer)
}
