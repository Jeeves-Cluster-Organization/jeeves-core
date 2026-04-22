use std::sync::OnceLock;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static INIT: OnceLock<()> = OnceLock::new();

/// Install a tracing subscriber once per process. Idempotent.
/// `RUST_LOG` overrides `default_level`; `AGENT_LOG_FORMAT=json` selects JSON output.
pub fn init_tracing(default_level: &str) {
    INIT.get_or_init(|| {
        let filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
        let json = std::env::var("AGENT_LOG_FORMAT")
            .map(|v| v.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        let result = if json {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().json())
                .try_init()
        } else {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().compact())
                .try_init()
        };
        if let Err(err) = result {
            eprintln!("tracing init skipped: {err}");
        }
    });
}
