//! KernelHandle — typed channel wrapper replacing all IPC.
//!
//! Every kernel operation is a variant of `KernelCommand` sent over an mpsc
//! channel. The caller gets a oneshot back with the typed result. No
//! serialization, no TCP, no codec.

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::{
    AgentExecutionMetrics, Instruction, PipelineConfig, SessionState,
};
use crate::kernel::{ProcessControlBlock, SchedulingPriority, SystemStatus};
use crate::types::{ProcessId, RequestId, Result, SessionId, UserId};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

/// Command variants sent to the kernel actor.
#[derive(Debug)]
pub enum KernelCommand {
    /// Initialize a pipeline session (auto-creates PCB if needed).
    InitializeSession {
        process_id: ProcessId,
        pipeline_config: Box<PipelineConfig>,
        envelope: Box<Envelope>,
        force: bool,
        resp_tx: oneshot::Sender<Result<SessionState>>,
    },
    /// Get the next instruction for a process.
    GetNextInstruction {
        process_id: ProcessId,
        resp_tx: oneshot::Sender<Result<Instruction>>,
    },
    /// Report a complete agent result and get next instruction.
    ProcessAgentResult {
        process_id: ProcessId,
        agent_name: String,
        output: serde_json::Value,
        metadata_updates: Option<HashMap<String, serde_json::Value>>,
        metrics: AgentExecutionMetrics,
        success: bool,
        error_message: String,
        break_loop: bool,
        resp_tx: oneshot::Sender<Result<Instruction>>,
    },
    /// Get orchestration session state.
    GetSessionState {
        process_id: ProcessId,
        resp_tx: oneshot::Sender<Result<SessionState>>,
    },
    /// Create a process (lifecycle).
    CreateProcess {
        process_id: ProcessId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
        priority: SchedulingPriority,
        resp_tx: oneshot::Sender<Result<ProcessControlBlock>>,
    },
    /// Terminate a process.
    TerminateProcess {
        process_id: ProcessId,
        resp_tx: oneshot::Sender<Result<()>>,
    },
    /// Get system status.
    GetSystemStatus {
        resp_tx: oneshot::Sender<SystemStatus>,
    },
}

/// Typed handle to the kernel actor. Clone-able, Send + Sync.
#[derive(Clone, Debug)]
pub struct KernelHandle {
    tx: mpsc::Sender<KernelCommand>,
}

impl KernelHandle {
    /// Create a new handle from a channel sender.
    pub fn new(tx: mpsc::Sender<KernelCommand>) -> Self {
        Self { tx }
    }

    /// Initialize a pipeline session.
    pub async fn initialize_session(
        &self,
        process_id: ProcessId,
        pipeline_config: PipelineConfig,
        envelope: Envelope,
        force: bool,
    ) -> Result<SessionState> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KernelCommand::InitializeSession {
                process_id,
                pipeline_config: Box::new(pipeline_config),
                envelope: Box::new(envelope),
                force,
                resp_tx,
            })
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor unavailable"))?;
        resp_rx
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor dropped response"))?
    }

    /// Get the next instruction for a process.
    pub async fn get_next_instruction(&self, process_id: &ProcessId) -> Result<Instruction> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KernelCommand::GetNextInstruction {
                process_id: process_id.clone(),
                resp_tx,
            })
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor unavailable"))?;
        resp_rx
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor dropped response"))?
    }

    /// Report agent result and get next instruction.
    #[allow(clippy::too_many_arguments)]
    pub async fn process_agent_result(
        &self,
        process_id: &ProcessId,
        agent_name: &str,
        output: serde_json::Value,
        metadata_updates: Option<HashMap<String, serde_json::Value>>,
        metrics: AgentExecutionMetrics,
        success: bool,
        error_message: &str,
        break_loop: bool,
    ) -> Result<Instruction> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KernelCommand::ProcessAgentResult {
                process_id: process_id.clone(),
                agent_name: agent_name.to_string(),
                output,
                metadata_updates,
                metrics,
                success,
                error_message: error_message.to_string(),
                break_loop,
                resp_tx,
            })
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor unavailable"))?;
        resp_rx
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor dropped response"))?
    }

    /// Get orchestration session state.
    pub async fn get_session_state(&self, process_id: &ProcessId) -> Result<SessionState> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KernelCommand::GetSessionState {
                process_id: process_id.clone(),
                resp_tx,
            })
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor unavailable"))?;
        resp_rx
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor dropped response"))?
    }

    /// Create a process.
    pub async fn create_process(
        &self,
        process_id: ProcessId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
        priority: SchedulingPriority,
    ) -> Result<ProcessControlBlock> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KernelCommand::CreateProcess {
                process_id,
                request_id,
                user_id,
                session_id,
                priority,
                resp_tx,
            })
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor unavailable"))?;
        resp_rx
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor dropped response"))?
    }

    /// Terminate a process.
    pub async fn terminate_process(&self, process_id: &ProcessId) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KernelCommand::TerminateProcess {
                process_id: process_id.clone(),
                resp_tx,
            })
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor unavailable"))?;
        resp_rx
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor dropped response"))?
    }

    /// Get system status.
    pub async fn get_system_status(&self) -> SystemStatus {
        let (resp_tx, resp_rx) = oneshot::channel();
        if self
            .tx
            .send(KernelCommand::GetSystemStatus { resp_tx })
            .await
            .is_err()
        {
            return SystemStatus {
                processes_total: 0,
                processes_by_state: Default::default(),
                services_healthy: 0,
                services_degraded: 0,
                services_unhealthy: 0,
                active_orchestration_sessions: 0,
            };
        }
        resp_rx.await.unwrap_or(SystemStatus {
            processes_total: 0,
            processes_by_state: Default::default(),
            services_healthy: 0,
            services_degraded: 0,
            services_unhealthy: 0,
            active_orchestration_sessions: 0,
        })
    }
}
