//! EngineService - Envelope lifecycle and execution operations.
//!
//! This service handles:
//!   - Envelope creation and updates
//!   - Bounds checking
//!   - Pipeline execution (streaming)
//!   - Agent execution
//!   - Envelope cloning

use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use crate::envelope::Envelope;
use crate::kernel::Kernel;
use crate::types::{EnvelopeId, ProcessId, RequestId, SessionId, UserId};

// Import generated proto types
use crate::proto::engine_service_server::EngineService as EngineServiceTrait;
use crate::proto::{
    AgentResult, BoundsResult, CloneRequest, CreateEnvelopeRequest, ExecuteAgentRequest,
    ExecuteRequest, ExecutionEvent, UpdateEnvelopeRequest,
};

/// EngineService implementation.
#[derive(Debug, Clone)]
pub struct EngineService {
    kernel: Arc<Mutex<Kernel>>,
}

impl EngineService {
    /// Create a new EngineService with a shared kernel.
    pub fn new(kernel: Arc<Mutex<Kernel>>) -> Self {
        Self { kernel }
    }

    /// Helper: Convert domain Envelope to proto Envelope.
    fn envelope_to_proto(envelope: Envelope) -> crate::proto::Envelope {
        // Use existing conversion logic
        envelope.into()
    }
}

#[tonic::async_trait]
impl EngineServiceTrait for EngineService {
    // =========================================================================
    // Envelope Lifecycle
    // =========================================================================

    /// CreateEnvelope creates a new envelope with initial metadata.
    async fn create_envelope(
        &self,
        request: Request<CreateEnvelopeRequest>,
    ) -> Result<Response<crate::proto::Envelope>, Status> {
        let req = request.into_inner();

        // Generate envelope ID and request ID if not provided
        let envelope_id = EnvelopeId::must(format!("env_{}", uuid::Uuid::new_v4().simple()));
        let request_id = if req.request_id.is_empty() {
            RequestId::must(format!("req_{}", uuid::Uuid::new_v4().simple()))
        } else {
            RequestId::from_string(req.request_id)
                .map_err(|e| Status::invalid_argument(e.to_string()))?
        };
        let user_id = UserId::from_string(req.user_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let session_id = SessionId::from_string(req.session_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        // Create envelope with defaults
        let mut envelope = Envelope::new();

        // Override with request values
        envelope.identity.envelope_id = envelope_id;
        envelope.identity.request_id = request_id;
        envelope.identity.user_id = user_id;
        envelope.identity.session_id = session_id;
        envelope.raw_input = req.raw_input;

        // Set stage order if provided
        if !req.stage_order.is_empty() {
            envelope.pipeline.stage_order = req.stage_order;
            envelope.pipeline.current_stage = envelope.pipeline.stage_order[0].clone();
        }

        // Store envelope in kernel
        let mut kernel = self.kernel.lock().await;
        kernel.store_envelope(envelope.clone());

        // Convert to proto
        let proto_envelope = Self::envelope_to_proto(envelope);

        Ok(Response::new(proto_envelope))
    }

    /// UpdateEnvelope updates an existing envelope.
    async fn update_envelope(
        &self,
        request: Request<UpdateEnvelopeRequest>,
    ) -> Result<Response<crate::proto::Envelope>, Status> {
        let req = request.into_inner();

        let proto_envelope = req
            .envelope
            .ok_or_else(|| Status::invalid_argument("envelope is required"))?;

        // Convert proto to domain envelope
        let envelope: Envelope = proto_envelope
            .try_into()
            .map_err(|e| Status::internal(format!("Failed to convert envelope: {}", e)))?;

        // Update envelope in kernel
        let mut kernel = self.kernel.lock().await;
        let envelope_id = envelope.identity.envelope_id.to_string();

        if kernel.get_envelope(&envelope_id).is_none() {
            return Err(Status::not_found(format!(
                "Envelope not found: {}",
                envelope_id
            )));
        }

        kernel.store_envelope(envelope.clone());

        // Convert back to proto
        let proto_envelope = Self::envelope_to_proto(envelope);

        Ok(Response::new(proto_envelope))
    }

    // =========================================================================
    // Bounds Checking
    // =========================================================================

    /// CheckBounds checks if envelope is within execution bounds.
    async fn check_bounds(
        &self,
        request: Request<crate::proto::Envelope>,
    ) -> Result<Response<BoundsResult>, Status> {
        let proto_envelope = request.into_inner();

        // Convert proto to domain envelope
        let envelope: Envelope = proto_envelope
            .try_into()
            .map_err(|e| Status::internal(format!("Failed to convert envelope: {}", e)))?;

        // Check bounds
        let can_continue = envelope.bounds.llm_call_count < envelope.bounds.max_llm_calls
            && envelope.pipeline.iteration < envelope.pipeline.max_iterations
            && envelope.bounds.agent_hop_count < envelope.bounds.max_agent_hops;

        // Determine terminal reason if bounds exceeded
        let terminal_reason = if envelope.bounds.llm_call_count >= envelope.bounds.max_llm_calls {
            crate::proto::TerminalReason::MaxLlmCallsExceeded as i32
        } else if envelope.pipeline.iteration >= envelope.pipeline.max_iterations {
            crate::proto::TerminalReason::MaxIterationsExceeded as i32
        } else if envelope.bounds.agent_hop_count >= envelope.bounds.max_agent_hops {
            crate::proto::TerminalReason::MaxAgentHopsExceeded as i32
        } else {
            crate::proto::TerminalReason::Unspecified as i32
        };

        let result = BoundsResult {
            can_continue,
            terminal_reason,
            llm_calls_remaining: (envelope.bounds.max_llm_calls - envelope.bounds.llm_call_count).max(0),
            agent_hops_remaining: (envelope.bounds.max_agent_hops - envelope.bounds.agent_hop_count).max(0),
            iterations_remaining: (envelope.pipeline.max_iterations - envelope.pipeline.iteration).max(0),
        };

        Ok(Response::new(result))
    }

    // =========================================================================
    // Execution
    // =========================================================================

    /// ExecutePipeline executes a pipeline and streams events.
    type ExecutePipelineStream =
        Pin<Box<dyn Stream<Item = Result<ExecutionEvent, Status>> + Send>>;

    async fn execute_pipeline(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<Self::ExecutePipelineStream>, Status> {
        let req = request.into_inner();

        let proto_envelope = req
            .envelope
            .ok_or_else(|| Status::invalid_argument("envelope is required"))?;

        // Convert proto to domain envelope
        let envelope: Envelope = proto_envelope
            .try_into()
            .map_err(|e| Status::internal(format!("Failed to convert envelope: {}", e)))?;

        // Parse pipeline config from bytes
        let pipeline_config: crate::kernel::orchestrator::PipelineConfig =
            serde_json::from_slice(&req.pipeline_config).map_err(|e| {
                Status::invalid_argument(format!("Invalid pipeline config: {}", e))
            })?;

        // Initialize orchestration session
        let process_id = ProcessId::must(envelope.identity.envelope_id.to_string());
        let mut kernel = self.kernel.lock().await;

        let _session_state = kernel
            .initialize_orchestration(process_id.clone(), pipeline_config, envelope, false)
            .map_err(|e| Status::internal(format!("Failed to initialize orchestration: {}", e)))?;

        drop(kernel); // Release lock before streaming

        // Create event stream
        // Note: This is a placeholder implementation. Full implementation would:
        // 1. Execute agents in a loop
        // 2. Stream events as they occur
        // 3. Handle interrupts and errors
        // For now, we'll return a simple completion event

        // Clone Arc before stream to satisfy 'static lifetime
        let kernel = self.kernel.clone();
        let process_id_clone = process_id.clone();

        let stream = async_stream::stream! {
            // Get kernel reference for streaming
            let mut kernel_guard = kernel.lock().await;

            // Get next instruction
            match kernel_guard.get_next_instruction(&process_id_clone) {
                Ok(instruction) => {
                    // Create event from instruction
                    let event = ExecutionEvent {
                        r#type: crate::proto::ExecutionEventType::PipelineCompleted as i32,
                        stage: instruction.agent_name.unwrap_or_default(),
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                        payload: vec![],
                        envelope: instruction.envelope.map(Self::envelope_to_proto),
                    };
                    yield Ok(event);
                }
                Err(e) => {
                    yield Err(Status::internal(format!("Pipeline execution failed: {}", e)));
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    /// ExecuteAgent executes a single agent and returns the result.
    async fn execute_agent(
        &self,
        request: Request<ExecuteAgentRequest>,
    ) -> Result<Response<AgentResult>, Status> {
        let req = request.into_inner();

        let proto_envelope = req
            .envelope
            .ok_or_else(|| Status::invalid_argument("envelope is required"))?;

        // Convert proto to domain envelope
        let envelope: Envelope = proto_envelope
            .try_into()
            .map_err(|e| Status::internal(format!("Failed to convert envelope: {}", e)))?;

        // Agent execution is a placeholder - full implementation would:
        // 1. Parse agent_config
        // 2. Execute the agent
        // 3. Update envelope with results
        // 4. Return updated envelope and metrics

        // For now, return success with unchanged envelope
        let result = AgentResult {
            success: true,
            output: vec![], // JSON-encoded output would go here
            error: String::new(),
            duration_ms: 0,
            llm_calls: 0,
            envelope: Some(Self::envelope_to_proto(envelope)),
        };

        Ok(Response::new(result))
    }

    // =========================================================================
    // State Management
    // =========================================================================

    /// CloneEnvelope creates a deep copy of an envelope.
    async fn clone_envelope(
        &self,
        request: Request<CloneRequest>,
    ) -> Result<Response<crate::proto::Envelope>, Status> {
        let req = request.into_inner();

        let proto_envelope = req
            .envelope
            .ok_or_else(|| Status::invalid_argument("envelope is required"))?;

        // Convert proto to domain envelope
        let envelope: Envelope = proto_envelope
            .try_into()
            .map_err(|e| Status::internal(format!("Failed to convert envelope: {}", e)))?;

        // Clone envelope with new ID
        let mut cloned = envelope.clone();
        cloned.identity.envelope_id = EnvelopeId::must(format!("env_{}", uuid::Uuid::new_v4().simple()));

        // Store cloned envelope in kernel
        let mut kernel = self.kernel.lock().await;
        kernel.store_envelope(cloned.clone());

        // Convert to proto
        let proto_envelope = Self::envelope_to_proto(cloned);

        Ok(Response::new(proto_envelope))
    }
}
