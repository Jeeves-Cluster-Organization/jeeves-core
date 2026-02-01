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

// Subsystem modules (checkpoint 3)
pub mod lifecycle;
pub mod resources;
pub mod orchestrator;
pub mod interrupts;
pub mod rate_limiter;
