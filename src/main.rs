//! Jeeves kernel gRPC server - main entry point.
//!
//! Starts a gRPC server with all 4 services:
//! - KernelService: Process lifecycle and resource management
//! - EngineService: Envelope operations
//! - OrchestrationService: Kernel-driven pipeline execution
//! - CommBusService: Message bus operations

use jeeves_core::Config;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Server;

// Import service implementations
use jeeves_core::grpc::{
    CommBusService, EngineService, KernelServiceImpl, OrchestrationService,
};
use jeeves_core::kernel::Kernel;

// Import generated proto server traits
use jeeves_core::proto::comm_bus_service_server::CommBusServiceServer;
use jeeves_core::proto::engine_service_server::EngineServiceServer;
use jeeves_core::proto::kernel_service_server::KernelServiceServer;
use jeeves_core::proto::orchestration_service_server::OrchestrationServiceServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = Config::default();

    // Initialize observability
    jeeves_core::observability::init_tracing();

    // Create shared kernel instance (all services share one kernel)
    let kernel = Arc::new(Mutex::new(Kernel::new()));

    // Create all 4 gRPC services sharing the same kernel
    let kernel_service = KernelServiceImpl::new(kernel.clone());
    let engine_service = EngineService::new(kernel.clone());
    let orchestration_service = OrchestrationService::new(kernel.clone());
    let commbus_service = CommBusService::new(kernel.clone());

    // Bind address from config
    let addr = config.server.grpc_addr.parse()?;

    tracing::info!("🚀 Jeeves Kernel gRPC server starting on {}", addr);
    tracing::info!("  ✓ KernelService: Process lifecycle and resources");
    tracing::info!("  ✓ EngineService: Envelope operations");
    tracing::info!("  ✓ OrchestrationService: Kernel-driven execution");
    tracing::info!("  ✓ CommBusService: Message bus");

    // Build and start gRPC server
    Server::builder()
        .add_service(KernelServiceServer::new(kernel_service))
        .add_service(EngineServiceServer::new(engine_service))
        .add_service(OrchestrationServiceServer::new(orchestration_service))
        .add_service(CommBusServiceServer::new(commbus_service))
        .serve(addr)
        .await?;

    Ok(())
}
