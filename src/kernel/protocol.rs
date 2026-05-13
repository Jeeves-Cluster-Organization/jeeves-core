//! Kernel ↔ runner contract types. Not part of the consumer-facing API.

use serde::{Deserialize, Serialize};

use crate::agent::policy::ContextOverflow;
use crate::run::{FlowInterrupt, TerminalReason};
use crate::types::{RunId, StageName};
use crate::workflow::RetryPolicy;

use super::routing::RoutingDecision;

/// Per-dispatch context layered on after the orchestrator runs. Populated by
/// `kernel::dispatch::get_next_instruction`.
///
/// `pub` because it's reachable through `Instruction::{RunAgent, Terminate}`,
/// but consumers see it flattened on the wire — they don't name this type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentDispatchContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_context: Option<serde_json::Value>,
    /// Verbatim LLM-provider hint; kernel never parses it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_overflow: Option<ContextOverflow>,
    /// Set when resuming after a confirmation interrupt; consumed from
    /// `audit.metadata` so it never leaks to subsequent dispatches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt_response: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    /// Routing decision that selected this stage; emitted as an audit event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_routing_decision: Option<RoutingDecision>,
}

/// Kernel → worker command emitted by `KernelHandle::get_next_instruction`.
#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum Instruction {
    RunAgent {
        agent: String,
        #[serde(flatten)]
        context: AgentDispatchContext,
    },
    Terminate {
        reason: TerminalReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(flatten)]
        context: AgentDispatchContext,
    },
    /// Suspend until the consumer resolves a pending tool-confirmation gate.
    WaitInterrupt {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interrupt: Option<FlowInterrupt>,
    },
}

impl Instruction {
    pub fn terminate(reason: TerminalReason, message: impl Into<String>) -> Self {
        Self::Terminate {
            reason,
            message: Some(message.into()),
            context: Default::default(),
        }
    }

    pub fn terminate_completed() -> Self {
        Self::Terminate {
            reason: TerminalReason::Completed,
            message: None,
            context: Default::default(),
        }
    }

    pub fn run_agent(name: impl Into<String>) -> Self {
        Self::RunAgent {
            agent: name.into(),
            context: Default::default(),
        }
    }

    pub fn wait_interrupt(interrupt: FlowInterrupt) -> Self {
        Self::WaitInterrupt {
            interrupt: Some(interrupt),
        }
    }
}

/// External snapshot of an orchestration session — returned by
/// `KernelHandle::get_session_state()`.
#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSnapshot {
    pub run_id: RunId,
    /// Name of the stage currently executing or about to execute.
    pub current_stage: StageName,
    /// Ordered list of all stage names in the workflow.
    pub stage_order: Vec<StageName>,
    /// Run serialized as JSON (outputs, bounds, audit trail, interrupts).
    pub run: serde_json::Value,
    pub terminated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<TerminalReason>,
}
