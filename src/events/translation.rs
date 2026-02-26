//! Event translation — kernel lifecycle events → frontend WebSocket events.
//!
//! Pure deterministic mapping. Replaces Python's `_translate_event()` (14 untyped
//! dict accesses, 5 stringly-typed dispatches) with compile-time-safe field access.
//!
//! Translation rules:
//!   process.created       → orchestrator.started
//!   process.state_changed → orchestrator.completed (TERMINATED only; WAITING filtered)
//!   resource.exhausted    → orchestrator.resource_exhausted
//!   process.cancelled     → orchestrator.cancelled
//!   (all others)          → None (not forwarded to frontend)

use serde_json::Value;

/// Translate a kernel lifecycle event into a frontend-friendly event.
///
/// Returns `Some((frontend_event_type, frontend_payload))` or `None` if the
/// event should not be forwarded to the frontend.
pub fn translate_kernel_event(event_type: &str, payload: &Value) -> Option<(String, Value)> {
    match event_type {
        "process.created" => {
            let request_id = payload
                .get("request_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let pid = payload
                .get("pid")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            Some((
                "orchestrator.started".to_string(),
                serde_json::json!({
                    "request_id": request_id,
                    "pid": pid,
                }),
            ))
        }

        "process.state_changed" => {
            let new_state = payload
                .get("new_state")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            match new_state {
                "TERMINATED" => {
                    let pid = payload
                        .get("pid")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    Some((
                        "orchestrator.completed".to_string(),
                        serde_json::json!({
                            "request_id": pid,
                            "status": "completed",
                        }),
                    ))
                }
                // WAITING and other state changes not forwarded to frontend
                _ => None,
            }
        }

        "resource.exhausted" => {
            let pid = payload
                .get("pid")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let reason = payload
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let usage = payload.get("usage").cloned().unwrap_or(Value::Null);

            Some((
                "orchestrator.resource_exhausted".to_string(),
                serde_json::json!({
                    "request_id": pid,
                    "resource": reason,
                    "usage": usage,
                }),
            ))
        }

        "process.cancelled" => {
            let pid = payload
                .get("pid")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let reason = payload
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            Some((
                "orchestrator.cancelled".to_string(),
                serde_json::json!({
                    "request_id": pid,
                    "reason": reason,
                }),
            ))
        }

        // process.terminated, process.resumed, and other internal events
        // are not forwarded to the frontend
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_created() {
        let payload = serde_json::json!({
            "pid": "pid-123",
            "request_id": "req-1",
            "user_id": "user-1",
            "session_id": "sess-1",
        });

        let result = translate_kernel_event("process.created", &payload);
        assert!(result.is_some());

        let (event_type, data) = result.unwrap();
        assert_eq!(event_type, "orchestrator.started");
        assert_eq!(data["request_id"], "req-1");
        assert_eq!(data["pid"], "pid-123");
    }

    #[test]
    fn test_state_changed_terminated() {
        let payload = serde_json::json!({
            "pid": "pid-123",
            "old_state": "RUNNING",
            "new_state": "TERMINATED",
        });

        let result = translate_kernel_event("process.state_changed", &payload);
        assert!(result.is_some());

        let (event_type, data) = result.unwrap();
        assert_eq!(event_type, "orchestrator.completed");
        assert_eq!(data["request_id"], "pid-123");
        assert_eq!(data["status"], "completed");
    }

    #[test]
    fn test_state_changed_waiting_filtered() {
        let payload = serde_json::json!({
            "pid": "pid-123",
            "old_state": "RUNNING",
            "new_state": "WAITING",
        });

        let result = translate_kernel_event("process.state_changed", &payload);
        assert!(result.is_none());
    }

    #[test]
    fn test_state_changed_ready_filtered() {
        let payload = serde_json::json!({
            "pid": "pid-123",
            "old_state": "NEW",
            "new_state": "READY",
        });

        let result = translate_kernel_event("process.state_changed", &payload);
        assert!(result.is_none());
    }

    #[test]
    fn test_resource_exhausted() {
        let payload = serde_json::json!({
            "pid": "pid-123",
            "reason": "llm_calls exceeded",
            "usage": {"llm_calls": 10},
        });

        let result = translate_kernel_event("resource.exhausted", &payload);
        assert!(result.is_some());

        let (event_type, data) = result.unwrap();
        assert_eq!(event_type, "orchestrator.resource_exhausted");
        assert_eq!(data["request_id"], "pid-123");
        assert_eq!(data["resource"], "llm_calls exceeded");
        assert_eq!(data["usage"]["llm_calls"], 10);
    }

    #[test]
    fn test_process_cancelled() {
        let payload = serde_json::json!({
            "pid": "pid-123",
            "reason": "User cancelled",
        });

        let result = translate_kernel_event("process.cancelled", &payload);
        assert!(result.is_some());

        let (event_type, data) = result.unwrap();
        assert_eq!(event_type, "orchestrator.cancelled");
        assert_eq!(data["request_id"], "pid-123");
        assert_eq!(data["reason"], "User cancelled");
    }

    #[test]
    fn test_unknown_event_filtered() {
        let payload = serde_json::json!({});
        assert!(translate_kernel_event("internal.debug", &payload).is_none());
        assert!(translate_kernel_event("process.terminated", &payload).is_none());
        assert!(translate_kernel_event("process.resumed", &payload).is_none());
    }
}
