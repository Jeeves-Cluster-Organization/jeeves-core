//! gRPC service implementations.
//!
//! Implements the four main services:
//! - KernelService - Process lifecycle and resource management
//! - EngineService - Envelope operations
//! - OrchestrationService - Kernel-driven pipeline execution
//! - CommBusService - Message bus operations

pub mod commbus_service;
pub mod conversions;
pub mod engine_service;
pub mod kernel_service;
pub mod orchestration_service;

// Re-export conversion helpers if needed
// pub use conversions::*;
pub use commbus_service::CommBusService;
pub use engine_service::EngineService;
pub use kernel_service::KernelServiceImpl;
pub use orchestration_service::OrchestrationService;
