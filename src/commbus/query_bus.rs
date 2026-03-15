//! Query/response methods for CommBus.

use super::types::{Query, QueryHandlerSender, QueryResponse};
use super::CommBus;
use crate::types::{Error, Result};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Duration};

impl CommBus {
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

    /// Get a query handler sender for fire-and-spawn patterns.
    ///
    /// Returns a clone of the sender (read-only access to the handler channel)
    /// so the caller can spawn a task to send the query without holding &mut self.
    pub fn get_query_handler(&self, query_type: &str) -> Option<QueryHandlerSender> {
        self.query_handlers.get(query_type).cloned()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::super::*;

    #[tokio::test]
    async fn test_query_no_handler() {
        let mut bus = CommBus::new();

        let result = bus.query(Query::test("test.query", b"{}".to_vec(), 1000)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No handler registered"));
    }

    #[tokio::test]
    async fn test_register_and_execute_query() {
        let mut bus = CommBus::new();

        let mut rx = bus
            .register_query_handler("test.query".to_string())
            .unwrap();

        tokio::spawn(async move {
            if let Some((_query, response_tx)) = rx.recv().await {
                let response = QueryResponse { result: b"{\"answer\":42}".to_vec(), error: None };
                let _ = response_tx.send(response);
            }
        });

        let response = bus.query(Query::test("test.query", b"{\"question\":\"meaning of life\"}".to_vec(), 1000)).await.unwrap();
        assert!(response.success());
        assert_eq!(response.result, b"{\"answer\":42}");

        let stats = bus.get_stats();
        assert_eq!(stats.queries_executed, 1);
        assert_eq!(stats.registered_query_handlers, 1);
    }

    #[tokio::test]
    async fn test_query_timeout() {
        let mut bus = CommBus::new();

        let mut _rx = bus
            .register_query_handler("test.query".to_string())
            .unwrap();

        let result = bus.query(Query::test("test.query", b"{}".to_vec(), 100)).await;
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

        tokio::spawn(async move {
            if let Some((_query, response_tx)) = rx.recv().await {
                let response = QueryResponse { result: vec![], error: Some("Something went wrong".to_string()) };
                let _ = response_tx.send(response);
            }
        });

        let response = bus.query(Query::test("test.query", b"{}".to_vec(), 1000)).await.unwrap();
        assert!(!response.success());
        assert_eq!(response.error.as_deref(), Some("Something went wrong"));
    }

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
}
