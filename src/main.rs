//! Jeeves kernel — single-process runtime.

use clap::Parser;
use jeeves_core::kernel::Kernel;
use jeeves_core::worker::actor::spawn_kernel;
use jeeves_core::worker::agent::AgentRegistry;
use jeeves_core::worker::gateway::{build_router, AppState};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(name = "jeeves-kernel", about = "Jeeves multi-agent orchestration kernel")]
enum Cli {
    /// Run the kernel with HTTP gateway.
    Run {
        /// HTTP bind address.
        #[arg(long, default_value = "0.0.0.0:8080")]
        http_addr: String,

        /// Directory containing prompt templates.
        #[arg(long)]
        prompts_dir: Option<std::path::PathBuf>,

        /// LLM API key (or set LLM_API_KEY env var).
        #[arg(long, env = "LLM_API_KEY")]
        llm_api_key: Option<String>,

        /// Default LLM model name.
        #[arg(long, env = "LLM_MODEL", default_value = "gpt-4o-mini")]
        llm_model: String,

        /// LLM API base URL (OpenAI-compatible).
        #[arg(long, env = "LLM_BASE_URL")]
        llm_base_url: Option<String>,
    },
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli {
        Cli::Run {
            http_addr,
            llm_api_key: _,
            llm_model: _,
            llm_base_url: _,
            prompts_dir: _,
        } => {
            // Load config from env
            let config = jeeves_core::Config::from_env();
            jeeves_core::observability::init_tracing_from_config(&config.observability);

            // Create and spawn kernel actor
            let kernel = Kernel::from_config(&config);
            let cancel = CancellationToken::new();
            let handle = spawn_kernel(kernel, cancel.clone());

            // Build agent registry (empty by default — capabilities register via HTTP)
            let agents = Arc::new(AgentRegistry::new());

            // Build HTTP router
            let app_state = AppState {
                handle,
                agents,
            };
            let router = build_router(app_state);

            // Bind and serve
            let addr: std::net::SocketAddr = http_addr.parse()?;
            let listener = tokio::net::TcpListener::bind(addr).await?;
            tracing::info!(listen_addr = %addr, "Jeeves kernel HTTP server starting");

            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = tokio::signal::ctrl_c().await;
                    tracing::info!("Shutdown signal received");
                    cancel.cancel();
                })
                .await?;
        }
    }

    Ok(())
}
