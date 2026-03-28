//! MCP stdio server — serves tools over stdin/stdout via JSON-RPC 2.0.
//!
//! Implements the MCP server protocol (Model Context Protocol) using stdio transport.
//! Reads JSON-RPC requests from stdin (newline-delimited), dispatches to `ToolRegistry`,
//! writes JSON-RPC responses to stdout.
//!
//! Supported methods:
//! - `initialize` — handshake, returns server capabilities
//! - `notifications/initialized` — client acknowledgment (no response)
//! - `tools/list` — list available tools from ToolRegistry
//! - `tools/call` — execute a tool by name

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::worker::tools::ToolRegistry;

// =============================================================================
// JSON-RPC types
// =============================================================================

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: Option<serde_json::Value>,
    /// MCP spec allows id to be number, string, or absent (notifications).
    id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

impl JsonRpcResponse {
    fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<serde_json::Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// JSON-RPC error codes
const PARSE_ERROR: i64 = -32700;
const METHOD_NOT_FOUND: i64 = -32601;
const INTERNAL_ERROR: i64 = -32603;

// =============================================================================
// MCP stdio server
// =============================================================================

/// MCP stdio server — reads from stdin, writes to stdout.
#[derive(Debug)]
pub struct McpStdioServer {
    tools: Arc<ToolRegistry>,
    server_name: String,
    server_version: String,
}

impl McpStdioServer {
    /// Create a new MCP stdio server backed by the given tool registry.
    pub fn new(tools: Arc<ToolRegistry>, server_name: String, server_version: String) -> Self {
        Self {
            tools,
            server_name,
            server_version,
        }
    }

    /// Run the server loop: read stdin line-by-line, dispatch, write to stdout.
    /// Returns when stdin is closed (EOF).
    pub async fn run(&self) -> crate::Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();
            let n = reader
                .read_line(&mut line)
                .await
                .map_err(|e| crate::Error::internal(format!("stdin read error: {e}")))?;

            if n == 0 {
                // EOF — client disconnected
                tracing::info!("MCP stdio: stdin closed, shutting down");
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Parse JSON-RPC request
            let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(req) => req,
                Err(e) => {
                    let resp = JsonRpcResponse::error(
                        None,
                        PARSE_ERROR,
                        format!("Parse error: {e}"),
                    );
                    write_response(&mut stdout, &resp).await?;
                    continue;
                }
            };

            // Notifications (no id) get no response
            let is_notification = request.id.is_none();

            // Dispatch
            let response = self.dispatch(&request).await;

            if !is_notification {
                if let Some(resp) = response {
                    write_response(&mut stdout, &resp).await?;
                }
            }
        }

        Ok(())
    }

    async fn dispatch(&self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = request.id.clone();

        match request.method.as_str() {
            "initialize" => Some(self.handle_initialize(id)),
            "notifications/initialized" => {
                tracing::debug!("MCP stdio: client initialized");
                None // notification — no response
            }
            "tools/list" => Some(self.handle_tools_list(id)),
            "tools/call" => Some(self.handle_tools_call(id, &request.params).await),
            method => {
                tracing::warn!(method, "MCP stdio: unknown method");
                Some(JsonRpcResponse::error(
                    id,
                    METHOD_NOT_FOUND,
                    format!("Method not found: {method}"),
                ))
            }
        }
    }

    fn handle_initialize(&self, id: Option<serde_json::Value>) -> JsonRpcResponse {
        tracing::info!("MCP stdio: initialize handshake");
        JsonRpcResponse::success(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": self.server_name,
                    "version": self.server_version
                }
            }),
        )
    }

    fn handle_tools_list(&self, id: Option<serde_json::Value>) -> JsonRpcResponse {
        let tools: Vec<serde_json::Value> = self
            .tools
            .list_all_tools()
            .into_iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.parameters,
                })
            })
            .collect();

        JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }))
    }

    async fn handle_tools_call(
        &self,
        id: Option<serde_json::Value>,
        params: &Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let (name, arguments) = match params {
            Some(p) => {
                let name = p
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let arguments = p
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                (name, arguments)
            }
            None => {
                return JsonRpcResponse::error(
                    id,
                    INTERNAL_ERROR,
                    "tools/call requires params with 'name' and 'arguments'",
                );
            }
        };

        if name.is_empty() {
            return JsonRpcResponse::error(id, INTERNAL_ERROR, "tools/call: missing tool name");
        }

        match self.tools.execute(&name, arguments).await {
            Ok(tool_output) => {
                // MCP tools/call returns content items, not raw JSON
                let text = serde_json::to_string(&tool_output.data).unwrap_or_default();
                JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": false
                    }),
                )
            }
            Err(e) => {
                // Tool execution errors are successful JSON-RPC responses with isError=true
                JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": e.to_string() }],
                        "isError": true
                    }),
                )
            }
        }
    }
}

/// Write a JSON-RPC response as a newline-delimited JSON line to stdout.
async fn write_response(
    stdout: &mut tokio::io::Stdout,
    response: &JsonRpcResponse,
) -> crate::Result<()> {
    let payload = serde_json::to_string(response)
        .map_err(|e| crate::Error::internal(format!("response serialize error: {e}")))?;
    stdout
        .write_all(payload.as_bytes())
        .await
        .map_err(|e| crate::Error::internal(format!("stdout write error: {e}")))?;
    stdout
        .write_all(b"\n")
        .await
        .map_err(|e| crate::Error::internal(format!("stdout newline error: {e}")))?;
    stdout
        .flush()
        .await
        .map_err(|e| crate::Error::internal(format!("stdout flush error: {e}")))?;
    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_response_serialization() {
        let resp = JsonRpcResponse::success(
            Some(serde_json::json!(1)),
            serde_json::json!({"tools": []}),
        );
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert!(json["result"]["tools"].is_array());
        assert!(json.get("error").is_none());
    }

    #[test]
    fn test_error_response_serialization() {
        let resp = JsonRpcResponse::error(
            Some(serde_json::json!(42)),
            METHOD_NOT_FOUND,
            "Method not found: foo",
        );
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 42);
        assert!(json.get("result").is_none());
        assert_eq!(json["error"]["code"], -32601);
        assert_eq!(json["error"]["message"], "Method not found: foo");
    }

    #[test]
    fn test_parse_initialize_request() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).expect("parse");
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, Some(serde_json::json!(1)));
    }

    #[test]
    fn test_parse_notification_no_id() {
        let json = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).expect("parse");
        assert_eq!(req.method, "notifications/initialized");
        assert!(req.id.is_none());
    }

    #[test]
    fn test_parse_tools_call_request() {
        let json = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hello"}}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).expect("parse");
        assert_eq!(req.method, "tools/call");
        let params = req.params.expect("params");
        assert_eq!(params["name"], "echo");
        assert_eq!(params["arguments"]["text"], "hello");
    }

    #[test]
    fn test_string_id_accepted() {
        let json = r#"{"jsonrpc":"2.0","id":"abc-123","method":"tools/list"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).expect("parse");
        assert_eq!(req.id, Some(serde_json::json!("abc-123")));
    }
}
