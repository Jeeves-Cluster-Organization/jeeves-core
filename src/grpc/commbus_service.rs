//! CommBusService - Message bus operations for Agentic OS IPC.
//!
//! This service handles:
//!   - Event publishing (fire-and-forget, fan-out)
//!   - Command sending (fire-and-forget)
//!   - Query execution (request-response)
//!   - Event subscription (server streaming)

use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use crate::kernel::Kernel;

// Import generated proto types
use crate::proto::comm_bus_service_server::CommBusService as CommBusServiceTrait;
use crate::proto::{
    CommBusEvent, CommBusPublishRequest, CommBusPublishResponse, CommBusQueryRequest,
    CommBusQueryResponse, CommBusSendRequest, CommBusSendResponse, CommBusSubscribeRequest,
};

/// CommBusService implementation.
#[derive(Debug, Clone)]
pub struct CommBusService {
    kernel: Arc<Mutex<Kernel>>,
}

impl CommBusService {
    /// Create a new CommBusService with a shared kernel.
    pub fn new(kernel: Arc<Mutex<Kernel>>) -> Self {
        Self { kernel }
    }
}

#[tonic::async_trait]
impl CommBusServiceTrait for CommBusService {
    /// Publish event to all subscribers (fire-and-forget, fan-out).
    async fn publish(
        &self,
        request: Request<CommBusPublishRequest>,
    ) -> Result<Response<CommBusPublishResponse>, Status> {
        let req = request.into_inner();

        // Placeholder implementation
        // Full implementation would:
        // 1. Parse event payload
        // 2. Broadcast to all subscribers for this event_type
        // 3. Return immediately (fire-and-forget)

        tracing::debug!(
            "CommBus: Publishing event type={} payload_size={}",
            req.event_type,
            req.payload.len()
        );

        Ok(Response::new(CommBusPublishResponse {
            success: true,
            error: String::new(),
        }))
    }

    /// Send command to single handler (fire-and-forget).
    async fn send(
        &self,
        request: Request<CommBusSendRequest>,
    ) -> Result<Response<CommBusSendResponse>, Status> {
        let req = request.into_inner();

        // Placeholder implementation
        // Full implementation would:
        // 1. Parse command payload
        // 2. Route to registered handler for this command_type
        // 3. Return immediately (fire-and-forget)

        tracing::debug!(
            "CommBus: Sending command type={} payload_size={}",
            req.command_type,
            req.payload.len()
        );

        Ok(Response::new(CommBusSendResponse {
            success: true,
            error: String::new(),
        }))
    }

    /// Query with response (request-response, synchronous).
    async fn query(
        &self,
        request: Request<CommBusQueryRequest>,
    ) -> Result<Response<CommBusQueryResponse>, Status> {
        let req = request.into_inner();

        // Placeholder implementation
        // Full implementation would:
        // 1. Parse query payload
        // 2. Execute query against registered handler
        // 3. Wait for response (with timeout)
        // 4. Return result

        tracing::debug!(
            "CommBus: Executing query type={} payload_size={} timeout_ms={}",
            req.query_type,
            req.payload.len(),
            req.timeout_ms
        );

        Ok(Response::new(CommBusQueryResponse {
            success: true,
            result: vec![], // Empty JSON object
            error: String::new(),
        }))
    }

    /// Subscribe to events (server streaming).
    type SubscribeStream = Pin<Box<dyn Stream<Item = Result<CommBusEvent, Status>> + Send>>;

    async fn subscribe(
        &self,
        request: Request<CommBusSubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let req = request.into_inner();

        // Clone Arc before stream to satisfy 'static lifetime
        let _kernel = self.kernel.clone();
        let event_types = req.event_types;

        tracing::debug!(
            "CommBus: Client subscribing to event types: {:?}",
            event_types
        );

        // Create event stream
        // Placeholder implementation - returns one test event then closes
        // Full implementation would:
        // 1. Register subscriber for specified event_types
        // 2. Stream events as they occur
        // 3. Handle unsubscribe on stream close

        let stream = async_stream::stream! {
            // Send one test event
            let event = CommBusEvent {
                event_type: "system.test".to_string(),
                payload: b"{}".to_vec(), // Empty JSON
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            };

            yield Ok(event);

            // Keep stream open for a moment
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            // Stream will close when this scope ends
        };

        Ok(Response::new(Box::pin(stream)))
    }
}
