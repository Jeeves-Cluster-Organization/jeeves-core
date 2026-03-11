//! Orchestration service handler — session management, instruction pipeline.

use crate::envelope::Envelope;
use crate::ipc::handlers::validation::{
    parse_non_negative_i32, parse_optional_non_negative_i64, require_non_negative_i64,
};
use crate::ipc::router::{str_field, DispatchResponse};
use crate::kernel::orchestrator::{AgentExecutionMetrics, InstructionKind, PipelineConfig};
use crate::kernel::{Kernel, SchedulingPriority};
use crate::types::{Error, ProcessId, Result};
use serde_json::Value;
use std::collections::HashMap;

pub async fn handle(kernel: &mut Kernel, method: &str, body: Value) -> Result<DispatchResponse> {
    match method {
        "InitializeSession" => {
            let process_id_str = str_field(&body, "process_id")?;
            let process_id = ProcessId::from_string(process_id_str)
                .map_err(|e| Error::validation(e.to_string()))?;

            let pipeline_config_val = body
                .get("pipeline_config")
                .ok_or_else(|| Error::validation("Missing field: pipeline_config"))?;
            let pipeline_config: PipelineConfig =
                serde_json::from_value(pipeline_config_val.clone())
                    .map_err(|e| Error::validation(format!("Invalid pipeline_config: {}", e)))?;

            let envelope_val = body
                .get("envelope")
                .ok_or_else(|| Error::validation("Missing field: envelope"))?;
            let envelope: Envelope = serde_json::from_value(envelope_val.clone())
                .map_err(|e| Error::validation(format!("Invalid envelope: {}", e)))?;
            envelope.validate()?;

            let force = body.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

            // Auto-create PCB if not already registered (Temporal-style: session init = workflow start)
            if kernel.get_process(&process_id).is_none() {
                kernel.create_process(
                    process_id.clone(),
                    envelope.identity.request_id.clone(),
                    envelope.identity.user_id.clone(),
                    envelope.identity.session_id.clone(),
                    SchedulingPriority::Normal,
                    None,
                )?;
            }

            let session_state =
                kernel.initialize_orchestration(process_id.clone(), pipeline_config, envelope, force)?;
            kernel.emit_envelope_snapshot(&process_id, "initialized");

            Ok(DispatchResponse::Single(session_state_to_value(
                &session_state,
            )))
        }

        "GetNextInstruction" => {
            let process_id_str = str_field(&body, "process_id")?;
            let process_id = ProcessId::from_string(process_id_str)
                .map_err(|e| Error::validation(e.to_string()))?;

            let instruction = kernel.get_next_instruction(&process_id)?;

            // Auto-terminate PCB when orchestrator says TERMINATE
            if instruction.kind == InstructionKind::Terminate {
                let _ = kernel.terminate_process(&process_id);
            }

            let envelope = kernel.get_process_envelope(&process_id);
            Ok(DispatchResponse::Single(instruction_to_value(&instruction, envelope)))
        }

        "ReportAgentResult" => {
            let process_id_str = str_field(&body, "process_id")?;
            let process_id = ProcessId::from_string(process_id_str)
                .map_err(|e| Error::validation(e.to_string()))?;

            let agent_name = str_field(&body, "agent_name")?.to_string();
            let success = body.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
            let error_message = body
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let output: Value = body
                .get("output")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

            let metrics_val = body.get("metrics");
            let metrics = if let Some(m) = metrics_val {
                AgentExecutionMetrics {
                    llm_calls: parse_non_negative_i32(
                        m.get("llm_calls").and_then(|v| v.as_i64()).unwrap_or(0),
                        "metrics.llm_calls",
                    )?,
                    tool_calls: parse_non_negative_i32(
                        m.get("tool_calls").and_then(|v| v.as_i64()).unwrap_or(0),
                        "metrics.tool_calls",
                    )?,
                    tokens_in: parse_optional_non_negative_i64(
                        m.get("tokens_in"),
                        "metrics.tokens_in",
                    )?,
                    tokens_out: parse_optional_non_negative_i64(
                        m.get("tokens_out"),
                        "metrics.tokens_out",
                    )?,
                    duration_ms: require_non_negative_i64(
                        m.get("duration_ms").and_then(|v| v.as_i64()).unwrap_or(0),
                        "metrics.duration_ms",
                    )?,
                }
            } else {
                AgentExecutionMetrics::default()
            };

            let metadata_updates: Option<HashMap<String, serde_json::Value>> = body
                .get("metadata_updates")
                .and_then(|v| serde_json::from_value(v.clone()).ok());

            let instruction = kernel.process_agent_result(
                &process_id, &agent_name, output, metadata_updates, metrics, success, &error_message,
            )?;
            let envelope = kernel.get_process_envelope(&process_id);
            Ok(DispatchResponse::Single(instruction_to_value(&instruction, envelope)))
        }

        "GetSessionState" => {
            let process_id_str = str_field(&body, "process_id")?;
            let process_id = ProcessId::from_string(process_id_str)
                .map_err(|e| Error::validation(e.to_string()))?;

            let session_state = kernel.get_orchestration_state(&process_id)?;
            Ok(DispatchResponse::Single(session_state_to_value(
                &session_state,
            )))
        }

        _ => Err(Error::not_found(format!(
            "Unknown orchestration method: {}",
            method
        ))),
    }
}

// =============================================================================
// DTO structs — derive(Serialize) replaces manual json! construction
// =============================================================================

/// DTO for Instruction → JSON, matching `kernel_client.py._dict_to_instruction`.
#[derive(serde::Serialize)]
struct InstructionResponse<'a> {
    kind: InstructionKind,
    agents: &'a [String],
    terminal_reason: &'a str,
    termination_message: &'a str,
    interrupt_pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    interrupt: Option<Value>,
    #[serde(default)]
    agent_config: Value,
}

/// DTO for SessionState → JSON, matching `kernel_client.py._dict_to_session_state`.
#[derive(serde::Serialize)]
struct SessionStateResponse<'a> {
    process_id: &'a str,
    current_stage: &'a str,
    stage_order: &'a [String],
    envelope: &'a Value,
    edge_traversals: &'a HashMap<String, i32>,
    terminated: bool,
    terminal_reason: &'a str,
}

/// Convert Instruction to the dict shape expected by `kernel_client.py._dict_to_instruction`.
///
/// The instruction is self-contained: agent_context, output_schema, and allowed_tools
/// are populated by the kernel's get_next_instruction enrichment phase.
pub fn instruction_to_value(
    instr: &crate::kernel::orchestrator::Instruction,
    _envelope: Option<&crate::envelope::Envelope>,
) -> Value {
    let terminal_reason_str = instr
        .terminal_reason
        .as_ref()
        .and_then(|r| serde_json::to_value(r).ok())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    // Build agent_config bundle from enriched instruction fields
    let mut agent_config = serde_json::Map::new();
    if let Some(ref ctx) = instr.agent_context {
        agent_config.insert("context".to_string(), ctx.clone());
    }
    if let Some(ref schema) = instr.output_schema {
        agent_config.insert("output_schema".to_string(), schema.clone());
    }
    if let Some(ref tools) = instr.allowed_tools {
        agent_config.insert("allowed_tools".to_string(), serde_json::json!(tools));
    }

    let dto = InstructionResponse {
        kind: instr.kind.clone(),
        agents: &instr.agents,
        terminal_reason: &terminal_reason_str,
        termination_message: instr.termination_message.as_deref().unwrap_or(""),
        interrupt_pending: instr.interrupt_pending,
        interrupt: instr.interrupt.as_ref().and_then(|i| serde_json::to_value(i).ok()),
        agent_config: serde_json::Value::Object(agent_config),
    };

    serde_json::to_value(dto).unwrap_or_default()
}

/// Convert SessionState to the dict shape expected by `kernel_client.py._dict_to_session_state`.
pub fn session_state_to_value(state: &crate::kernel::orchestrator::SessionState) -> Value {
    let terminal_reason_str = state
        .terminal_reason
        .as_ref()
        .and_then(|r| serde_json::to_value(r).ok())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    let dto = SessionStateResponse {
        process_id: state.process_id.as_str(),
        current_stage: &state.current_stage,
        stage_order: &state.stage_order,
        envelope: &state.envelope,
        edge_traversals: &state.edge_traversals,
        terminated: state.terminated,
        terminal_reason: &terminal_reason_str,
    };

    serde_json::to_value(dto).unwrap_or_default()
}
