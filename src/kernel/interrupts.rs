//! Flow interrupt management.

/// Interrupt service (checkpoint 7 - stub for now).
#[derive(Debug)]
pub struct InterruptService {
    // Interrupt handling will be implemented in checkpoint 7
}

impl InterruptService {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for InterruptService {
    fn default() -> Self {
        Self::new()
    }
}
