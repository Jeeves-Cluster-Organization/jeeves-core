//! Observability utilities.

use std::sync::OnceLock;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static TRACING_INIT: OnceLock<()> = OnceLock::new();

/// Initialize tracing subscriber once for the process.
///
/// Log format defaults to plain text and can be switched to JSON via
/// `JEEVES_LOG_FORMAT=json`. Filter defaults to `info` if `RUST_LOG` is unset.
pub fn init_tracing() {
    TRACING_INIT.get_or_init(|| {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let json = std::env::var("JEEVES_LOG_FORMAT")
            .map(|v| v.eq_ignore_ascii_case("json"))
            .unwrap_or(false);

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

#[cfg(test)]
mod tests {
    use super::init_tracing;

    #[test]
    fn init_tracing_is_idempotent() {
        init_tracing();
        init_tracing();
    }
}
