//! Jeeves kernel IPC server — main entry point.

use jeeves_core::ipc::IpcServer;
use jeeves_core::kernel::Kernel;
use jeeves_core::Config;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config = Config::default();
    jeeves_core::observability::init_tracing();

    let kernel = Arc::new(Mutex::new(Kernel::new()));
    let addr = config.server.listen_addr.parse()?;

    tracing::info!("Jeeves Kernel IPC server starting on {}", addr);
    let server = IpcServer::new(kernel, addr, config.ipc);
    server.serve().await?;

    Ok(())
}
