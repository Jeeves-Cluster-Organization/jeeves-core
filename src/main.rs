//! Jeeves kernel — MCP stdio server binary.
//!
//! Reads JSON-RPC from stdin, dispatches to ToolRegistry, writes to stdout.
//! All tracing goes to stderr (safe for MCP protocol on stdout).
//!
//! Usage:
//!   jeeves-kernel                          # Run MCP stdio server
//!   JEEVES_MCP_SERVERS='[...]' jeeves-kernel  # Connect upstream MCP servers as tools

use jeeves_core::worker::mcp_server::McpStdioServer;
use jeeves_core::worker::tools::{ToolExecutor, ToolRegistry};
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Init tracing to stderr (default for tracing-subscriber fmt layer)
    let config = jeeves_core::Config::from_env();
    jeeves_core::observability::init_tracing_from_config(&config.observability);

    // Build tool registry
    let mut tool_registry = ToolRegistry::new();

    // Auto-connect upstream MCP servers from JEEVES_MCP_SERVERS env var.
    // These become tools served by this MCP stdio server (proxy pattern).
    if let Ok(mcp_json) = std::env::var("JEEVES_MCP_SERVERS") {
        if let Ok(servers) =
            serde_json::from_str::<Vec<jeeves_core::types::config::McpServerConfig>>(&mcp_json)
        {
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
                        let tools = executor.list_tools();
                        let tool_count = tools.len();
                        for tool in &tools {
                            tool_registry.register(tool.name.clone(), executor.clone());
                        }
                        tracing::info!(name = %mcp_cfg.name, tools = tool_count, "Upstream MCP server connected");
                    }
                    Err(e) => {
                        tracing::error!(name = %mcp_cfg.name, error = %e, "Upstream MCP server connect failed");
                    }
                }
            }
        }
    }

    let tools = Arc::new(tool_registry);
    let server = McpStdioServer::new(
        tools,
        "jeeves-kernel".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    );

    tracing::info!("MCP stdio server starting (reading from stdin)");
    server.run().await?;

    Ok(())
}
