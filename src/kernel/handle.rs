//! Typed channel to the kernel actor. Each command is a `KernelCommand`
//! variant; the caller waits on a oneshot reply.

use crate::run::Run;
use crate::kernel::protocol::{AgentExecutionMetrics, Instruction, RunSnapshot};
use crate::kernel::{RunRecord, SystemStatus};
use crate::workflow::Workflow;
use crate::types::{RunId, RequestId, Result, SessionId, UserId};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

/// Command variants sent to the kernel actor. `pub(crate)` because consumers
/// drive the kernel through `KernelHandle` methods, never by naming commands.
pub(crate) enum KernelCommand {
    /// Initialize a workflow session (auto-creates a run record if needed).
    InitializeSession {
        run_id: RunId,
        workflow: Box<Workflow>,
        run: Box<Run>,
        force: bool,
        resp_tx: oneshot::Sender<Result<RunSnapshot>>,
    },
    /// Get the next instruction for a run.
    GetNextInstruction {
        run_id: RunId,
        resp_tx: oneshot::Sender<Result<Instruction>>,
    },
    /// Report a complete agent result (mutation only, no instruction returned).
    ProcessAgentResult {
        run_id: RunId,
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
        run_id: RunId,
        resp_tx: oneshot::Sender<Result<RunSnapshot>>,
    },
    /// Create a run record (lifecycle).
    CreateRun {
        run_id: RunId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
        resp_tx: oneshot::Sender<Result<RunRecord>>,
    },
    /// Terminate a run.
    TerminateRun {
        run_id: RunId,
        resp_tx: oneshot::Sender<Result<()>>,
    },
    /// Get system status.
    GetSystemStatus {
        resp_tx: oneshot::Sender<SystemStatus>,
    },
    /// Resolve a pending interrupt.
    ResolveInterrupt {
        run_id: RunId,
        interrupt_id: String,
        response: crate::run::InterruptResponse,
        resp_tx: oneshot::Sender<Result<()>>,
    },
    /// Set an interrupt without a lifecycle transition (tool-confirmation gate).
    SetRunInterrupt {
        run_id: RunId,
        interrupt: crate::run::FlowInterrupt,
        resp_tx: oneshot::Sender<Result<()>>,
    },

    /// Single-tool or full-system health snapshot.
    GetToolHealth {
        tool_name: Option<String>,
        resp_tx: oneshot::Sender<Result<serde_json::Value>>,
    },

    RegisterRoutingFn {
        name: String,
        routing_fn: std::sync::Arc<dyn crate::kernel::routing::RoutingFn>,
        resp_tx: oneshot::Sender<()>,
    },
}

impl std::fmt::Debug for KernelCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegisterRoutingFn { name, .. } => {
                f.debug_struct("RegisterRoutingFn").field("name", name).finish()
            }
            other => {
                write!(f, "KernelCommand::{}", match other {
                    Self::InitializeSession { .. } => "InitializeSession",
                    Self::GetNextInstruction { .. } => "GetNextInstruction",
                    Self::ProcessAgentResult { .. } => "ProcessAgentResult",
                    Self::GetSessionState { .. } => "GetSessionState",
                    Self::CreateRun { .. } => "CreateRun",
                    Self::TerminateRun { .. } => "TerminateRun",
                    Self::GetSystemStatus { .. } => "GetSystemStatus",
                    Self::ResolveInterrupt { .. } => "ResolveInterrupt",
                    Self::SetRunInterrupt { .. } => "SetRunInterrupt",
                    Self::GetToolHealth { .. } => "GetToolHealth",
                    Self::RegisterRoutingFn { .. } => unreachable!(),
                })
            }
        }
    }
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
    /// Create a new handle from a channel sender. `pub(crate)` because the
    /// channel half is internal; consumers obtain a `KernelHandle` via
    /// [`kernel::actor::spawn`](crate::kernel::actor::spawn).
    pub(crate) fn new(tx: mpsc::Sender<KernelCommand>) -> Self {
        Self { tx }
    }

    /// Register a named routing function on the kernel's orchestrator.
    pub async fn register_routing_fn(
        &self,
        name: impl Into<String>,
        routing_fn: std::sync::Arc<dyn crate::kernel::routing::RoutingFn>,
    ) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(KernelCommand::RegisterRoutingFn {
                name: name.into(),
                routing_fn,
                resp_tx,
            })
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor unavailable"))?;
        resp_rx
            .await
            .map_err(|_| crate::types::Error::internal("Kernel actor dropped response"))?;
        Ok(())
    }

    /// Initialize a workflow session.
    pub async fn initialize_session(
        &self,
        run_id: RunId,
        workflow: Workflow,
        run: Run,
        force: bool,
    ) -> Result<RunSnapshot> {
        kernel_request!(self, InitializeSession {
            run_id: run_id,
            workflow: Box::new(workflow),
            run: Box::new(run),
            force: force,
        })
    }

    /// Get the next instruction for a run.
    pub async fn get_next_instruction(&self, run_id: &RunId) -> Result<Instruction> {
        kernel_request!(self, GetNextInstruction {
            run_id: run_id.clone(),
        })
    }

    /// Report agent result (mutation only — caller fetches next instruction separately).
    #[allow(clippy::too_many_arguments)]
    pub async fn process_agent_result(
        &self,
        run_id: &RunId,
        agent_name: &str,
        output: serde_json::Value,
        metadata_updates: Option<HashMap<String, serde_json::Value>>,
        metrics: AgentExecutionMetrics,
        success: bool,
        error_message: &str,
        break_loop: bool,
    ) -> Result<()> {
        kernel_request!(self, ProcessAgentResult {
            run_id: run_id.clone(),
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
    pub async fn get_session_state(&self, run_id: &RunId) -> Result<RunSnapshot> {
        kernel_request!(self, GetSessionState {
            run_id: run_id.clone(),
        })
    }

    /// Create a run record.
    pub async fn create_run(
        &self,
        run_id: RunId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
    ) -> Result<RunRecord> {
        kernel_request!(self, CreateRun {
            run_id: run_id,
            request_id: request_id,
            user_id: user_id,
            session_id: session_id,
        })
    }

    /// Terminate a run.
    pub async fn terminate_run(&self, run_id: &RunId) -> Result<()> {
        kernel_request!(self, TerminateRun {
            run_id: run_id.clone(),
        })
    }

    /// Set a pending interrupt on a run without a lifecycle transition.
    ///
    /// Used by the worker workflow loop for tool confirmation gates. Does NOT
    /// change lifecycle state (run stays in its current state).
    pub async fn set_run_interrupt(
        &self,
        run_id: &RunId,
        interrupt: crate::run::FlowInterrupt,
    ) -> Result<()> {
        kernel_request!(self, SetRunInterrupt {
            run_id: run_id.clone(),
            interrupt: interrupt,
        })
    }

    /// Resolve a pending interrupt for a run.
    pub async fn resolve_interrupt(
        &self,
        run_id: &RunId,
        interrupt_id: &str,
        response: crate::run::InterruptResponse,
    ) -> Result<()> {
        kernel_request!(self, ResolveInterrupt {
            run_id: run_id.clone(),
            interrupt_id: interrupt_id.to_string(),
            response: response,
        })
    }

    /// `Some(name)` returns that tool's health report; `None` returns the
    /// full-system report.
    pub async fn get_tool_health(&self, tool_name: Option<&str>) -> Result<serde_json::Value> {
        kernel_request!(self, GetToolHealth {
            tool_name: tool_name.map(|s| s.to_string()),
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
                runs_total: 0,
                runs_by_state: Default::default(),
                active_orchestration_sessions: 0,
            };
        }
        resp_rx.await.unwrap_or(SystemStatus {
            runs_total: 0,
            runs_by_state: Default::default(),
            active_orchestration_sessions: 0,
        })
    }
}
