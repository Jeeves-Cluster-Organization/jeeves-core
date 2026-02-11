//! Orchestration service handler — session management, instruction pipeline.

use crate::envelope::Envelope;
use crate::ipc::dispatch::{str_field, DispatchResponse};
use crate::kernel::orchestrator::{AgentExecutionMetrics, PipelineConfig};
use crate::kernel::Kernel;
use crate::types::{Error, ProcessId, Result};
use serde_json::Value;
use std::collections::HashMap;

pub async fn handle(
    kernel: &mut Kernel,
    method: &str,
    body: Value,
) -> Result<DispatchResponse> {
    match method {
        "InitializeSession" => {
            let process_id_str = str_field(&body, "process_id")?;
            let process_id = ProcessId::from_string(process_id_str)
                .map_err(|e| Error::validation(e.to_string()))?;

            let pipeline_config_str = str_field(&body, "pipeline_config")?;
            let pipeline_config: PipelineConfig = serde_json::from_str(&pipeline_config_str)
                .map_err(|e| Error::validation(format!("Invalid pipeline_config: {}", e)))?;

            let envelope_str = str_field(&body, "envelope")?;
            let envelope: Envelope = serde_json::from_str(&envelope_str)
                .map_err(|e| Error::validation(format!("Invalid envelope: {}", e)))?;

            let force = body.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

            let session_state = kernel.initialize_orchestration(
                process_id,
                pipeline_config,
                envelope,
                force,
            )?;

            Ok(DispatchResponse::Single(session_state_to_value(&session_state)))
        }

        "GetNextInstruction" => {
            let process_id_str = str_field(&body, "process_id")?;
            let process_id = ProcessId::from_string(process_id_str)
                .map_err(|e| Error::validation(e.to_string()))?;

            let instruction = kernel.get_next_instruction(&process_id)?;
            Ok(DispatchResponse::Single(instruction_to_value(&instruction)))
        }

        "ReportAgentResult" => {
            let process_id_str = str_field(&body, "process_id")?;
            let process_id = ProcessId::from_string(process_id_str)
                .map_err(|e| Error::validation(e.to_string()))?;

            let agent_name = str_field(&body, "agent_name")?;

            let output_str = body.get("output").and_then(|v| v.as_str()).unwrap_or("{}");
            let output: Value = if output_str.is_empty() {
                Value::Object(serde_json::Map::new())
            } else {
                serde_json::from_str(output_str)
                    .map_err(|e| Error::validation(format!("Invalid output: {}", e)))?
            };

            let metrics_val = body.get("metrics");
            let metrics = if let Some(m) = metrics_val {
                AgentExecutionMetrics {
                    llm_calls: m.get("llm_calls").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    tool_calls: m.get("tool_calls").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    tokens_in: m.get("tokens_in").and_then(|v| v.as_i64()).unwrap_or(0),
                    tokens_out: m.get("tokens_out").and_then(|v| v.as_i64()).unwrap_or(0),
                    duration_ms: m.get("duration_ms").and_then(|v| v.as_i64()).unwrap_or(0),
                }
            } else {
                AgentExecutionMetrics {
                    llm_calls: 0,
                    tool_calls: 0,
                    tokens_in: 0,
                    tokens_out: 0,
                    duration_ms: 0,
                }
            };

            let mut envelope = kernel
                .orchestrator
                .get_envelope_for_process(&process_id)
                .ok_or_else(|| Error::not_found(format!("Envelope not found: {}", process_id)))?
                .clone();

            if let Value::Object(output_map) = output {
                let mut agent_output = HashMap::new();
                for (key, value) in output_map {
                    agent_output.insert(key, value);
                }
                envelope.outputs.insert(agent_name, agent_output);
            }

            kernel.report_agent_result(&process_id, metrics, envelope)?;

            let instruction = kernel.get_next_instruction(&process_id)?;
            Ok(DispatchResponse::Single(instruction_to_value(&instruction)))
        }

        "GetSessionState" => {
            let process_id_str = str_field(&body, "process_id")?;
            let process_id = ProcessId::from_string(process_id_str)
                .map_err(|e| Error::validation(e.to_string()))?;

            let session_state = kernel.get_orchestration_state(&process_id)?;
            Ok(DispatchResponse::Single(session_state_to_value(&session_state)))
        }

        _ => Err(Error::not_found(format!("Unknown orchestration method: {}", method))),
    }
}

// =============================================================================
// Conversion helpers (used by engine handler too via pub)
// =============================================================================

/// Convert Instruction to the dict shape expected by `kernel_client.py._dict_to_instruction`.
pub fn instruction_to_value(instr: &crate::kernel::orchestrator::Instruction) -> Value {
    let kind_str = serde_json::to_value(&instr.kind)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "UNKNOWN".to_string());

    let terminal_reason_str = instr.terminal_reason.as_ref()
        .and_then(|r| serde_json::to_value(r).ok())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    let agent_config = instr.agent_config.as_ref().map(|v| v.to_string());
    let envelope = instr
        .envelope
        .as_ref()
        .and_then(|e| serde_json::to_string(e).ok());

    serde_json::json!({
        "kind": kind_str,
        "agent_name": instr.agent_name.as_deref().unwrap_or(""),
        "agent_config": agent_config,
        "envelope": envelope,
        "terminal_reason": terminal_reason_str,
        "termination_message": instr.termination_message.as_deref().unwrap_or(""),
        "interrupt_pending": instr.interrupt_pending,
        "interrupt": instr.interrupt.as_ref().and_then(|i| serde_json::to_value(i).ok()),
    })
}

/// Convert SessionState to the dict shape expected by `kernel_client.py._dict_to_session_state`.
pub fn session_state_to_value(state: &crate::kernel::orchestrator::SessionState) -> Value {
    let envelope_str = serde_json::to_string(&state.envelope).ok();
    let terminal_reason_str = state.terminal_reason.as_ref()
        .and_then(|r| serde_json::to_value(r).ok())
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    serde_json::json!({
        "process_id": state.process_id.as_str(),
        "current_stage": state.current_stage,
        "stage_order": state.stage_order,
        "envelope": envelope_str,
        "edge_traversals": state.edge_traversals,
        "terminated": state.terminated,
        "terminal_reason": terminal_reason_str,
    })
}
