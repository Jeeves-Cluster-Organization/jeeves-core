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
use crate::types::Error;
use chrono::{DateTime, TimeZone, Utc};
use std::collections::HashMap;

// =============================================================================
// ProcessState conversions
// =============================================================================

impl TryFrom<i32> for ProcessState {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match proto::ProcessState::try_from(value) {
            Ok(proto::ProcessState::New) => Ok(ProcessState::New),
            Ok(proto::ProcessState::Ready) => Ok(ProcessState::Ready),
            Ok(proto::ProcessState::Running) => Ok(ProcessState::Running),
            Ok(proto::ProcessState::Waiting) => Ok(ProcessState::Waiting),
            Ok(proto::ProcessState::Blocked) => Ok(ProcessState::Blocked),
            Ok(proto::ProcessState::Terminated) => Ok(ProcessState::Terminated),
            Ok(proto::ProcessState::Zombie) => Ok(ProcessState::Zombie),
            _ => Err(Error::validation(format!("Invalid ProcessState: {}", value))),
        }
    }
}

impl From<ProcessState> for i32 {
    fn from(state: ProcessState) -> i32 {
        match state {
            ProcessState::New => proto::ProcessState::New as i32,
            ProcessState::Ready => proto::ProcessState::Ready as i32,
            ProcessState::Running => proto::ProcessState::Running as i32,
            ProcessState::Waiting => proto::ProcessState::Waiting as i32,
            ProcessState::Blocked => proto::ProcessState::Blocked as i32,
            ProcessState::Terminated => proto::ProcessState::Terminated as i32,
            ProcessState::Zombie => proto::ProcessState::Zombie as i32,
        }
    }
}

// =============================================================================
// SchedulingPriority conversions
// =============================================================================

impl TryFrom<i32> for SchedulingPriority {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match proto::SchedulingPriority::try_from(value) {
            Ok(proto::SchedulingPriority::Realtime) => Ok(SchedulingPriority::Realtime),
            Ok(proto::SchedulingPriority::High) => Ok(SchedulingPriority::High),
            Ok(proto::SchedulingPriority::Normal) => Ok(SchedulingPriority::Normal),
            Ok(proto::SchedulingPriority::Low) => Ok(SchedulingPriority::Low),
            Ok(proto::SchedulingPriority::Idle) => Ok(SchedulingPriority::Idle),
            _ => Err(Error::validation(format!(
                "Invalid SchedulingPriority: {}",
                value
            ))),
        }
    }
}

impl From<SchedulingPriority> for i32 {
    fn from(priority: SchedulingPriority) -> i32 {
        match priority {
            SchedulingPriority::Realtime => proto::SchedulingPriority::Realtime as i32,
            SchedulingPriority::High => proto::SchedulingPriority::High as i32,
            SchedulingPriority::Normal => proto::SchedulingPriority::Normal as i32,
            SchedulingPriority::Low => proto::SchedulingPriority::Low as i32,
            SchedulingPriority::Idle => proto::SchedulingPriority::Idle as i32,
        }
    }
}

// =============================================================================
// InterruptKind conversions
// =============================================================================

impl TryFrom<i32> for InterruptKind {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match proto::InterruptKind::try_from(value) {
            Ok(proto::InterruptKind::Clarification) => Ok(InterruptKind::Clarification),
            Ok(proto::InterruptKind::Confirmation) => Ok(InterruptKind::Confirmation),
            Ok(proto::InterruptKind::Checkpoint) => Ok(InterruptKind::Checkpoint),
            Ok(proto::InterruptKind::ResourceExhausted) => Ok(InterruptKind::ResourceExhausted),
            Ok(proto::InterruptKind::Timeout) => Ok(InterruptKind::Timeout),
            Ok(proto::InterruptKind::SystemError) => Ok(InterruptKind::SystemError),
            Ok(proto::InterruptKind::AgentReview) => Ok(InterruptKind::AgentReview),
            _ => Err(Error::validation(format!("Invalid InterruptKind: {}", value))),
        }
    }
}

impl From<InterruptKind> for i32 {
    fn from(kind: InterruptKind) -> i32 {
        match kind {
            InterruptKind::Clarification => proto::InterruptKind::Clarification as i32,
            InterruptKind::Confirmation => proto::InterruptKind::Confirmation as i32,
            InterruptKind::Checkpoint => proto::InterruptKind::Checkpoint as i32,
            InterruptKind::ResourceExhausted => proto::InterruptKind::ResourceExhausted as i32,
            InterruptKind::Timeout => proto::InterruptKind::Timeout as i32,
            InterruptKind::SystemError => proto::InterruptKind::SystemError as i32,
            InterruptKind::AgentReview => proto::InterruptKind::AgentReview as i32,
        }
    }
}

// =============================================================================
// TerminalReason conversions
// =============================================================================

impl TryFrom<i32> for TerminalReason {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match proto::TerminalReason::try_from(value) {
            Ok(proto::TerminalReason::Completed) => Ok(TerminalReason::Completed),
            Ok(proto::TerminalReason::MaxIterationsExceeded) => {
                Ok(TerminalReason::MaxIterationsExceeded)
            }
            Ok(proto::TerminalReason::MaxLlmCallsExceeded) => {
                Ok(TerminalReason::MaxLlmCallsExceeded)
            }
            Ok(proto::TerminalReason::MaxAgentHopsExceeded) => {
                Ok(TerminalReason::MaxAgentHopsExceeded)
            }
            Ok(proto::TerminalReason::UserCancelled) => Ok(TerminalReason::UserCancelled),
            Ok(proto::TerminalReason::ToolFailedFatally) => Ok(TerminalReason::ToolFailedFatally),
            Ok(proto::TerminalReason::LlmFailedFatally) => Ok(TerminalReason::LlmFailedFatally),
            Ok(proto::TerminalReason::PolicyViolation) => Ok(TerminalReason::PolicyViolation),
            _ => Err(Error::validation(format!(
                "Invalid TerminalReason: {}",
                value
            ))),
        }
    }
}

impl From<TerminalReason> for i32 {
    fn from(reason: TerminalReason) -> i32 {
        match reason {
            TerminalReason::Completed => proto::TerminalReason::Completed as i32,
            TerminalReason::MaxIterationsExceeded => {
                proto::TerminalReason::MaxIterationsExceeded as i32
            }
            TerminalReason::MaxLlmCallsExceeded => {
                proto::TerminalReason::MaxLlmCallsExceeded as i32
            }
            TerminalReason::MaxAgentHopsExceeded => {
                proto::TerminalReason::MaxAgentHopsExceeded as i32
            }
            TerminalReason::UserCancelled => proto::TerminalReason::UserCancelled as i32,
            TerminalReason::ToolFailedFatally => proto::TerminalReason::ToolFailedFatally as i32,
            TerminalReason::LlmFailedFatally => proto::TerminalReason::LlmFailedFatally as i32,
            TerminalReason::PolicyViolation => proto::TerminalReason::PolicyViolation as i32,
        }
    }
}

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

        Ok(ProcessControlBlock {
            pid: proto.pid,
            request_id: proto.request_id,
            user_id: proto.user_id,
            session_id: proto.session_id,
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
            parent_pid: if proto.parent_pid.is_empty() {
                None
            } else {
                Some(proto.parent_pid)
            },
            child_pids: proto.child_pids,
        })
    }
}

impl From<ProcessControlBlock> for proto::ProcessControlBlock {
    fn from(pcb: ProcessControlBlock) -> proto::ProcessControlBlock {
        proto::ProcessControlBlock {
            pid: pcb.pid,
            request_id: pcb.request_id,
            user_id: pcb.user_id,
            session_id: pcb.session_id,
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
            parent_pid: pcb.parent_pid.unwrap_or_default(),
            child_pids: pcb.child_pids,
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

// InstructionKind conversions
impl From<InstructionKind> for i32 {
    fn from(kind: InstructionKind) -> i32 {
        match kind {
            InstructionKind::RunAgent => proto::InstructionKind::RunAgent as i32,
            InstructionKind::Terminate => proto::InstructionKind::Terminate as i32,
            InstructionKind::WaitInterrupt => proto::InstructionKind::WaitInterrupt as i32,
        }
    }
}

impl TryFrom<i32> for InstructionKind {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match proto::InstructionKind::try_from(value) {
            Ok(proto::InstructionKind::RunAgent) => Ok(InstructionKind::RunAgent),
            Ok(proto::InstructionKind::Terminate) => Ok(InstructionKind::Terminate),
            Ok(proto::InstructionKind::WaitInterrupt) => Ok(InstructionKind::WaitInterrupt),
            _ => Err(Error::validation(format!(
                "Invalid InstructionKind: {}",
                value
            ))),
        }
    }
}

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
