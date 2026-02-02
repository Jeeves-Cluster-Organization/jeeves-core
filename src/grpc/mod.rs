//! gRPC service implementations.
//!
//! Implements the four main services:
//! - KernelService - Process lifecycle and resource management
//! - EngineService - Envelope operations
//! - OrchestrationService - Kernel-driven pipeline execution
//! - CommBusService - Message bus operations

pub mod conversions;
pub mod kernel_service;

// Re-export conversion helpers if needed
// pub use conversions::*;
pub use kernel_service::KernelServiceImpl;
