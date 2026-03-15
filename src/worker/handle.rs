//! KernelHandle — typed channel wrapper replacing all IPC.
//!
//! Every kernel operation is a variant of `KernelCommand` sent over an mpsc
//! channel. The caller gets a oneshot back with the typed result. No
//! serialization, no TCP, no codec.

use crate::commbus::{Event, Query, QueryResponse, Subscription};
use crate::envelope::Envelope;
use crate::kernel::agent_card::AgentCard;
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
    /// Report a complete agent result (mutation only, no instruction returned).
    ProcessAgentResult {
        process_id: ProcessId,
        agent_name: String,
        output: serde_json::Value,
        metadata_updates: Option<HashMap<String, serde_json::Value>>,
        metrics: AgentExecutionMetrics,
        success: bool,
        error_message: String,
        break_loop: bool,
        resp_tx: oneshot::Sender<Result<()>>,
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
    /// Resolve a pending interrupt.
    ResolveInterrupt {
        process_id: ProcessId,
        interrupt_id: String,
        response: crate::envelope::InterruptResponse,
        resp_tx: oneshot::Sender<Result<()>>,
    },

    // =========================================================================
    // CommBus Federation
    // =========================================================================

    /// Publish an event to CommBus subscribers.
    PublishEvent {
        event: Event,
        resp_tx: oneshot::Sender<Result<usize>>,
    },
    /// Subscribe to CommBus event types.
    Subscribe {
        subscriber_id: String,
        event_types: Vec<String>,
        resp_tx: oneshot::Sender<Result<(Subscription, mpsc::Receiver<Event>)>>,
    },
    /// Unsubscribe from CommBus.
    Unsubscribe {
        subscription: Subscription,
        resp_tx: oneshot::Sender<()>,
    },
    /// Execute a CommBus query (request/response with timeout).
    CommBusQuery {
        query: Query,
        resp_tx: oneshot::Sender<Result<QueryResponse>>,
    },
    /// List registered agent cards (discovery).
    ListAgentCards {
        filter: Option<String>,
        resp_tx: oneshot::Sender<Vec<AgentCard>>,
    },

    // =========================================================================
    // Checkpoint/Resume
    // =========================================================================

    /// Capture a checkpoint of a running process.
    Checkpoint {
        process_id: ProcessId,
        resp_tx: oneshot::Sender<Result<crate::kernel::checkpoint::CheckpointSnapshot>>,
    },
    /// Resume a process from a checkpoint snapshot.
    ResumeFromCheckpoint {
        snapshot: Box<crate::kernel::checkpoint::CheckpointSnapshot>,
        pipeline_config: Box<PipelineConfig>,
        resp_tx: oneshot::Sender<Result<ProcessId>>,
    },
}

/// Typed handle to the kernel actor. Clone-able, Send + Sync.
#[derive(Clone, Debug)]
pub struct KernelHandle {
    tx: mpsc::Sender<KernelCommand>,
}

/// Send a KernelCommand and await the oneshot response.
/// Covers methods with the standard request-response pattern.
macro_rules! kernel_request {
    ($self:ident, $variant:ident { $($field:ident : $val:expr),* $(,)? }) => {{
        let (resp_tx, resp_rx) = oneshot::channel();
        $self.tx
            .send(KernelCommand::$variant { $($field: $val,)* resp_tx })
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor unavailable"))?;
        resp_rx
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor dropped response"))?
    }};
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
        kernel_request!(self, InitializeSession {
            process_id: process_id,
            pipeline_config: Box::new(pipeline_config),
            envelope: Box::new(envelope),
            force: force,
        })
    }

    /// Get the next instruction for a process.
    pub async fn get_next_instruction(&self, process_id: &ProcessId) -> Result<Instruction> {
        kernel_request!(self, GetNextInstruction {
            process_id: process_id.clone(),
        })
    }

    /// Report agent result (mutation only — caller fetches next instruction separately).
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
    ) -> Result<()> {
        kernel_request!(self, ProcessAgentResult {
            process_id: process_id.clone(),
            agent_name: agent_name.to_string(),
            output: output,
            metadata_updates: metadata_updates,
            metrics: metrics,
            success: success,
            error_message: error_message.to_string(),
            break_loop: break_loop,
        })
    }

    /// Get orchestration session state.
    pub async fn get_session_state(&self, process_id: &ProcessId) -> Result<SessionState> {
        kernel_request!(self, GetSessionState {
            process_id: process_id.clone(),
        })
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
        kernel_request!(self, CreateProcess {
            process_id: process_id,
            request_id: request_id,
            user_id: user_id,
            session_id: session_id,
            priority: priority,
        })
    }

    /// Terminate a process.
    pub async fn terminate_process(&self, process_id: &ProcessId) -> Result<()> {
        kernel_request!(self, TerminateProcess {
            process_id: process_id.clone(),
        })
    }

    /// Resolve a pending interrupt for a process.
    pub async fn resolve_interrupt(
        &self,
        process_id: &ProcessId,
        interrupt_id: &str,
        response: crate::envelope::InterruptResponse,
    ) -> Result<()> {
        kernel_request!(self, ResolveInterrupt {
            process_id: process_id.clone(),
            interrupt_id: interrupt_id.to_string(),
            response: response,
        })
    }

    // =========================================================================
    // CommBus Federation
    // =========================================================================

    /// Publish an event to CommBus subscribers.
    pub async fn publish_event(&self, event: Event) -> Result<usize> {
        kernel_request!(self, PublishEvent { event: event })
    }

    /// Subscribe to CommBus event types.
    pub async fn subscribe(
        &self,
        subscriber_id: String,
        event_types: Vec<String>,
    ) -> Result<(Subscription, mpsc::Receiver<Event>)> {
        kernel_request!(self, Subscribe {
            subscriber_id: subscriber_id,
            event_types: event_types,
        })
    }

    /// Unsubscribe from CommBus.
    pub async fn unsubscribe(&self, subscription: Subscription) {
        let (resp_tx, resp_rx) = oneshot::channel();
        if self
            .tx
            .send(KernelCommand::Unsubscribe {
                subscription,
                resp_tx,
            })
            .await
            .is_ok()
        {
            let _ = resp_rx.await;
        }
    }

    /// Execute a CommBus query.
    pub async fn commbus_query(&self, query: Query) -> Result<QueryResponse> {
        kernel_request!(self, CommBusQuery { query: query })
    }

    /// List registered agent cards (discovery).
    pub async fn list_agent_cards(&self, filter: Option<String>) -> Vec<AgentCard> {
        let (resp_tx, resp_rx) = oneshot::channel();
        if self
            .tx
            .send(KernelCommand::ListAgentCards { filter, resp_tx })
            .await
            .is_err()
        {
            return vec![];
        }
        resp_rx.await.unwrap_or_default()
    }

    // =========================================================================
    // Checkpoint/Resume
    // =========================================================================

    /// Capture a checkpoint of a running process.
    pub async fn checkpoint(
        &self,
        process_id: ProcessId,
    ) -> Result<crate::kernel::checkpoint::CheckpointSnapshot> {
        kernel_request!(self, Checkpoint {
            process_id: process_id,
        })
    }

    /// Resume a process from a checkpoint snapshot.
    pub async fn resume_from_checkpoint(
        &self,
        snapshot: crate::kernel::checkpoint::CheckpointSnapshot,
        pipeline_config: PipelineConfig,
    ) -> Result<ProcessId> {
        kernel_request!(self, ResumeFromCheckpoint {
            snapshot: Box::new(snapshot),
            pipeline_config: Box::new(pipeline_config),
        })
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
