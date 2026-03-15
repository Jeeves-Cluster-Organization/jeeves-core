//! Data types for the communication bus.

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

// =============================================================================
// Message Types
// =============================================================================

/// Event message for pub/sub pattern.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub event_type: String,
    pub payload: Vec<u8>, // JSON-encoded
    pub timestamp_ms: i64,
    pub source: String, // Process ID that published the event
}

/// Command message for fire-and-forget pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub command_type: String,
    pub payload: Vec<u8>, // JSON-encoded
    pub source: String,   // Process ID that sent the command
}

/// Query message for request/response pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub query_type: String,
    pub payload: Vec<u8>, // JSON-encoded
    pub timeout_ms: u64,
    pub source: String, // Process ID that issued the query
}

/// Response to a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponse {
    pub result: Vec<u8>, // JSON-encoded
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl QueryResponse {
    /// True when the query completed without error.
    pub fn success(&self) -> bool {
        self.error.is_none()
    }
}

// =============================================================================
// Test Constructors
// =============================================================================

#[cfg(test)]
impl Event {
    /// Test helper: build an event with default source and timestamp.
    pub fn test(event_type: &str, payload: Vec<u8>) -> Self {
        Self {
            event_type: event_type.to_string(),
            payload,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            source: "test".to_string(),
        }
    }
}

#[cfg(test)]
impl Query {
    /// Test helper: build a query with default source.
    pub fn test(query_type: &str, payload: Vec<u8>, timeout_ms: u64) -> Self {
        Self {
            query_type: query_type.to_string(),
            payload,
            timeout_ms,
            source: "test".to_string(),
        }
    }
}

#[cfg(test)]
impl Command {
    /// Test helper: build a command with default source.
    pub fn test(command_type: &str, payload: Vec<u8>) -> Self {
        Self {
            command_type: command_type.to_string(),
            payload,
            source: "test".to_string(),
        }
    }
}

// =============================================================================
// Subscriber Management
// =============================================================================

/// Bounded channel capacity for all CommBus channels.
pub const CHANNEL_CAPACITY: usize = 256;

/// Subscriber handle for receiving events.
#[derive(Debug)]
pub struct Subscriber {
    pub id: String,
    pub event_types: Vec<String>,
    pub tx: mpsc::Sender<Event>,
}

/// Subscription receipt for managing subscriptions.
#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: String,
    pub event_types: Vec<String>,
}

// =============================================================================
// Handler Types
// =============================================================================

/// Sender type for query handler channels (query + response oneshot).
pub type QueryHandlerSender = mpsc::Sender<(Query, oneshot::Sender<QueryResponse>)>;

// =============================================================================
// Statistics
// =============================================================================

/// Statistics about bus usage.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BusStats {
    pub events_published: u64,
    pub commands_sent: u64,
    pub queries_executed: u64,
    pub active_subscribers: usize,
    pub registered_command_handlers: usize,
    pub registered_query_handlers: usize,
}
