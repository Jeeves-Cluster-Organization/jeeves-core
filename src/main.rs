//! Jeeves kernel IPC server — main entry point.

use jeeves_core::ipc::IpcServer;
use jeeves_core::kernel::Kernel;
use jeeves_core::Config;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env();
    jeeves_core::observability::init_tracing_from_config(&config.observability);

    let kernel = Kernel::from_config(&config);
    let addr = config.server.listen_addr.parse()?;

    tracing::info!(
        listen_addr = %addr,
        rate_limit_rpm = config.rate_limit.requests_per_minute,
        max_llm_calls = config.defaults.max_llm_calls,
        "Jeeves Kernel IPC server starting",
    );
    let server = IpcServer::new(kernel, addr, config.ipc);
    server.serve().await?;

    Ok(())
}
