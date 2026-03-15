//! Communication bus for pub/sub and request/response patterns.
//!
//! This module provides kernel-mediated inter-process communication (IPC) for the agentic OS.
//! All agent communication flows through the kernel, enabling:
//!   - Message quotas and rate limiting
//!   - Full tracing and observability
//!   - Security and access control
//!   - Fault isolation
//!
//! Patterns supported:
//!   - **Events**: Pub/sub with fan-out to all subscribers
//!   - **Commands**: Fire-and-forget to single handler
//!   - **Queries**: Request/response with timeout

pub mod types;
mod event_bus;
mod command_bus;
mod query_bus;

pub use types::*;

use std::collections::HashMap;
use tokio::sync::mpsc;

// =============================================================================
// CommBus - In-Memory Message Bus
// =============================================================================

/// In-memory communication bus for kernel-mediated IPC.
///
/// This bus provides:
///   - Event pub/sub (fan-out to all subscribers)
///   - Command routing (fire-and-forget to single handler)
///   - Query/response (request-response with timeout)
///
/// All messages flow through the kernel for observability and control.
/// Owned by the Kernel (single-actor model) — no concurrent access.
#[derive(Debug)]
pub struct CommBus {
    /// Event subscribers: event_type -> list of subscribers
    pub(crate) subscribers: HashMap<String, Vec<Subscriber>>,

    /// Command handlers: command_type -> handler channel
    pub(crate) command_handlers: HashMap<String, mpsc::UnboundedSender<Command>>,

    /// Query handlers: query_type -> handler channel
    pub(crate) query_handlers: HashMap<String, QueryHandlerSender>,

    /// Statistics
    pub(crate) stats: BusStats,
}

impl CommBus {
    /// Create a new CommBus instance.
    pub fn new() -> Self {
        Self {
            subscribers: HashMap::new(),
            command_handlers: HashMap::new(),
            query_handlers: HashMap::new(),
            stats: BusStats::default(),
        }
    }

    /// Get current bus statistics.
    pub fn get_stats(&self) -> BusStats {
        self.stats.clone()
    }
}

impl Default for CommBus {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_stats() {
        let mut bus = CommBus::new();

        let initial_stats = bus.get_stats();
        assert_eq!(initial_stats.events_published, 0);
        assert_eq!(initial_stats.commands_sent, 0);
        assert_eq!(initial_stats.queries_executed, 0);

        // Subscribe
        let (_sub, _rx) = bus
            .subscribe("sub1".to_string(), vec!["test.event".to_string()])
            .unwrap();

        let stats = bus.get_stats();
        assert_eq!(stats.active_subscribers, 1);
    }
}
