//! Communication bus for pub/sub and request/response patterns.

/// CommBus (checkpoint 7 - stub for now).
#[derive(Debug)]
pub struct CommBus {
    // Message bus will be implemented in checkpoint 7
}

impl CommBus {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for CommBus {
    fn default() -> Self {
        Self::new()
    }
}
