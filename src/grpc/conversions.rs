//! Proto ↔ Domain conversions.
//!
//! Converts between protobuf types (crate::proto::*) and domain types (crate::kernel::*, crate::envelope::*).
//! Implements TryFrom for fallible conversions (proto -> domain) and From for infallible conversions (domain -> proto).

use crate::envelope::{
    Envelope, FlowInterrupt, InterruptKind, InterruptResponse, TerminalReason,
};
use crate::kernel::{
    ProcessControlBlock, ProcessState, ResourceQuota, ResourceUsage, SchedulingPriority,
};
use crate::proto::{self, InterruptStatus};
use crate::types::{Error, ProcessId, RequestId, SessionId, UserId};
use chrono::{DateTime, TimeZone, Utc};
use std::collections::HashMap;

// =============================================================================
// Enum conversion macro — generates TryFrom<i32> and From<X> for i32
// via the proto enum as intermediary (handles UNSPECIFIED = 0 as error).
// =============================================================================

macro_rules! proto_enum_conv {
    ($domain:ty, $proto:ty, $label:expr, [$( $variant:ident ),+ $(,)?]) => {
        impl TryFrom<i32> for $domain {
            type Error = Error;

            fn try_from(value: i32) -> Result<Self, Self::Error> {
                match <$proto>::try_from(value) {
                    $( Ok(<$proto>::$variant) => Ok(<$domain>::$variant), )+
                    _ => Err(Error::validation(format!(concat!("Invalid ", $label, ": {}"), value))),
                }
            }
        }

        impl From<$domain> for i32 {
            fn from(val: $domain) -> i32 {
                match val {
                    $( <$domain>::$variant => <$proto>::$variant as i32, )+
                }
            }
        }
    };
}

// =============================================================================
// Enum conversions (all 5 enums)
// =============================================================================

proto_enum_conv!(ProcessState, proto::ProcessState, "ProcessState", [
    New, Ready, Running, Waiting, Blocked, Terminated, Zombie,
]);

proto_enum_conv!(SchedulingPriority, proto::SchedulingPriority, "SchedulingPriority", [
    Realtime, High, Normal, Low, Idle,
]);

proto_enum_conv!(InterruptKind, proto::InterruptKind, "InterruptKind", [
    Clarification, Confirmation, Checkpoint, ResourceExhausted, Timeout, SystemError, AgentReview,
]);

proto_enum_conv!(TerminalReason, proto::TerminalReason, "TerminalReason", [
    Completed, MaxIterationsExceeded, MaxLlmCallsExceeded, MaxAgentHopsExceeded,
    UserCancelled, ToolFailedFatally, LlmFailedFatally, PolicyViolation,
]);

// =============================================================================
// ResourceQuota conversions
// =============================================================================

impl TryFrom<proto::ResourceQuota> for ResourceQuota {
    type Error = Error;

    fn try_from(proto: proto::ResourceQuota) -> Result<Self, Self::Error> {
        Ok(ResourceQuota {
            max_input_tokens: proto.max_input_tokens,
            max_output_tokens: proto.max_output_tokens,
            max_context_tokens: proto.max_context_tokens,
            max_llm_calls: proto.max_llm_calls,
            max_tool_calls: proto.max_tool_calls,
            max_agent_hops: proto.max_agent_hops,
            max_iterations: proto.max_iterations,
            timeout_seconds: proto.timeout_seconds,
            soft_timeout_seconds: proto.soft_timeout_seconds,
            rate_limit_rpm: proto.rate_limit_rpm,
            rate_limit_rph: proto.rate_limit_rph,
            rate_limit_burst: proto.rate_limit_burst,
            max_inference_requests: proto.max_inference_requests,
            max_inference_input_chars: proto.max_inference_input_chars,
        })
    }
}

impl From<ResourceQuota> for proto::ResourceQuota {
    fn from(quota: ResourceQuota) -> proto::ResourceQuota {
        proto::ResourceQuota {
            max_input_tokens: quota.max_input_tokens,
            max_output_tokens: quota.max_output_tokens,
            max_context_tokens: quota.max_context_tokens,
            max_llm_calls: quota.max_llm_calls,
            max_tool_calls: quota.max_tool_calls,
            max_agent_hops: quota.max_agent_hops,
            max_iterations: quota.max_iterations,
            timeout_seconds: quota.timeout_seconds,
            soft_timeout_seconds: quota.soft_timeout_seconds,
            rate_limit_rpm: quota.rate_limit_rpm,
            rate_limit_rph: quota.rate_limit_rph,
            rate_limit_burst: quota.rate_limit_burst,
            max_inference_requests: quota.max_inference_requests,
            max_inference_input_chars: quota.max_inference_input_chars,
        }
    }
}

// =============================================================================
// ResourceUsage conversions
// =============================================================================

impl From<ResourceUsage> for proto::ResourceUsage {
    fn from(usage: ResourceUsage) -> proto::ResourceUsage {
        proto::ResourceUsage {
            llm_calls: usage.llm_calls,
            tool_calls: usage.tool_calls,
            agent_hops: usage.agent_hops,
            iterations: usage.iterations,
            tokens_in: usage.tokens_in as i32,
            tokens_out: usage.tokens_out as i32,
            elapsed_seconds: usage.elapsed_seconds,
            inference_requests: usage.inference_requests,
            inference_input_chars: usage.inference_input_chars as i32,
        }
    }
}

impl TryFrom<proto::ResourceUsage> for ResourceUsage {
    type Error = Error;

    fn try_from(proto: proto::ResourceUsage) -> Result<Self, Self::Error> {
        Ok(ResourceUsage {
            llm_calls: proto.llm_calls,
            tool_calls: proto.tool_calls,
            agent_hops: proto.agent_hops,
            iterations: proto.iterations,
            tokens_in: proto.tokens_in as i64,
            tokens_out: proto.tokens_out as i64,
            elapsed_seconds: proto.elapsed_seconds,
            inference_requests: proto.inference_requests,
            inference_input_chars: proto.inference_input_chars as i64,
        })
    }
}

// =============================================================================
// ProcessControlBlock conversions
// =============================================================================

impl TryFrom<proto::ProcessControlBlock> for ProcessControlBlock {
    type Error = Error;

    fn try_from(proto: proto::ProcessControlBlock) -> Result<Self, Self::Error> {
        let state = ProcessState::try_from(proto.state)?;
        let priority = SchedulingPriority::try_from(proto.priority)?;

        let quota = proto
            .quota
            .ok_or_else(|| Error::validation("Missing quota in ProcessControlBlock"))?;
        let quota = ResourceQuota::try_from(quota)?;

        let usage = proto
            .usage
            .ok_or_else(|| Error::validation("Missing usage in ProcessControlBlock"))?;
        let usage = ResourceUsage::try_from(usage)?;

        let created_at = ms_to_datetime(proto.created_at_ms)?;
        let started_at = if proto.started_at_ms > 0 {
            Some(ms_to_datetime(proto.started_at_ms)?)
        } else {
            None
        };
        let completed_at = if proto.completed_at_ms > 0 {
            Some(ms_to_datetime(proto.completed_at_ms)?)
        } else {
            None
        };
        let last_scheduled_at = if proto.last_scheduled_at_ms > 0 {
            Some(ms_to_datetime(proto.last_scheduled_at_ms)?)
        } else {
            None
        };

        let pending_interrupt = if proto.pending_interrupt != 0 {
            Some(InterruptKind::try_from(proto.pending_interrupt)?)
        } else {
            None
        };

        let interrupt_data = if !proto.interrupt_data.is_empty() {
            Some(serde_json::from_slice(&proto.interrupt_data).map_err(|e| {
                Error::internal(format!("Failed to deserialize interrupt_data: {}", e))
            })?)
        } else {
            None
        };

        let pid = ProcessId::from_string(proto.pid)
            .map_err(|e| Error::validation(e.to_string()))?;
        let request_id = RequestId::from_string(proto.request_id)
            .map_err(|e| Error::validation(e.to_string()))?;
        let user_id = UserId::from_string(proto.user_id)
            .map_err(|e| Error::validation(e.to_string()))?;
        let session_id = SessionId::from_string(proto.session_id)
            .map_err(|e| Error::validation(e.to_string()))?;

        let parent_pid = if proto.parent_pid.is_empty() {
            None
        } else {
            Some(ProcessId::from_string(proto.parent_pid)
                .map_err(|e| Error::validation(e.to_string()))?)
        };

        let child_pids: Vec<ProcessId> = proto.child_pids
            .into_iter()
            .map(|s| ProcessId::from_string(s).map_err(|e| Error::validation(e.to_string())))
            .collect::<std::result::Result<_, _>>()?;

        Ok(ProcessControlBlock {
            pid,
            request_id,
            user_id,
            session_id,
            state,
            priority,
            quota,
            usage,
            created_at,
            started_at,
            completed_at,
            last_scheduled_at,
            current_stage: if proto.current_stage.is_empty() {
                None
            } else {
                Some(proto.current_stage)
            },
            current_service: if proto.current_service.is_empty() {
                None
            } else {
                Some(proto.current_service)
            },
            pending_interrupt,
            interrupt_data,
            parent_pid,
            child_pids,
        })
    }
}

impl From<ProcessControlBlock> for proto::ProcessControlBlock {
    fn from(pcb: ProcessControlBlock) -> proto::ProcessControlBlock {
        proto::ProcessControlBlock {
            pid: pcb.pid.to_string(),
            request_id: pcb.request_id.to_string(),
            user_id: pcb.user_id.to_string(),
            session_id: pcb.session_id.to_string(),
            state: i32::from(pcb.state),
            priority: i32::from(pcb.priority),
            quota: Some(proto::ResourceQuota::from(pcb.quota)),
            usage: Some(proto::ResourceUsage::from(pcb.usage)),
            created_at_ms: datetime_to_ms(&pcb.created_at),
            started_at_ms: pcb.started_at.map(|t| datetime_to_ms(&t)).unwrap_or(0),
            completed_at_ms: pcb.completed_at.map(|t| datetime_to_ms(&t)).unwrap_or(0),
            last_scheduled_at_ms: pcb
                .last_scheduled_at
                .map(|t| datetime_to_ms(&t))
                .unwrap_or(0),
            current_stage: pcb.current_stage.unwrap_or_default(),
            current_service: pcb.current_service.unwrap_or_default(),
            pending_interrupt: pcb
                .pending_interrupt
                .map(|k| i32::from(k))
                .unwrap_or(0),
            interrupt_data: pcb
                .interrupt_data
                .map(|d| serde_json::to_vec(&d).unwrap_or_default())
                .unwrap_or_default(),
            parent_pid: pcb.parent_pid.map(|p| p.to_string()).unwrap_or_default(),
            child_pids: pcb.child_pids.into_iter().map(|p| p.to_string()).collect(),
        }
    }
}

// =============================================================================
// Envelope conversions
// =============================================================================

impl TryFrom<proto::Envelope> for Envelope {
    type Error = Error;

    fn try_from(proto: proto::Envelope) -> Result<Self, Self::Error> {
        let received_at = ms_to_datetime(proto.received_at_ms)?;
        let created_at = ms_to_datetime(proto.created_at_ms)?;
        let completed_at = if proto.completed_at_ms > 0 {
            Some(ms_to_datetime(proto.completed_at_ms)?)
        } else {
            None
        };

        // Deserialize outputs from proto bytes
        let mut outputs = HashMap::new();
        for (key, value_bytes) in proto.outputs {
            let value: HashMap<String, serde_json::Value> =
                serde_json::from_slice(&value_bytes).map_err(|e| {
                    Error::internal(format!("Failed to deserialize output '{}': {}", key, e))
                })?;
            outputs.insert(key, value);
        }

        let terminal_reason = if proto.terminal_reason != 0 {
            Some(TerminalReason::try_from(proto.terminal_reason)?)
        } else {
            None
        };

        let interrupt = proto
            .interrupt
            .map(|i| FlowInterrupt::try_from(i))
            .transpose()?;

        Ok(Envelope {
            envelope_id: proto.envelope_id,
            request_id: proto.request_id,
            user_id: proto.user_id,
            session_id: proto.session_id,
            raw_input: proto.raw_input,
            received_at,
            outputs,
            current_stage: proto.current_stage,
            stage_order: proto.stage_order,
            iteration: proto.iteration,
            max_iterations: proto.max_iterations,
            active_stages: proto.active_stages.into_keys().collect(),
            completed_stage_set: proto.completed_stage_set.into_keys().collect(),
            failed_stages: Some(proto.failed_stages),
            parallel_mode: Some(false), // Not in proto
            llm_call_count: proto.llm_call_count,
            max_llm_calls: proto.max_llm_calls,
            tool_call_count: 0, // Not in proto yet
            agent_hop_count: proto.agent_hop_count,
            max_agent_hops: proto.max_agent_hops,
            tokens_in: 0, // Not in proto yet
            tokens_out: 0, // Not in proto yet
            terminal_reason,
            terminated: proto.terminated,
            termination_reason: if proto.termination_reason.is_empty() {
                None
            } else {
                Some(proto.termination_reason)
            },
            interrupt_pending: proto.interrupt_pending,
            interrupt,
            completed_stages: Vec::new(), // Not in proto
            current_stage_number: proto.current_stage_number,
            max_stages: proto.max_stages,
            all_goals: proto.all_goals,
            remaining_goals: proto.remaining_goals,
            goal_completion_status: proto.goal_completion_status,
            prior_plans: Vec::new(), // Not in proto
            loop_feedback: Vec::new(), // Not in proto
            processing_history: Vec::new(), // Not in proto
            errors: Vec::new(),       // Not in proto
            created_at,
            completed_at,
            metadata: HashMap::new(), // Not in proto
        })
    }
}

impl From<Envelope> for proto::Envelope {
    fn from(env: Envelope) -> proto::Envelope {
        // Serialize outputs to bytes
        let mut outputs = HashMap::new();
        for (key, value) in env.outputs {
            if let Ok(value_bytes) = serde_json::to_vec(&value) {
                outputs.insert(key, value_bytes);
            }
        }

        proto::Envelope {
            envelope_id: env.envelope_id,
            request_id: env.request_id,
            user_id: env.user_id,
            session_id: env.session_id,
            raw_input: env.raw_input,
            received_at_ms: datetime_to_ms(&env.received_at),
            current_stage: env.current_stage,
            stage_order: env.stage_order,
            iteration: env.iteration,
            max_iterations: env.max_iterations,
            llm_call_count: env.llm_call_count,
            max_llm_calls: env.max_llm_calls,
            agent_hop_count: env.agent_hop_count,
            max_agent_hops: env.max_agent_hops,
            terminated: env.terminated,
            termination_reason: env.termination_reason.unwrap_or_default(),
            terminal_reason: env.terminal_reason.map(|r| i32::from(r)).unwrap_or(0),
            completed_at_ms: env.completed_at.map(|t| datetime_to_ms(&t)).unwrap_or(0),
            interrupt_pending: env.interrupt_pending,
            interrupt: env.interrupt.map(|i| proto::FlowInterrupt::from(i)),
            outputs,
            active_stages: env.active_stages.into_iter().map(|s| (s, true)).collect(),
            completed_stage_set: env.completed_stage_set.into_iter().map(|s| (s, true)).collect(),
            failed_stages: env.failed_stages.unwrap_or_default(),
            current_stage_number: env.current_stage_number,
            max_stages: env.max_stages,
            all_goals: env.all_goals,
            remaining_goals: env.remaining_goals,
            goal_completion_status: env.goal_completion_status,
            metadata_str: HashMap::new(), // Not in domain
            created_at_ms: datetime_to_ms(&env.created_at),
        }
    }
}

// =============================================================================
// FlowInterrupt conversions
// =============================================================================

impl TryFrom<proto::FlowInterrupt> for FlowInterrupt {
    type Error = Error;

    fn try_from(proto: proto::FlowInterrupt) -> Result<Self, Self::Error> {
        let kind = InterruptKind::try_from(proto.kind)?;
        let created_at = ms_to_datetime(proto.created_at_ms)?;
        let expires_at = if proto.expires_at_ms > 0 {
            Some(ms_to_datetime(proto.expires_at_ms)?)
        } else {
            None
        };

        let data = if !proto.data.is_empty() {
            Some(serde_json::from_slice(&proto.data).map_err(|e| {
                Error::internal(format!("Failed to deserialize interrupt data: {}", e))
            })?)
        } else {
            None
        };

        let response = proto
            .response
            .map(|r| InterruptResponse::try_from(r))
            .transpose()?;

        Ok(FlowInterrupt {
            kind,
            id: proto.id,
            question: if proto.question.is_empty() {
                None
            } else {
                Some(proto.question)
            },
            message: if proto.message.is_empty() {
                None
            } else {
                Some(proto.message)
            },
            data,
            response,
            created_at,
            expires_at,
        })
    }
}

impl From<FlowInterrupt> for proto::FlowInterrupt {
    fn from(interrupt: FlowInterrupt) -> proto::FlowInterrupt {
        proto::FlowInterrupt {
            id: interrupt.id,
            kind: i32::from(interrupt.kind),
            request_id: String::new(), // Not in domain
            user_id: String::new(),    // Not in domain
            session_id: String::new(), // Not in domain
            envelope_id: String::new(), // Not in domain
            question: interrupt.question.unwrap_or_default(),
            message: interrupt.message.unwrap_or_default(),
            data: interrupt
                .data
                .map(|d| serde_json::to_vec(&d).unwrap_or_default())
                .unwrap_or_default(),
            response: interrupt
                .response
                .map(|r| proto::InterruptResponse::from(r)),
            status: InterruptStatus::Unspecified as i32, // Not in domain
            created_at_ms: datetime_to_ms(&interrupt.created_at),
            expires_at_ms: interrupt
                .expires_at
                .map(|t| datetime_to_ms(&t))
                .unwrap_or(0),
            resolved_at_ms: 0, // Not in domain
            trace_id: String::new(), // Not in domain
            span_id: String::new(), // Not in domain
        }
    }
}

// =============================================================================
// InterruptResponse conversions
// =============================================================================

impl TryFrom<proto::InterruptResponse> for InterruptResponse {
    type Error = Error;

    fn try_from(proto: proto::InterruptResponse) -> Result<Self, Self::Error> {
        let received_at = ms_to_datetime(proto.resolved_at_ms)?;

        let data = if !proto.data.is_empty() {
            Some(serde_json::from_slice(&proto.data).map_err(|e| {
                Error::internal(format!("Failed to deserialize response data: {}", e))
            })?)
        } else {
            None
        };

        Ok(InterruptResponse {
            text: if proto.text.is_empty() {
                None
            } else {
                Some(proto.text)
            },
            approved: if proto.approved { Some(true) } else { None },
            decision: if proto.decision.is_empty() {
                None
            } else {
                Some(proto.decision)
            },
            data,
            received_at,
        })
    }
}

impl From<InterruptResponse> for proto::InterruptResponse {
    fn from(response: InterruptResponse) -> proto::InterruptResponse {
        proto::InterruptResponse {
            text: response.text.unwrap_or_default(),
            approved: response.approved.unwrap_or(false),
            decision: response.decision.unwrap_or_default(),
            data: response
                .data
                .map(|d| serde_json::to_vec(&d).unwrap_or_default())
                .unwrap_or_default(),
            resolved_at_ms: datetime_to_ms(&response.received_at),
        }
    }
}

// =============================================================================
// Helper functions
// =============================================================================

fn ms_to_datetime(ms: i64) -> Result<DateTime<Utc>, Error> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .ok_or_else(|| Error::validation(format!("Invalid timestamp: {}", ms)))
}

fn datetime_to_ms(dt: &DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}
// =============================================================================
// Orchestrator conversions (for OrchestrationService)
// =============================================================================

use crate::kernel::orchestrator::{
    AgentExecutionMetrics, Instruction, InstructionKind, SessionState,
};

proto_enum_conv!(InstructionKind, proto::InstructionKind, "InstructionKind", [
    RunAgent, Terminate, WaitInterrupt,
]);

// Instruction conversions
impl From<Instruction> for proto::Instruction {
    fn from(instruction: Instruction) -> proto::Instruction {
        proto::Instruction {
            kind: i32::from(instruction.kind),
            agent_name: instruction.agent_name.unwrap_or_default(),
            agent_config: instruction
                .agent_config
                .and_then(|v| serde_json::to_vec(&v).ok())
                .unwrap_or_default(),
            envelope: instruction
                .envelope
                .and_then(|e| serde_json::to_vec(&e).ok())
                .unwrap_or_default(),
            terminal_reason: instruction
                .terminal_reason
                .map(|r| i32::from(r))
                .unwrap_or(0),
            termination_message: instruction.termination_message.unwrap_or_default(),
            interrupt_pending: instruction.interrupt_pending,
            interrupt: instruction.interrupt.map(|i| i.into()),
        }
    }
}

// AgentExecutionMetrics conversions
impl TryFrom<proto::AgentExecutionMetrics> for AgentExecutionMetrics {
    type Error = Error;

    fn try_from(proto: proto::AgentExecutionMetrics) -> Result<Self, Self::Error> {
        Ok(AgentExecutionMetrics {
            llm_calls: proto.llm_calls,
            tool_calls: proto.tool_calls,
            tokens_in: proto.tokens_in as i64,
            tokens_out: proto.tokens_out as i64,
            duration_ms: proto.duration_ms as i64,
        })
    }
}

impl From<AgentExecutionMetrics> for proto::AgentExecutionMetrics {
    fn from(metrics: AgentExecutionMetrics) -> proto::AgentExecutionMetrics {
        proto::AgentExecutionMetrics {
            llm_calls: metrics.llm_calls,
            tool_calls: metrics.tool_calls,
            tokens_in: metrics.tokens_in as i32,
            tokens_out: metrics.tokens_out as i32,
            duration_ms: metrics.duration_ms as i32,
        }
    }
}

// SessionState conversions
impl From<SessionState> for proto::SessionState {
    fn from(state: SessionState) -> proto::SessionState {
        proto::SessionState {
            process_id: state.process_id,
            current_stage: state.current_stage,
            stage_order: state.stage_order,
            envelope: serde_json::to_vec(&state.envelope)
                .ok()
                .unwrap_or_default(),
            edge_traversals: state.edge_traversals,
            terminated: state.terminated,
            terminal_reason: state.terminal_reason.map(|r| i32::from(r)).unwrap_or(0),
        }
    }
}
