//! OrchestrationService - Kernel-driven pipeline execution.
//!
//! This service handles:
//!   - Session initialization with pipeline config
//!   - Instruction retrieval (what agent to run next)
//!   - Agent result reporting
//!   - Session state queries

use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

use crate::envelope::Envelope;
use crate::kernel::orchestrator::{AgentExecutionMetrics, PipelineConfig};
use crate::kernel::Kernel;

// Import generated proto types
use crate::proto::orchestration_service_server::OrchestrationService as OrchestrationServiceTrait;
use crate::proto::{
    GetNextInstructionRequest, GetSessionStateRequest, InitializeSessionRequest, Instruction,
    ReportAgentResultRequest, SessionState,
};

/// OrchestrationService implementation.
#[derive(Debug, Clone)]
pub struct OrchestrationService {
    kernel: Arc<Mutex<Kernel>>,
}

impl OrchestrationService {
    /// Create a new OrchestrationService with a shared kernel.
    pub fn new(kernel: Arc<Mutex<Kernel>>) -> Self {
        Self { kernel }
    }
}

#[tonic::async_trait]
impl OrchestrationServiceTrait for OrchestrationService {
    /// InitializeSession creates a new orchestration session with pipeline config.
    async fn initialize_session(
        &self,
        request: Request<InitializeSessionRequest>,
    ) -> Result<Response<SessionState>, Status> {
        let req = request.into_inner();

        // Parse pipeline config from bytes
        let pipeline_config: PipelineConfig =
            serde_json::from_slice(&req.pipeline_config).map_err(|e| {
                Status::invalid_argument(format!("Invalid pipeline config: {}", e))
            })?;

        // Parse envelope from bytes
        let envelope: Envelope = serde_json::from_slice(&req.envelope)
            .map_err(|e| Status::invalid_argument(format!("Invalid envelope: {}", e)))?;

        // Initialize orchestration session
        let mut kernel = self.kernel.lock().await;
        let session_state = kernel
            .initialize_orchestration(
                req.process_id.clone(),
                pipeline_config,
                envelope,
                req.force,
            )
            .map_err(|e| Status::internal(format!("Failed to initialize orchestration: {}", e)))?;

        // Convert to proto
        let proto_state: SessionState = session_state.into();

        Ok(Response::new(proto_state))
    }

    /// GetNextInstruction returns the next instruction for a process.
    async fn get_next_instruction(
        &self,
        request: Request<GetNextInstructionRequest>,
    ) -> Result<Response<Instruction>, Status> {
        let req = request.into_inner();

        // Get next instruction from kernel
        let mut kernel = self.kernel.lock().await;
        let instruction = kernel
            .get_next_instruction(&req.process_id)
            .map_err(|e| Status::internal(format!("Failed to get next instruction: {}", e)))?;

        // Convert to proto
        let proto_instruction: Instruction = instruction.into();

        Ok(Response::new(proto_instruction))
    }

    /// ReportAgentResult reports agent execution result and returns next instruction.
    async fn report_agent_result(
        &self,
        request: Request<ReportAgentResultRequest>,
    ) -> Result<Response<Instruction>, Status> {
        let req = request.into_inner();

        // Parse agent output
        let output: serde_json::Value = serde_json::from_slice(&req.output)
            .map_err(|e| Status::invalid_argument(format!("Invalid agent output: {}", e)))?;

        // Convert proto metrics to domain
        let proto_metrics = req
            .metrics
            .ok_or_else(|| Status::invalid_argument("metrics is required"))?;

        let metrics: AgentExecutionMetrics = proto_metrics
            .try_into()
            .map_err(|e| Status::internal(format!("Failed to convert metrics: {}", e)))?;

        // Parse envelope from bytes (updated after agent execution)
        // Note: In the proto, envelope is part of the output, but we need to reconstruct
        // For now, get it from kernel's stored state
        let mut kernel = self.kernel.lock().await;

        let mut envelope = kernel
            .orchestrator
            .get_envelope_for_process(&req.process_id)
            .ok_or_else(|| {
                Status::not_found(format!("Envelope not found: {}", req.process_id))
            })?
            .clone();

        // Update envelope with agent output
        if let serde_json::Value::Object(output_map) = output {
            let mut agent_output = std::collections::HashMap::new();
            for (key, value) in output_map {
                agent_output.insert(key, value);
            }
            envelope.outputs.insert(req.agent_name.clone(), agent_output);
        }

        // Report result
        kernel
            .report_agent_result(&req.process_id, metrics, envelope)
            .map_err(|e| Status::internal(format!("Failed to report agent result: {}", e)))?;

        // Get next instruction
        let instruction = kernel
            .get_next_instruction(&req.process_id)
            .map_err(|e| Status::internal(format!("Failed to get next instruction: {}", e)))?;

        // Convert to proto
        let proto_instruction: Instruction = instruction.into();

        Ok(Response::new(proto_instruction))
    }

    /// GetSessionState returns the current session state.
    async fn get_session_state(
        &self,
        request: Request<GetSessionStateRequest>,
    ) -> Result<Response<SessionState>, Status> {
        let req = request.into_inner();

        // Get session state from kernel
        let kernel = self.kernel.lock().await;
        let session_state = kernel
            .get_orchestration_state(&req.process_id)
            .map_err(|e| Status::internal(format!("Failed to get session state: {}", e)))?;

        // Convert to proto
        let proto_state: SessionState = session_state.into();

        Ok(Response::new(proto_state))
    }
}
