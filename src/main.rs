//! Jeeves kernel — single-process runtime.

use clap::Parser;
use jeeves_core::kernel::Kernel;
use jeeves_core::worker::actor::spawn_kernel;
use jeeves_core::worker::agent::{AgentRegistry, LlmAgent, McpDelegatingAgent, DeterministicAgent};
use jeeves_core::worker::gateway::{build_router, AppState};
use jeeves_core::worker::llm::openai::OpenAiProvider;
use jeeves_core::worker::prompts::PromptRegistry;
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
                                let tools = executor.list_tools();
                                let tool_count = tools.len();
                                // Register per tool name so ToolRegistry.execute() finds by tool name
                                for tool in &tools {
                                    tool_registry.register(tool.name.clone(), executor.clone());
                                }
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

            // Load prompt registry from JEEVES_PROMPTS_DIR (default: ./prompts)
            let prompts_dir = std::env::var("JEEVES_PROMPTS_DIR").unwrap_or_else(|_| "./prompts".to_string());
            let prompts = Arc::new(PromptRegistry::from_dir(&prompts_dir));
            tracing::info!(dir = %prompts_dir, "Prompt registry loaded");

            // Build LLM provider (shared across all LLM agents)
            let llm: Arc<dyn jeeves_core::worker::llm::LlmProvider> = {
                let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
                let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
                let mut provider = OpenAiProvider::new(api_key, model);
                if let Ok(base_url) = std::env::var("OPENAI_BASE_URL") {
                    provider = provider.with_base_url(base_url);
                }
                Arc::new(provider)
            };

            // Build agent registry from JEEVES_AGENTS env var
            let mut agent_registry = AgentRegistry::new();
            if let Ok(agents_json) = std::env::var("JEEVES_AGENTS") {
                match serde_json::from_str::<Vec<jeeves_core::types::AgentConfig>>(&agents_json) {
                    Ok(agent_configs) => {
                        for cfg in agent_configs {
                            let agent: Arc<dyn jeeves_core::worker::agent::Agent> = match cfg.agent_type.as_str() {
                                "llm" => Arc::new(LlmAgent {
                                    llm: llm.clone(),
                                    prompts: prompts.clone(),
                                    tools: tools.clone(),
                                    prompt_key: cfg.prompt_key.unwrap_or_default(),
                                    temperature: cfg.temperature,
                                    max_tokens: cfg.max_tokens,
                                    model: cfg.model,
                                    max_tool_rounds: 10,
                                }),
                                "mcp_delegate" => {
                                    let tool_name = cfg.tool_name.unwrap_or_else(|| cfg.name.clone());
                                    Arc::new(McpDelegatingAgent {
                                        tool_name,
                                        tools: tools.clone(),
                                    })
                                }
                                "deterministic" | "gate" => Arc::new(DeterministicAgent),
                                other => {
                                    tracing::warn!(name = %cfg.name, agent_type = other, "Unknown agent type, skipping");
                                    continue;
                                }
                            };
                            tracing::info!(name = %cfg.name, agent_type = %cfg.agent_type, "Agent registered");
                            agent_registry.register(cfg.name, agent);
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "Failed to parse JEEVES_AGENTS"),
                }
            }
            let agents = Arc::new(agent_registry);

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
