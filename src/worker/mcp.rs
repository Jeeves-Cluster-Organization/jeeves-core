//! MCP (Model Context Protocol) tool executor — HTTP and stdio transports.
//!
//! Implements `ToolExecutor` so MCP tools integrate with `ToolRegistry` and
//! kernel tool ACLs without any registry changes.
//!
//! Protocol subset: `tools/list` + `tools/call` (JSON-RPC 2.0).
//! No resources, prompts, or sampling.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::types::{Error, Result};
use crate::worker::tools::{ToolExecutor, ToolInfo};

// =============================================================================
// Transport configuration
// =============================================================================

/// MCP transport — how to connect to an MCP server.
#[derive(Debug, Clone)]
pub enum McpTransport {
    /// JSON-RPC over HTTP POST.
    Http { url: String },
    /// JSON-RPC over stdin/stdout of a child process.
    Stdio { command: String, args: Vec<String> },
}

// =============================================================================
// JSON-RPC types (hand-rolled for 2 methods)
// =============================================================================

#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
    id: u64,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
}

/// MCP tool definition from `tools/list` response.
#[derive(Debug, Deserialize)]
struct McpToolDef {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "inputSchema")]
    input_schema: Option<serde_json::Value>,
}

/// MCP `tools/call` result content item.
#[derive(Debug, Deserialize)]
struct McpToolResult {
    #[serde(default)]
    content: Vec<McpContentItem>,
    #[serde(default)]
    #[allow(dead_code)]
    is_error: bool,
}

#[derive(Debug, Deserialize)]
struct McpContentItem {
    #[serde(default)]
    #[allow(dead_code)]
    r#type: String,
    #[serde(default)]
    text: Option<String>,
}

// =============================================================================
// Internal client
// =============================================================================

/// Stdio transport internals — boxed to keep enum variant sizes balanced.
struct StdioClient {
    stdin: Mutex<tokio::process::ChildStdin>,
    stdout: Mutex<BufReader<tokio::process::ChildStdout>>,
    // Held for Drop cleanup — start_kill() on drop.
    _child: Mutex<tokio::process::Child>,
}

/// Transport-specific I/O, concurrency-safe via Mutex.
enum McpClient {
    Http {
        client: reqwest::Client,
        url: String,
    },
    Stdio(Box<StdioClient>),
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpClient::Http { url, .. } => f.debug_struct("Http").field("url", url).finish(),
            McpClient::Stdio(_) => f.debug_struct("Stdio").finish(),
        }
    }
}

impl McpClient {
    async fn send(&self, request: &JsonRpcRequest<'_>) -> Result<JsonRpcResponse> {
        match self {
            McpClient::Http { client, url } => {
                let resp = client
                    .post(url)
                    .json(request)
                    .send()
                    .await
                    .map_err(|e| Error::internal(format!("MCP HTTP error: {e}")))?;

                let status = resp.status();
                if !status.is_success() {
                    return Err(Error::internal(format!("MCP HTTP status: {status}")));
                }

                resp.json::<JsonRpcResponse>()
                    .await
                    .map_err(|e| Error::internal(format!("MCP HTTP parse error: {e}")))
            }
            McpClient::Stdio(ref io) => {
                let payload = serde_json::to_string(request)
                    .map_err(|e| Error::internal(format!("MCP serialize error: {e}")))?;

                // Write request (newline-delimited JSON)
                {
                    let mut stdin = io.stdin.lock().await;
                    stdin
                        .write_all(payload.as_bytes())
                        .await
                        .map_err(|e| Error::internal(format!("MCP stdin write error: {e}")))?;
                    stdin
                        .write_all(b"\n")
                        .await
                        .map_err(|e| Error::internal(format!("MCP stdin newline error: {e}")))?;
                    stdin
                        .flush()
                        .await
                        .map_err(|e| Error::internal(format!("MCP stdin flush error: {e}")))?;
                }

                // Read response (one line)
                let mut line = String::new();
                {
                    let mut stdout = io.stdout.lock().await;
                    stdout
                        .read_line(&mut line)
                        .await
                        .map_err(|e| Error::internal(format!("MCP stdout read error: {e}")))?;
                }

                if line.is_empty() {
                    return Err(Error::internal("MCP process closed stdout"));
                }

                serde_json::from_str::<JsonRpcResponse>(&line)
                    .map_err(|e| Error::internal(format!("MCP response parse error: {e}")))
            }
        }
    }
}

// =============================================================================
// McpToolExecutor
// =============================================================================

/// MCP tool executor — connects to an MCP server, discovers tools, executes them.
///
/// Implements `ToolExecutor` so it plugs directly into `ToolRegistry`:
/// ```ignore
/// let mcp = Arc::new(McpToolExecutor::connect(transport).await?);
/// for tool in mcp.list_tools() {
///     registry.register(tool.name.clone(), mcp.clone());
/// }
/// ```
#[derive(Debug)]
pub struct McpToolExecutor {
    client: McpClient,
    tools: Vec<ToolInfo>,
    next_id: AtomicU64,
}

impl McpToolExecutor {
    /// Connect to an MCP server and discover tools via `tools/list`.
    pub async fn connect(transport: McpTransport) -> Result<Self> {
        let client = match transport {
            McpTransport::Http { url } => McpClient::Http {
                client: reqwest::Client::new(),
                url,
            },
            McpTransport::Stdio { command, args } => {
                let mut child = tokio::process::Command::new(&command)
                    .args(&args)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| {
                        Error::internal(format!("MCP spawn error ({command}): {e}"))
                    })?;

                let stdin = child.stdin.take().ok_or_else(|| {
                    Error::internal("MCP child process has no stdin")
                })?;
                let stdout = child.stdout.take().ok_or_else(|| {
                    Error::internal("MCP child process has no stdout")
                })?;

                McpClient::Stdio(Box::new(StdioClient {
                    stdin: Mutex::new(stdin),
                    stdout: Mutex::new(BufReader::new(stdout)),
                    _child: Mutex::new(child),
                }))
            }
        };

        let executor = Self {
            client,
            tools: Vec::new(),
            next_id: AtomicU64::new(1),
        };

        // Discover tools
        let tools = executor.discover_tools().await?;

        Ok(Self {
            tools,
            ..executor
        })
    }

    fn next_request_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn discover_tools(&self) -> Result<Vec<ToolInfo>> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "tools/list",
            params: None,
            id: self.next_request_id(),
        };

        let resp = self.client.send(&req).await?;

        if let Some(err) = resp.error {
            return Err(Error::internal(format!(
                "MCP tools/list error: {}",
                err.message
            )));
        }

        let result = resp
            .result
            .ok_or_else(|| Error::internal("MCP tools/list: no result"))?;

        // MCP tools/list returns { tools: [...] }
        let tools_val = result
            .get("tools")
            .ok_or_else(|| Error::internal("MCP tools/list: missing 'tools' field"))?;

        let mcp_tools: Vec<McpToolDef> = serde_json::from_value(tools_val.clone())
            .map_err(|e| Error::internal(format!("MCP tools/list parse error: {e}")))?;

        Ok(mcp_tools
            .into_iter()
            .map(|t| ToolInfo {
                name: t.name,
                description: t.description.unwrap_or_default(),
                parameters: t.input_schema.unwrap_or(serde_json::json!({})),
            })
            .collect())
    }
}

#[async_trait]
impl ToolExecutor for McpToolExecutor {
    async fn execute(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "tools/call",
            params: Some(serde_json::json!({
                "name": name,
                "arguments": params,
            })),
            id: self.next_request_id(),
        };

        let resp = self.client.send(&req).await?;

        if let Some(err) = resp.error {
            // Return as JSON value so the LLM can react in the ReAct loop
            return Ok(serde_json::json!({"error": err.message}));
        }

        let result = resp
            .result
            .ok_or_else(|| Error::internal("MCP tools/call: no result"))?;

        // MCP tools/call returns { content: [{type, text}], isError }
        // Try to extract structured content; fall back to raw result
        if let Ok(tool_result) = serde_json::from_value::<McpToolResult>(result.clone()) {
            let text: String = tool_result
                .content
                .iter()
                .filter_map(|c| c.text.as_deref())
                .collect::<Vec<_>>()
                .join("\n");

            // Try to parse as JSON; if not, return as string
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                return Ok(parsed);
            }
            return Ok(serde_json::json!({"result": text}));
        }

        // Fallback: return raw result
        Ok(result)
    }

    fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools.clone()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_serialization() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "tools/list",
            params: None,
            id: 1,
        };
        let json = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "tools/list");
        assert_eq!(json["id"], 1);
        assert!(json.get("params").is_none());
    }

    #[test]
    fn test_json_rpc_request_with_params() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "tools/call",
            params: Some(serde_json::json!({"name": "search", "arguments": {"q": "hello"}})),
            id: 42,
        };
        let json = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["method"], "tools/call");
        assert_eq!(json["params"]["name"], "search");
        assert_eq!(json["id"], 42);
    }

    #[test]
    fn test_json_rpc_response_success() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"echo","description":"echoes input"}]}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_json_rpc_response_error() {
        let json = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_none());
        let err = resp.error.expect("error");
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }

    #[test]
    fn test_mcp_tool_def_parse() {
        let json = r#"{"name":"search","description":"Search the web","inputSchema":{"type":"object","properties":{"q":{"type":"string"}}}}"#;
        let tool: McpToolDef = serde_json::from_str(json).expect("parse");
        assert_eq!(tool.name, "search");
        assert_eq!(tool.description.as_deref(), Some("Search the web"));
        assert!(tool.input_schema.is_some());
    }

    #[test]
    fn test_mcp_tool_def_minimal() {
        let json = r#"{"name":"ping"}"#;
        let tool: McpToolDef = serde_json::from_str(json).expect("parse");
        assert_eq!(tool.name, "ping");
        assert!(tool.description.is_none());
        assert!(tool.input_schema.is_none());
    }

    #[test]
    fn test_mcp_tool_result_parse() {
        let json = r#"{"content":[{"type":"text","text":"hello world"}],"isError":false}"#;
        let result: McpToolResult = serde_json::from_str(json).expect("parse");
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].text.as_deref(), Some("hello world"));
        assert!(!result.is_error);
    }
}
