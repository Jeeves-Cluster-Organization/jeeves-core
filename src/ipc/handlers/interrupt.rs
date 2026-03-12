//! Interrupt service handler — create, resolve, cancel, query interrupts.

use crate::envelope::{InterruptKind, InterruptResponse};
use crate::ipc::handlers::validation::parse_enum;
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
    parse_enum::<InterruptKind>(s, "interrupt kind")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::Kernel;

    async fn call(kernel: &mut Kernel, method: &str, body: Value) -> Value {
        match handle(kernel, method, body).await.unwrap() {
            DispatchResponse::Single(v) => v,
            _ => panic!("Expected Single response"),
        }
    }

    #[tokio::test]
    async fn test_create_interrupt() {
        let mut kernel = Kernel::new();
        let r = call(
            &mut kernel,
            "CreateInterrupt",
            serde_json::json!({
                "kind": "confirmation",
                "request_id": "req-1",
                "user_id": "user-1",
                "session_id": "sess-1",
                "envelope_id": "env-1",
                "question": "Please confirm",
            }),
        )
        .await;
        assert!(r.get("id").is_some());
        assert_eq!(r["status"], "pending");
    }

    #[tokio::test]
    async fn test_resolve_interrupt() {
        let mut kernel = Kernel::new();
        let created = call(
            &mut kernel,
            "CreateInterrupt",
            serde_json::json!({
                "kind": "confirmation",
                "request_id": "req-1",
                "user_id": "user-1",
                "session_id": "sess-1",
                "envelope_id": "env-1",
            }),
        )
        .await;
        let interrupt_id = created["id"].as_str().unwrap();

        let r = call(
            &mut kernel,
            "ResolveInterrupt",
            serde_json::json!({
                "interrupt_id": interrupt_id,
                "response": {
                    "approved": true,
                    "received_at": "2026-03-13T00:00:00Z",
                },
                "user_id": "user-1",
            }),
        )
        .await;
        assert_eq!(r["success"], true);
    }

    #[tokio::test]
    async fn test_cancel_interrupt() {
        let mut kernel = Kernel::new();
        let created = call(
            &mut kernel,
            "CreateInterrupt",
            serde_json::json!({
                "kind": "confirmation",
                "request_id": "req-1",
                "user_id": "user-1",
                "session_id": "sess-1",
                "envelope_id": "env-1",
            }),
        )
        .await;
        let interrupt_id = created["id"].as_str().unwrap();

        let r = call(
            &mut kernel,
            "CancelInterrupt",
            serde_json::json!({"interrupt_id": interrupt_id, "reason": "no longer needed"}),
        )
        .await;
        assert_eq!(r["success"], true);
    }

    #[tokio::test]
    async fn test_get_nonexistent_interrupt_returns_error() {
        let mut kernel = Kernel::new();
        let err = match handle(
            &mut kernel,
            "GetInterrupt",
            serde_json::json!({"interrupt_id": "nonexistent"}),
        )
        .await
        {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        assert!(err.to_string().contains("not found"));
    }
}
