//! Tools service handler — catalog, validation, access control, prompt generation.

use crate::ipc::router::{str_field, DispatchResponse};
use crate::kernel::Kernel;
use crate::tools::ToolEntry;
use crate::types::{Error, Result};
use serde_json::Value;

pub async fn handle(kernel: &mut Kernel, method: &str, body: Value) -> Result<DispatchResponse> {
    match method {
        "RegisterTool" => {
            let entry: ToolEntry = serde_json::from_value(body.clone())
                .map_err(|e| Error::validation(format!("Invalid tool entry: {}", e)))?;

            kernel.tool_catalog_mut().register(entry)?;

            Ok(DispatchResponse::Single(serde_json::json!({
                "registered": true,
            })))
        }

        "GetToolEntry" => {
            let tool_id = str_field(&body, "tool_id")?;

            let entry = kernel
                .tool_catalog()
                .get(&tool_id)
                .ok_or_else(|| Error::not_found(format!("Unknown tool: {}", tool_id)))?;

            let value = serde_json::to_value(entry)
                .map_err(|e| Error::internal(format!("Serialization error: {}", e)))?;

            Ok(DispatchResponse::Single(value))
        }

        "ListTools" => {
            let entries: Vec<Value> = kernel
                .tool_catalog()
                .list_entries()
                .iter()
                .filter_map(|e| serde_json::to_value(*e).ok())
                .collect();

            Ok(DispatchResponse::Single(serde_json::json!({
                "tools": entries,
                "count": entries.len(),
            })))
        }

        "ValidateToolParams" => {
            let tool_id = str_field(&body, "tool_id")?;
            let params = body
                .get("params")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

            let errors = kernel.tool_catalog().validate_params(&tool_id, &params)?;

            Ok(DispatchResponse::Single(serde_json::json!({
                "valid": errors.is_empty(),
                "errors": errors,
            })))
        }

        "FillDefaults" => {
            let tool_id = str_field(&body, "tool_id")?;
            let mut params = body
                .get("params")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

            kernel.tool_catalog().fill_defaults(&tool_id, &mut params)?;

            Ok(DispatchResponse::Single(serde_json::json!({
                "params": params,
            })))
        }

        "GenerateToolPrompt" => {
            let allowed_tools: Option<Vec<String>> = body
                .get("allowed_tools")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                });

            let prompt = kernel
                .tool_catalog()
                .generate_prompt(allowed_tools.as_deref());

            Ok(DispatchResponse::Single(serde_json::json!({
                "prompt": prompt,
            })))
        }

        "GrantToolAccess" => {
            let agent_name = str_field(&body, "agent_name")?;
            let tool_ids: Vec<String> = body
                .get("tool_ids")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            kernel.tool_access_mut().grant_many(&agent_name, &tool_ids);

            Ok(DispatchResponse::Single(serde_json::json!({
                "granted": true,
                "agent_name": agent_name,
                "tool_count": tool_ids.len(),
            })))
        }

        "RevokeToolAccess" => {
            let agent_name = str_field(&body, "agent_name")?;
            let tool_id = str_field(&body, "tool_id")?;

            kernel.tool_access_mut().revoke(&agent_name, &tool_id);

            Ok(DispatchResponse::Single(serde_json::json!({
                "revoked": true,
            })))
        }

        "CheckToolAccess" => {
            let agent_name = str_field(&body, "agent_name")?;
            let tool_id = str_field(&body, "tool_id")?;

            let allowed = kernel.tool_access().check_access(&agent_name, &tool_id);

            Ok(DispatchResponse::Single(serde_json::json!({
                "allowed": allowed,
            })))
        }

        "GetAgentTools" => {
            let agent_name = str_field(&body, "agent_name")?;

            let tool_ids = kernel.tool_access().tools_for_agent(&agent_name);

            Ok(DispatchResponse::Single(serde_json::json!({
                "agent_name": agent_name,
                "tool_ids": tool_ids,
            })))
        }

        // =================================================================
        // Health & Circuit Breaking
        // =================================================================

        "RecordToolExecution" => {
            let tool_name = str_field(&body, "tool_name")?;
            let success = body.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
            let latency_ms = body
                .get("latency_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let error_type = body
                .get("error_type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            kernel
                .tool_health_mut()
                .record_execution(&tool_name, success, latency_ms, error_type);

            Ok(DispatchResponse::Single(serde_json::json!({
                "recorded": true,
            })))
        }

        "CheckToolHealth" => {
            let tool_name = str_field(&body, "tool_name")?;

            let report = kernel.tool_health().check_tool_health(&tool_name);
            let value = serde_json::to_value(&report)
                .map_err(|e| Error::internal(format!("Serialization error: {}", e)))?;

            Ok(DispatchResponse::Single(value))
        }

        "GetSystemToolHealth" => {
            let report = kernel.tool_health().check_system_health();
            let value = serde_json::to_value(&report)
                .map_err(|e| Error::internal(format!("Serialization error: {}", e)))?;

            Ok(DispatchResponse::Single(value))
        }

        "ShouldCircuitBreak" => {
            let tool_name = str_field(&body, "tool_name")?;

            let should_break = kernel.tool_health().should_circuit_break(&tool_name);

            Ok(DispatchResponse::Single(serde_json::json!({
                "circuit_broken": should_break,
            })))
        }

        "GetErrorPatterns" => {
            let tool_name = str_field(&body, "tool_name")?;

            let patterns = kernel.tool_health().get_error_patterns(&tool_name);
            let pattern_list: Vec<Value> = patterns
                .into_iter()
                .map(|(error_type, count)| {
                    serde_json::json!({
                        "error_type": error_type,
                        "count": count,
                    })
                })
                .collect();

            Ok(DispatchResponse::Single(serde_json::json!({
                "tool_name": tool_name,
                "patterns": pattern_list,
            })))
        }

        _ => Err(Error::not_found(format!(
            "Unknown tools method: {}",
            method
        ))),
    }
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

    fn tool_json(id: &str) -> Value {
        serde_json::json!({
            "id": id,
            "description": format!("Test tool {}", id),
            "parameters": [],
            "category": "read",
            "risk_semantic": "read_only",
            "risk_severity": "low",
        })
    }

    #[tokio::test]
    async fn test_register_and_get_tool() {
        let mut kernel = Kernel::new();
        let r = call(&mut kernel, "RegisterTool", tool_json("my_tool")).await;
        assert_eq!(r["registered"], true);

        let t = call(
            &mut kernel,
            "GetToolEntry",
            serde_json::json!({"tool_id": "my_tool"}),
        )
        .await;
        assert_eq!(t["id"], "my_tool");
        assert_eq!(t["description"], "Test tool my_tool");
    }

    #[tokio::test]
    async fn test_list_tools() {
        let mut kernel = Kernel::new();
        call(&mut kernel, "RegisterTool", tool_json("a")).await;
        call(&mut kernel, "RegisterTool", tool_json("b")).await;
        let r = call(&mut kernel, "ListTools", serde_json::json!({})).await;
        assert_eq!(r["count"], 2);
    }

    #[tokio::test]
    async fn test_grant_and_check_access() {
        let mut kernel = Kernel::new();
        call(
            &mut kernel,
            "GrantToolAccess",
            serde_json::json!({"agent_name": "a1", "tool_ids": ["t1", "t2"]}),
        )
        .await;
        let r = call(
            &mut kernel,
            "CheckToolAccess",
            serde_json::json!({"agent_name": "a1", "tool_id": "t1"}),
        )
        .await;
        assert_eq!(r["allowed"], true);
    }

    #[tokio::test]
    async fn test_check_access_not_granted() {
        let mut kernel = Kernel::new();
        let r = call(
            &mut kernel,
            "CheckToolAccess",
            serde_json::json!({"agent_name": "a1", "tool_id": "t1"}),
        )
        .await;
        assert_eq!(r["allowed"], false);
    }

    #[tokio::test]
    async fn test_revoke_access() {
        let mut kernel = Kernel::new();
        call(
            &mut kernel,
            "GrantToolAccess",
            serde_json::json!({"agent_name": "a1", "tool_ids": ["t1"]}),
        )
        .await;
        call(
            &mut kernel,
            "RevokeToolAccess",
            serde_json::json!({"agent_name": "a1", "tool_id": "t1"}),
        )
        .await;
        let r = call(
            &mut kernel,
            "CheckToolAccess",
            serde_json::json!({"agent_name": "a1", "tool_id": "t1"}),
        )
        .await;
        assert_eq!(r["allowed"], false);
    }

    #[tokio::test]
    async fn test_unknown_method_returns_error() {
        let mut kernel = Kernel::new();
        let err = match handle(&mut kernel, "Bogus", serde_json::json!({})).await {
            Err(e) => e,
            Ok(_) => panic!("Expected error"),
        };
        assert!(err.to_string().contains("Unknown tools method"));
    }
}

