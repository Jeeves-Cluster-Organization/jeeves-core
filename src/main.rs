//! Jeeves kernel — single-process runtime.

use clap::Parser;
use jeeves_core::kernel::Kernel;
use jeeves_core::worker::actor::spawn_kernel;
use jeeves_core::worker::agent::AgentRegistry;
use jeeves_core::worker::gateway::{build_router, AppState};
use jeeves_core::worker::tools::{ToolExecutor, ToolRegistry};
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
    },
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli {
        Cli::Run { http_addr } => {
            // Load config from env
            let config = jeeves_core::Config::from_env();
            jeeves_core::observability::init_tracing_from_config(&config.observability);

            // Create and spawn kernel actor
            // `mut` enables pre-spawn registration (services, subscriptions) on &mut Kernel
            #[allow(unused_mut)]
            let mut kernel = Kernel::from_config(&config);
            let cancel = CancellationToken::new();

            // Build tool registry mutably, register MCP tools, then wrap in Arc
            let mut tool_registry = ToolRegistry::new();

            // MCP auto-connect from JEEVES_MCP_SERVERS env var
            if let Ok(mcp_json) = std::env::var("JEEVES_MCP_SERVERS") {
                if let Ok(servers) = serde_json::from_str::<Vec<jeeves_core::types::config::McpServerConfig>>(&mcp_json) {
                    for mcp_cfg in servers {
                        let transport = match mcp_cfg.transport.as_str() {
                            "http" => jeeves_core::worker::mcp::McpTransport::Http {
                                url: mcp_cfg.url.clone().unwrap_or_default(),
                            },
                            "stdio" => jeeves_core::worker::mcp::McpTransport::Stdio {
                                command: mcp_cfg.command.clone().unwrap_or_default(),
                                args: mcp_cfg.args.clone().unwrap_or_default(),
                            },
                            other => {
                                tracing::warn!(transport = other, name = %mcp_cfg.name, "Unknown MCP transport, skipping");
                                continue;
                            }
                        };
                        match jeeves_core::worker::mcp::McpToolExecutor::connect(transport).await {
                            Ok(executor) => {
                                let executor = Arc::new(executor);
                                let tool_count = executor.list_tools().len();
                                tool_registry.register(&mcp_cfg.name, executor);
                                tracing::info!(name = %mcp_cfg.name, tools = tool_count, "MCP server connected");
                            }
                            Err(e) => tracing::error!(name = %mcp_cfg.name, error = %e, "MCP server connect failed"),
                        }
                        // Register service in kernel for health tracking
                        kernel.register_service(
                            jeeves_core::kernel::ServiceInfo::new(
                                mcp_cfg.name.clone(),
                                jeeves_core::kernel::SERVICE_TYPE_MCP.into(),
                            ),
                        );
                    }
                }
            }

            let tools = Arc::new(tool_registry);

            // Pre-spawn wiring point: register MCP services, CommBus subscriptions, etc.

            let handle = spawn_kernel(kernel, cancel.clone());

            // Build agent registry (empty by default — capabilities register via HTTP)
            let agents = Arc::new(AgentRegistry::new());

            // Build HTTP router
            let app_state = AppState {
                handle,
                agents,
                tools,
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
