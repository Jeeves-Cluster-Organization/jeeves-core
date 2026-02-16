//! Engine service handler — envelope CRUD + pipeline execution.

use crate::envelope::Envelope;
use crate::ipc::dispatch::{str_field, DispatchResponse};
use crate::ipc::handlers::orchestration::instruction_to_value;
use crate::kernel::orchestrator::PipelineConfig;
use crate::kernel::Kernel;
use crate::types::{EnvelopeId, Error, ProcessId, RequestId, Result, SessionId, UserId};
use serde_json::Value;

pub async fn handle(kernel: &mut Kernel, method: &str, body: Value) -> Result<DispatchResponse> {
    match method {
        "CreateEnvelope" => {
            let raw_input = body
                .get("raw_input")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let request_id_str = body
                .get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let user_id_str = body.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
            let session_id_str = body
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let envelope_id = EnvelopeId::must(format!("env_{}", uuid::Uuid::new_v4().simple()));
            let request_id = if request_id_str.is_empty() {
                RequestId::must(format!("req_{}", uuid::Uuid::new_v4().simple()))
            } else {
                RequestId::from_string(request_id_str.to_string())
                    .map_err(|e| Error::validation(e.to_string()))?
            };
            let user_id = if user_id_str.is_empty() {
                UserId::from_string("anonymous".to_string())
                    .map_err(|e| Error::validation(e.to_string()))?
            } else {
                UserId::from_string(user_id_str.to_string())
                    .map_err(|e| Error::validation(e.to_string()))?
            };
            let session_id = if session_id_str.is_empty() {
                SessionId::new()
            } else {
                SessionId::from_string(session_id_str.to_string())
                    .map_err(|e| Error::validation(e.to_string()))?
            };

            let stage_order: Vec<String> = body
                .get("stage_order")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let mut envelope = Envelope::new();
            envelope.identity.envelope_id = envelope_id;
            envelope.identity.request_id = request_id;
            envelope.identity.user_id = user_id;
            envelope.identity.session_id = session_id;
            envelope.raw_input = raw_input;

            if !stage_order.is_empty() {
                envelope.pipeline.current_stage = stage_order[0].clone();
                envelope.pipeline.stage_order = stage_order;
            }

            kernel.store_envelope(envelope.clone());

            let env_value = serde_json::to_value(&envelope)
                .map_err(|e| Error::internal(format!("Failed to serialize envelope: {}", e)))?;
            Ok(DispatchResponse::Single(env_value))
        }

        "CheckBounds" => {
            let envelope: Envelope = serde_json::from_value(body)
                .map_err(|e| Error::validation(format!("Invalid envelope: {}", e)))?;

            let can_continue = envelope.bounds.llm_call_count < envelope.bounds.max_llm_calls
                && envelope.pipeline.iteration < envelope.pipeline.max_iterations
                && envelope.bounds.agent_hop_count < envelope.bounds.max_agent_hops;

            let terminal_reason = if envelope.bounds.llm_call_count >= envelope.bounds.max_llm_calls
            {
                "MAX_LLM_CALLS_EXCEEDED"
            } else if envelope.pipeline.iteration >= envelope.pipeline.max_iterations {
                "MAX_ITERATIONS_EXCEEDED"
            } else if envelope.bounds.agent_hop_count >= envelope.bounds.max_agent_hops {
                "MAX_AGENT_HOPS_EXCEEDED"
            } else {
                ""
            };

            Ok(DispatchResponse::Single(serde_json::json!({
                "can_continue": can_continue,
                "terminal_reason": terminal_reason,
                "llm_calls_remaining": (envelope.bounds.max_llm_calls - envelope.bounds.llm_call_count).max(0),
                "agent_hops_remaining": (envelope.bounds.max_agent_hops - envelope.bounds.agent_hop_count).max(0),
                "iterations_remaining": (envelope.pipeline.max_iterations - envelope.pipeline.iteration).max(0),
            })))
        }

        "UpdateEnvelope" => {
            let env_val = body
                .get("envelope")
                .ok_or_else(|| Error::validation("Missing required field: envelope"))?;
            let envelope: Envelope = serde_json::from_value(env_val.clone())
                .map_err(|e| Error::validation(format!("Invalid envelope: {}", e)))?;

            let env_id = envelope.identity.envelope_id.as_str().to_string();
            if kernel.get_envelope(&env_id).is_none() {
                return Err(Error::not_found(format!("Envelope {} not found", env_id)));
            }

            kernel.store_envelope(envelope.clone());
            let v = serde_json::to_value(&envelope)
                .map_err(|e| Error::internal(format!("Failed to serialize envelope: {}", e)))?;
            Ok(DispatchResponse::Single(v))
        }

        "ExecutePipeline" => {
            let envelope_str = str_field(&body, "envelope")?;
            let envelope: Envelope = serde_json::from_str(&envelope_str)
                .map_err(|e| Error::validation(format!("Invalid envelope: {}", e)))?;

            let pipeline_config_str = str_field(&body, "pipeline_config")?;
            let pipeline_config: PipelineConfig = serde_json::from_str(&pipeline_config_str)
                .map_err(|e| Error::validation(format!("Invalid pipeline_config: {}", e)))?;

            let pid = ProcessId::from_string(envelope.identity.envelope_id.as_str().to_string())
                .map_err(|e| Error::validation(e.to_string()))?;

            kernel.initialize_orchestration(pid.clone(), pipeline_config, envelope, false)?;
            let instruction = kernel.get_next_instruction(&pid)?;
            Ok(DispatchResponse::Single(instruction_to_value(&instruction)))
        }

        "CloneEnvelope" => {
            let env_val = body
                .get("envelope")
                .ok_or_else(|| Error::validation("Missing required field: envelope"))?;
            let mut cloned: Envelope = serde_json::from_value(env_val.clone())
                .map_err(|e| Error::validation(format!("Invalid envelope: {}", e)))?;

            cloned.identity.envelope_id = EnvelopeId::new();
            kernel.store_envelope(cloned.clone());
            let v = serde_json::to_value(&cloned)
                .map_err(|e| Error::internal(format!("Failed to serialize envelope: {}", e)))?;
            Ok(DispatchResponse::Single(v))
        }

        _ => Err(Error::not_found(format!(
            "Unknown engine method: {}",
            method
        ))),
    }
}
