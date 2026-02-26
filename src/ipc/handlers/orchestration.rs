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
                kernel.initialize_orchestration(process_id, pipeline_config, envelope, force)?;

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

            Ok(DispatchResponse::Single(instruction_to_value(&instruction)))
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
                AgentExecutionMetrics {
                    llm_calls: 0,
                    tool_calls: 0,
                    tokens_in: None,
                    tokens_out: None,
                    duration_ms: 0,
                }
            };

            // Mutate envelope in-place (never extracted from HashMap — impossible to lose)
            {
                let envelope = kernel
                    .get_process_envelope_mut(&process_id)
                    .ok_or_else(|| Error::not_found(format!("Envelope not found: {}", process_id)))?;

                let mut agent_output = HashMap::new();
                if let Value::Object(output_map) = output {
                    for (key, value) in output_map {
                        agent_output.insert(key, value);
                    }
                }
                if !success {
                    agent_output.insert("success".to_string(), Value::Bool(false));
                    if !error_message.is_empty() {
                        agent_output.insert("error".to_string(), Value::String(error_message.clone()));
                    }
                    envelope.audit.metadata.insert(
                        "last_agent_failure".to_string(),
                        serde_json::json!({
                            "agent_name": agent_name,
                            "error": error_message,
                        }),
                    );
                }
                envelope.outputs.insert(agent_name.clone(), agent_output);
            }

            // Report metrics and advance pipeline (envelope stays in HashMap)
            kernel.report_agent_result(&process_id, metrics)?;

            let instruction = kernel.get_next_instruction(&process_id)?;
            Ok(DispatchResponse::Single(instruction_to_value(&instruction)))
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
// Conversion helpers
// =============================================================================

/// Convert Instruction to the dict shape expected by `kernel_client.py._dict_to_instruction`.
pub fn instruction_to_value(instr: &crate::kernel::orchestrator::Instruction) -> Value {
    let kind_str = serde_json::to_value(&instr.kind)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "UNKNOWN".to_string());

    let terminal_reason_str = instr
        .terminal_reason
        .as_ref()
        .and_then(|r| serde_json::to_value(r).ok())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    let envelope = instr
        .envelope
        .as_ref()
        .and_then(|e| serde_json::to_value(e).ok());

    serde_json::json!({
        "kind": kind_str,
        "agent_name": instr.agent_name.as_deref().unwrap_or(""),
        "envelope": envelope,
        "terminal_reason": terminal_reason_str,
        "termination_message": instr.termination_message.as_deref().unwrap_or(""),
        "interrupt_pending": instr.interrupt_pending,
        "interrupt": instr.interrupt.as_ref().and_then(|i| serde_json::to_value(i).ok()),
    })
}

/// Convert SessionState to the dict shape expected by `kernel_client.py._dict_to_session_state`.
pub fn session_state_to_value(state: &crate::kernel::orchestrator::SessionState) -> Value {
    let terminal_reason_str = state
        .terminal_reason
        .as_ref()
        .and_then(|r| serde_json::to_value(r).ok())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    serde_json::json!({
        "process_id": state.process_id.as_str(),
        "current_stage": state.current_stage,
        "stage_order": state.stage_order,
        "envelope": state.envelope.clone(),
        "edge_traversals": state.edge_traversals,
        "terminated": state.terminated,
        "terminal_reason": terminal_reason_str,
    })
}
