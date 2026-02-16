//! CommBus service handler — publish, send, query, subscribe (streaming).

use crate::commbus::{Command, Event, Query};
use crate::ipc::dispatch::{str_field, DispatchResponse};
use crate::kernel::Kernel;
use crate::types::{Error, IpcConfig, Result};
use serde_json::Value;
use tokio::sync::mpsc;

pub async fn handle(
    kernel: &mut Kernel,
    method: &str,
    body: Value,
    ipc_config: &IpcConfig,
) -> Result<DispatchResponse> {
    match method {
        "Publish" => {
            let event_type = str_field(&body, "event_type")?;
            let payload = payload_bytes(&body)?;
            let source = body
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let event = Event {
                event_type,
                payload,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                source,
            };

            let delivered = kernel
                .commbus
                .publish(event)
                .await
                .map_err(|e| Error::internal(format!("CommBus publish failed: {}", e)))?;

            Ok(DispatchResponse::Single(serde_json::json!({
                "success": true,
                "delivered": delivered,
            })))
        }

        "Send" => {
            let command_type = str_field(&body, "command_type")?;
            let payload = payload_bytes(&body)?;
            let source = body
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let command = Command {
                command_type,
                payload,
                source,
            };

            kernel
                .commbus
                .send_command(command)
                .await
                .map_err(|e| Error::internal(format!("CommBus send failed: {}", e)))?;

            Ok(DispatchResponse::Single(serde_json::json!({
                "success": true,
                "error": "",
            })))
        }

        "Query" => {
            let query_type = str_field(&body, "query_type")?;
            let payload = payload_bytes(&body)?;
            let source = body
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let timeout_ms = body
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(ipc_config.default_query_timeout_ms)
                .min(ipc_config.max_query_timeout_ms);

            let query = Query {
                query_type,
                payload,
                timeout_ms,
                source,
            };

            let response = kernel
                .commbus
                .query(query)
                .await
                .map_err(|e| Error::internal(format!("CommBus query failed: {}", e)))?;

            let result_str = String::from_utf8(response.result).unwrap_or_default();
            let result_val: Value = serde_json::from_str(&result_str).unwrap_or(Value::Null);

            Ok(DispatchResponse::Single(serde_json::json!({
                "success": response.success,
                "result": result_val,
                "error": response.error,
            })))
        }

        "Subscribe" => {
            let event_types: Vec<String> = body
                .get("event_types")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if event_types.is_empty() {
                return Err(Error::validation(
                    "event_types is required (non-empty array)",
                ));
            }

            let sub_id = body
                .get("subscriber_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| format!("ipc-sub-{}", uuid::Uuid::new_v4().simple()));

            let (_subscription, event_rx) = kernel
                .commbus
                .subscribe(sub_id, event_types)
                .await
                .map_err(|e| Error::internal(format!("Subscribe failed: {}", e)))?;

            // Bridge UnboundedReceiver<Event> → bounded mpsc::Receiver<Value>
            let (tx, rx) = mpsc::channel(ipc_config.stream_channel_capacity);
            tokio::spawn(async move {
                let mut event_rx = event_rx;
                while let Some(event) = event_rx.recv().await {
                    let chunk = serde_json::json!({
                        "event_type": event.event_type,
                        "payload": String::from_utf8_lossy(&event.payload).to_string(),
                        "timestamp_ms": event.timestamp_ms,
                        "source": event.source,
                    });
                    if tx.send(chunk).await.is_err() {
                        break; // Consumer disconnected
                    }
                }
            });

            Ok(DispatchResponse::Stream(rx))
        }

        _ => Err(Error::not_found(format!(
            "Unknown commbus method: {}",
            method
        ))),
    }
}

/// Parse and validate the `payload` field as JSON, returning raw bytes.
fn payload_bytes(body: &Value) -> Result<Vec<u8>> {
    let s = body.get("payload").and_then(|v| v.as_str()).unwrap_or("{}");
    // Validate it's valid JSON
    serde_json::from_str::<Value>(s)
        .map_err(|e| Error::validation(format!("Invalid JSON payload: {}", e)))?;
    Ok(s.as_bytes().to_vec())
}
