//! Kernel - the main orchestration actor.
//!
//! The Kernel owns all mutable state and processes gRPC commands via a single
//! message channel. Subsystems (lifecycle, resources, interrupts, rate limiter)
//! are plain structs owned by the Kernel, not separate actors.

/// Kernel actor (checkpoint 3 - stub for now).
#[derive(Debug)]
pub struct Kernel {
    // Subsystems will be added in checkpoint 3
}

impl Kernel {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

// Core types
pub mod types;

// Subsystem modules
pub mod interrupts;
pub mod lifecycle;
pub mod orchestrator;
pub mod rate_limiter;
pub mod resources;

// Re-export key types
pub use lifecycle::LifecycleManager;
pub use types::{ProcessControlBlock, ProcessState, ResourceQuota, ResourceUsage, SchedulingPriority};
