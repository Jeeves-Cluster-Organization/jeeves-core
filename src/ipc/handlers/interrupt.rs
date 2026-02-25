//! Interrupt service handler — create, resolve, cancel, query interrupts.

use crate::envelope::{InterruptKind, InterruptResponse};
use crate::ipc::router::{str_field, DispatchResponse};
use crate::kernel::interrupts::CreateInterruptParams;
use crate::kernel::Kernel;
use crate::types::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

pub async fn handle(kernel: &mut Kernel, method: &str, body: Value) -> Result<DispatchResponse> {
    match method {
        "CreateInterrupt" => {
            let kind_str = str_field(&body, "kind")?;
            let kind = parse_interrupt_kind(&kind_str)?;
            let request_id = str_field(&body, "request_id")?;
            let user_id = str_field(&body, "user_id")?;
            let session_id = str_field(&body, "session_id")?;
            let envelope_id = str_field(&body, "envelope_id")?;
            let question = body
                .get("question")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let message = body
                .get("message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let data = body
                .get("data")
                .and_then(|v| v.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<HashMap<String, Value>>()
                });

            let interrupt = kernel.create_interrupt(CreateInterruptParams {
                kind,
                request_id,
                user_id,
                session_id,
                envelope_id,
                question,
                message,
                data,
                trace_id: None,
                span_id: None,
            });

            let value = serde_json::to_value(&interrupt)
                .map_err(|e| Error::internal(format!("Serialization failed: {}", e)))?;
            Ok(DispatchResponse::Single(value))
        }

        "ResolveInterrupt" => {
            let interrupt_id = str_field(&body, "interrupt_id")?;
            let user_id = body
                .get("user_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let response_val = body
                .get("response")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
            let response: InterruptResponse = serde_json::from_value(response_val)
                .map_err(|e| Error::validation(format!("Invalid response: {}", e)))?;

            let success =
                kernel.resolve_interrupt(&interrupt_id, response, user_id.as_deref());

            Ok(DispatchResponse::Single(serde_json::json!({
                "success": success,
            })))
        }

        "CancelInterrupt" => {
            let interrupt_id = str_field(&body, "interrupt_id")?;
            let reason = body
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("cancelled")
                .to_string();

            let success = kernel.cancel_interrupt(&interrupt_id, reason);

            Ok(DispatchResponse::Single(serde_json::json!({
                "success": success,
            })))
        }

        "GetInterrupt" => {
            let interrupt_id = str_field(&body, "interrupt_id")?;

            let interrupt = kernel.get_interrupt(&interrupt_id);

            match interrupt {
                Some(i) => {
                    let value = serde_json::to_value(i)
                        .map_err(|e| Error::internal(format!("Serialization failed: {}", e)))?;
                    Ok(DispatchResponse::Single(value))
                }
                None => Err(Error::not_found(format!(
                    "Interrupt {} not found",
                    interrupt_id
                ))),
            }
        }

        "GetPendingForSession" => {
            let session_id = str_field(&body, "session_id")?;
            let kinds = body
                .get("kinds")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .filter_map(|s| parse_interrupt_kind(s).ok())
                        .collect::<Vec<_>>()
                });

            let interrupts =
                kernel.get_pending_for_session(&session_id, kinds.as_deref());

            let values: Vec<Value> = interrupts
                .iter()
                .filter_map(|i| serde_json::to_value(i).ok())
                .collect();

            Ok(DispatchResponse::Single(
                serde_json::json!({ "interrupts": values }),
            ))
        }

        _ => Err(Error::not_found(format!(
            "Unknown interrupt method: {}",
            method
        ))),
    }
}

fn parse_interrupt_kind(s: &str) -> Result<InterruptKind> {
    match s {
        "clarification" => Ok(InterruptKind::Clarification),
        "confirmation" => Ok(InterruptKind::Confirmation),
        "agent_review" => Ok(InterruptKind::AgentReview),
        "checkpoint" => Ok(InterruptKind::Checkpoint),
        "resource_exhausted" => Ok(InterruptKind::ResourceExhausted),
        "timeout" => Ok(InterruptKind::Timeout),
        "system_error" => Ok(InterruptKind::SystemError),
        _ => Err(Error::validation(format!("Invalid interrupt kind: {}", s))),
    }
}
