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

use crate::types::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Duration};

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
    pub success: bool,
    pub result: Vec<u8>, // JSON-encoded
    pub error: String,
}

// =============================================================================
// Subscriber Management
// =============================================================================

/// Subscriber handle for receiving events.
#[derive(Debug)]
pub struct Subscriber {
    pub id: String,
    pub event_types: Vec<String>,
    pub tx: mpsc::UnboundedSender<Event>,
}

/// Subscription receipt for managing subscriptions.
#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: String,
    pub event_types: Vec<String>,
}

// =============================================================================
// CommBus - In-Memory Message Bus
// =============================================================================

/// Sender type for query handler channels (query + response oneshot).
pub type QueryHandlerSender = mpsc::UnboundedSender<(Query, oneshot::Sender<QueryResponse>)>;

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
    subscribers: HashMap<String, Vec<Subscriber>>,

    /// Command handlers: command_type -> handler channel
    command_handlers: HashMap<String, mpsc::UnboundedSender<Command>>,

    /// Query handlers: query_type -> handler channel
    query_handlers: HashMap<String, QueryHandlerSender>,

    /// Statistics
    stats: BusStats,
}

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

    // =========================================================================
    // Event Pub/Sub
    // =========================================================================

    /// Publish an event to all subscribers.
    ///
    /// This is a fan-out operation - the event is delivered to ALL subscribers
    /// that have registered interest in this event_type.
    pub fn publish(&mut self, event: Event) -> Result<usize> {
        // Find all subscribers interested in this event type
        let interested = self.subscribers
            .get(&event.event_type)
            .map(|subs| subs.as_slice())
            .unwrap_or(&[]);

        let mut delivered = 0;
        for subscriber in interested {
            // Fire-and-forget send to subscriber
            // If channel is closed, subscriber has disconnected (we'll clean them up later)
            if subscriber.tx.send(event.clone()).is_ok() {
                delivered += 1;
            }
        }

        // Update stats
        self.stats.events_published += 1;

        tracing::debug!(
            "Published event type={} to {} subscribers",
            event.event_type,
            delivered
        );

        Ok(delivered)
    }

    /// Subscribe to event types.
    ///
    /// Returns (subscription handle, receiver channel) for receiving events.
    pub fn subscribe(
        &mut self,
        subscriber_id: String,
        event_types: Vec<String>,
    ) -> Result<(Subscription, mpsc::UnboundedReceiver<Event>)> {
        let (tx, rx) = mpsc::unbounded_channel();

        let subscriber = Subscriber {
            id: subscriber_id.clone(),
            event_types: event_types.clone(),
            tx,
        };

        // Register subscriber for each event type
        for event_type in &event_types {
            self.subscribers
                .entry(event_type.clone())
                .or_default()
                .push(Subscriber {
                    id: subscriber.id.clone(),
                    event_types: subscriber.event_types.clone(),
                    tx: subscriber.tx.clone(),
                });
        }

        // Update stats
        self.stats.active_subscribers = self.subscribers.values().map(|v| v.len()).sum();

        tracing::debug!(
            "Subscriber {} registered for events: {:?}",
            subscriber_id,
            event_types
        );

        Ok((
            Subscription {
                id: subscriber_id,
                event_types,
            },
            rx,
        ))
    }

    // =========================================================================
    // Command Routing
    // =========================================================================

    /// Send a command to a registered handler (fire-and-forget).
    ///
    /// # Errors
    ///
    /// Returns error if no handler registered for this command type.
    pub fn send_command(&mut self, command: Command) -> Result<()> {
        let handler = self.command_handlers
            .get(&command.command_type)
            .ok_or_else(|| {
                Error::validation(format!(
                    "No handler registered for command type: {}",
                    command.command_type
                ))
            })?;

        // Fire-and-forget send
        handler.send(command.clone()).map_err(|_| {
            Error::internal(format!(
                "Failed to send command to handler: {}",
                command.command_type
            ))
        })?;

        // Update stats
        self.stats.commands_sent += 1;

        tracing::debug!("Sent command type={}", command.command_type);

        Ok(())
    }

    // =========================================================================
    // Query/Response
    // =========================================================================

    /// Execute a query and wait for response (with timeout).
    ///
    /// # Errors
    ///
    /// Returns error if no handler registered or query times out.
    pub async fn query(&mut self, query: Query) -> Result<QueryResponse> {
        let handler = self.query_handlers
            .get(&query.query_type)
            .ok_or_else(|| {
                Error::validation(format!(
                    "No handler registered for query type: {}",
                    query.query_type
                ))
            })?;

        // Create oneshot channel for response
        let (response_tx, response_rx) = oneshot::channel();

        // Send query to handler
        handler.send((query.clone(), response_tx)).map_err(|_| {
            Error::internal(format!(
                "Failed to send query to handler: {}",
                query.query_type
            ))
        })?;

        // Wait for response with timeout
        let query_timeout = Duration::from_millis(query.timeout_ms);
        let response = timeout(query_timeout, response_rx)
            .await
            .map_err(|_| {
                Error::timeout(format!(
                    "Query timeout after {}ms: {}",
                    query.timeout_ms, query.query_type
                ))
            })?
            .map_err(|_| {
                Error::internal(format!("Query response channel closed: {}", query.query_type))
            })?;

        // Update stats
        self.stats.queries_executed += 1;

        tracing::debug!("Executed query type={}", query.query_type);

        Ok(response)
    }

    /// Register a query handler.
    ///
    /// Returns receiver channel for receiving queries.
    /// Handler must send response via the oneshot channel provided with each query.
    ///
    /// # Errors
    ///
    /// Returns error if handler already registered for this query type.
    pub fn register_query_handler(
        &mut self,
        query_type: String,
    ) -> Result<mpsc::UnboundedReceiver<(Query, oneshot::Sender<QueryResponse>)>> {
        let (tx, rx) = mpsc::unbounded_channel();

        if self.query_handlers.contains_key(&query_type) {
            return Err(Error::validation(format!(
                "Query handler already registered: {}",
                query_type
            )));
        }

        self.query_handlers.insert(query_type.clone(), tx);

        // Update stats
        self.stats.registered_query_handlers = self.query_handlers.len();

        tracing::debug!("Registered query handler: {}", query_type);

        Ok(rx)
    }

    // =========================================================================
    // Command Handler Registration
    // =========================================================================

    /// Register a command handler.
    ///
    /// Returns receiver channel for receiving commands.
    ///
    /// # Errors
    ///
    /// Returns error if handler already registered for this command type.
    pub fn register_command_handler(
        &mut self,
        command_type: String,
    ) -> Result<mpsc::UnboundedReceiver<Command>> {
        let (tx, rx) = mpsc::unbounded_channel();

        if self.command_handlers.contains_key(&command_type) {
            return Err(Error::validation(format!(
                "Command handler already registered: {}",
                command_type
            )));
        }

        self.command_handlers.insert(command_type.clone(), tx);

        // Update stats
        self.stats.registered_command_handlers = self.command_handlers.len();

        tracing::debug!("Registered command handler: {}", command_type);

        Ok(rx)
    }

    // =========================================================================
    // Subscription Management
    // =========================================================================

    /// Unsubscribe a subscriber by ID.
    ///
    /// Removes the subscriber from all event type lists.
    pub fn unsubscribe(&mut self, subscription: &Subscription) {
        for event_type in &subscription.event_types {
            if let Some(subs) = self.subscribers.get_mut(event_type) {
                subs.retain(|s| s.id != subscription.id);
            }
        }
        // Update stats
        self.stats.active_subscribers = self.subscribers.values().map(|v| v.len()).sum();

        tracing::debug!("Unsubscribed: {}", subscription.id);
    }

    /// Get a query handler sender for fire-and-spawn patterns.
    ///
    /// Returns a clone of the sender (read-only access to the handler channel)
    /// so the caller can spawn a task to send the query without holding &mut self.
    pub fn get_query_handler(&self, query_type: &str) -> Option<QueryHandlerSender> {
        self.query_handlers.get(query_type).cloned()
    }

    /// Prune disconnected subscribers from all event type lists.
    pub fn cleanup_disconnected(&mut self) {
        for subs in self.subscribers.values_mut() {
            subs.retain(|s| !s.tx.is_closed());
        }
        // Update stats
        self.stats.active_subscribers = self.subscribers.values().map(|v| v.len()).sum();
    }

    // =========================================================================
    // Statistics
    // =========================================================================

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
    use chrono::Utc;

    // =========================================================================
    // Event Pub/Sub Tests
    // =========================================================================

    #[test]
    fn test_publish_to_zero_subscribers() {
        let mut bus = CommBus::new();

        let event = Event {
            event_type: "test.event".to_string(),
            payload: b"{}".to_vec(),
            timestamp_ms: Utc::now().timestamp_millis(),
            source: "test_process".to_string(),
        };

        let result = bus.publish(event);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // No subscribers

        let stats = bus.get_stats();
        assert_eq!(stats.events_published, 1);
    }

    #[test]
    fn test_subscribe_and_publish() {
        let mut bus = CommBus::new();

        // Subscribe to event
        let (_subscription, mut rx) = bus
            .subscribe(
                "subscriber1".to_string(),
                vec!["test.event".to_string()],
            )
            .unwrap();

        // Publish event
        let event = Event {
            event_type: "test.event".to_string(),
            payload: b"{\"msg\":\"hello\"}".to_vec(),
            timestamp_ms: Utc::now().timestamp_millis(),
            source: "publisher".to_string(),
        };

        let delivered = bus.publish(event.clone()).unwrap();
        assert_eq!(delivered, 1);

        // Receive event
        let received = rx.try_recv().unwrap();
        assert_eq!(received.event_type, "test.event");
        assert_eq!(received.source, "publisher");

    }

    #[test]
    fn test_multiple_subscribers_fan_out() {
        let mut bus = CommBus::new();

        // Subscribe two subscribers to same event
        let (_sub1, mut rx1) = bus
            .subscribe("sub1".to_string(), vec!["test.event".to_string()])
            .unwrap();

        let (_sub2, mut rx2) = bus
            .subscribe("sub2".to_string(), vec!["test.event".to_string()])
            .unwrap();

        // Publish event
        let event = Event {
            event_type: "test.event".to_string(),
            payload: b"{}".to_vec(),
            timestamp_ms: Utc::now().timestamp_millis(),
            source: "publisher".to_string(),
        };

        let delivered = bus.publish(event).unwrap();
        assert_eq!(delivered, 2); // Both subscribers received

        // Both subscribers can receive
        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_subscribe_multiple_event_types() {
        let mut bus = CommBus::new();

        let (_sub, mut rx) = bus
            .subscribe(
                "sub1".to_string(),
                vec!["event.a".to_string(), "event.b".to_string()],
            )
            .unwrap();

        // Publish event A
        let event_a = Event {
            event_type: "event.a".to_string(),
            payload: b"{}".to_vec(),
            timestamp_ms: Utc::now().timestamp_millis(),
            source: "test".to_string(),
        };
        bus.publish(event_a).unwrap();

        // Publish event B
        let event_b = Event {
            event_type: "event.b".to_string(),
            payload: b"{}".to_vec(),
            timestamp_ms: Utc::now().timestamp_millis(),
            source: "test".to_string(),
        };
        bus.publish(event_b).unwrap();

        // Subscriber receives both
        let recv_a = rx.try_recv().unwrap();
        assert_eq!(recv_a.event_type, "event.a");

        let recv_b = rx.try_recv().unwrap();
        assert_eq!(recv_b.event_type, "event.b");
    }

    // =========================================================================
    // Command Tests
    // =========================================================================

    #[test]
    fn test_send_command_no_handler() {
        let mut bus = CommBus::new();

        let command = Command {
            command_type: "test.command".to_string(),
            payload: b"{}".to_vec(),
            source: "test".to_string(),
        };

        let result = bus.send_command(command);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No handler registered"));
    }

    // =========================================================================
    // Query Tests
    // =========================================================================

    #[tokio::test]
    async fn test_query_no_handler() {
        let mut bus = CommBus::new();

        let query = Query {
            query_type: "test.query".to_string(),
            payload: b"{}".to_vec(),
            timeout_ms: 1000,
            source: "test".to_string(),
        };

        let result = bus.query(query).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No handler registered"));
    }

    #[tokio::test]
    async fn test_register_and_execute_query() {
        let mut bus = CommBus::new();

        // Register handler
        let mut rx = bus
            .register_query_handler("test.query".to_string())
            .unwrap();

        // Spawn handler task
        tokio::spawn(async move {
            if let Some((_query, response_tx)) = rx.recv().await {
                // Handler processes query and sends response
                let response = QueryResponse {
                    success: true,
                    result: b"{\"answer\":42}".to_vec(),
                    error: String::new(),
                };
                let _ = response_tx.send(response);
            }
        });

        // Execute query
        let query = Query {
            query_type: "test.query".to_string(),
            payload: b"{\"question\":\"meaning of life\"}".to_vec(),
            timeout_ms: 1000,
            source: "querier".to_string(),
        };

        let response = bus.query(query).await.unwrap();
        assert!(response.success);
        assert_eq!(response.result, b"{\"answer\":42}");

        // Check stats
        let stats = bus.get_stats();
        assert_eq!(stats.queries_executed, 1);
        assert_eq!(stats.registered_query_handlers, 1);
    }

    #[tokio::test]
    async fn test_query_timeout() {
        let mut bus = CommBus::new();

        // Register handler that never responds
        let mut _rx = bus
            .register_query_handler("test.query".to_string())
            .unwrap();

        // Query with short timeout
        let query = Query {
            query_type: "test.query".to_string(),
            payload: b"{}".to_vec(),
            timeout_ms: 100, // 100ms timeout
            source: "test".to_string(),
        };

        let result = bus.query(query).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("timeout"));
    }

    #[tokio::test]
    async fn test_query_error_response() {
        let mut bus = CommBus::new();

        let mut rx = bus
            .register_query_handler("test.query".to_string())
            .unwrap();

        // Handler returns error response
        tokio::spawn(async move {
            if let Some((_query, response_tx)) = rx.recv().await {
                let response = QueryResponse {
                    success: false,
                    result: vec![],
                    error: "Something went wrong".to_string(),
                };
                let _ = response_tx.send(response);
            }
        });

        let query = Query {
            query_type: "test.query".to_string(),
            payload: b"{}".to_vec(),
            timeout_ms: 1000,
            source: "test".to_string(),
        };

        let response = bus.query(query).await.unwrap();
        assert!(!response.success);
        assert_eq!(response.error, "Something went wrong");
    }

    // =========================================================================
    // Statistics Tests
    // =========================================================================

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

    // =========================================================================
    // Unsubscribe Tests
    // =========================================================================

    #[test]
    fn test_unsubscribe_removes_subscriber() {
        let mut bus = CommBus::new();

        let (sub, _rx) = bus
            .subscribe("sub1".to_string(), vec!["test.event".to_string()])
            .unwrap();

        assert_eq!(bus.get_stats().active_subscribers, 1);

        bus.unsubscribe(&sub);

        // After unsubscribe, publishing should deliver to zero subscribers
        let event = Event {
            event_type: "test.event".to_string(),
            payload: b"{}".to_vec(),
            timestamp_ms: Utc::now().timestamp_millis(),
            source: "test".to_string(),
        };
        let delivered = bus.publish(event).unwrap();
        assert_eq!(delivered, 0);
    }

    #[test]
    fn test_unsubscribe_only_affects_target() {
        let mut bus = CommBus::new();

        let (sub1, _rx1) = bus
            .subscribe("sub1".to_string(), vec!["test.event".to_string()])
            .unwrap();
        let (_sub2, mut rx2) = bus
            .subscribe("sub2".to_string(), vec!["test.event".to_string()])
            .unwrap();

        // Unsubscribe only sub1
        bus.unsubscribe(&sub1);

        let event = Event {
            event_type: "test.event".to_string(),
            payload: b"{}".to_vec(),
            timestamp_ms: Utc::now().timestamp_millis(),
            source: "test".to_string(),
        };
        let delivered = bus.publish(event).unwrap();
        assert_eq!(delivered, 1); // Only sub2

        assert!(rx2.try_recv().is_ok());
    }

    // =========================================================================
    // get_query_handler Tests
    // =========================================================================

    #[test]
    fn test_get_query_handler_returns_none_when_missing() {
        let bus = CommBus::new();
        assert!(bus.get_query_handler("nonexistent").is_none());
    }

    #[test]
    fn test_get_query_handler_returns_sender() {
        let mut bus = CommBus::new();
        let _rx = bus.register_query_handler("test.query".to_string()).unwrap();

        let handler = bus.get_query_handler("test.query");
        assert!(handler.is_some());
    }

    // =========================================================================
    // register_command_handler Tests
    // =========================================================================

    #[test]
    fn test_register_command_handler_and_send() {
        let mut bus = CommBus::new();

        let mut rx = bus.register_command_handler("test.cmd".to_string()).unwrap();

        let command = Command {
            command_type: "test.cmd".to_string(),
            payload: b"{\"action\":\"do_thing\"}".to_vec(),
            source: "test".to_string(),
        };

        bus.send_command(command).unwrap();
        let received = rx.try_recv().unwrap();
        assert_eq!(received.command_type, "test.cmd");
    }

    // =========================================================================
    // cleanup_disconnected Tests
    // =========================================================================

    #[test]
    fn test_cleanup_disconnected_prunes_dead_subscribers() {
        let mut bus = CommBus::new();

        let (_sub1, rx1) = bus
            .subscribe("sub1".to_string(), vec!["test.event".to_string()])
            .unwrap();
        let (_sub2, _rx2) = bus
            .subscribe("sub2".to_string(), vec!["test.event".to_string()])
            .unwrap();

        assert_eq!(bus.get_stats().active_subscribers, 2);

        // Drop sub1's receiver — makes it disconnected
        drop(rx1);

        // Cleanup should prune the disconnected subscriber
        bus.cleanup_disconnected();
        assert_eq!(bus.get_stats().active_subscribers, 1);

        // Publishing should only deliver to sub2
        let event = Event {
            event_type: "test.event".to_string(),
            payload: b"{}".to_vec(),
            timestamp_ms: Utc::now().timestamp_millis(),
            source: "test".to_string(),
        };
        let delivered = bus.publish(event).unwrap();
        assert_eq!(delivered, 1);
    }

}
