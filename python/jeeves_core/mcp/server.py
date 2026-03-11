"""MCP Server — Expose jeeves tools as MCP-compatible endpoints.

JSON-RPC endpoint that exposes the kernel's tool catalog to external
MCP clients. Uses FastAPI router pattern for mounting in the gateway.

Endpoints:
    POST /mcp/ — JSON-RPC dispatch (initialize, tools/list, tools/call)

Reuses: ToolCatalog, ToolExecutionCore, KernelClient.list_tools(),
        gateway SSE (SSEStream, format_sse_event).
"""

from __future__ import annotations

import json
import logging
from typing import Any, Dict, List, Optional

from fastapi import APIRouter, Request
from fastapi.responses import JSONResponse

logger = logging.getLogger(__name__)

mcp_router = APIRouter()

# JSON-RPC constants
JSONRPC_VERSION = "2.0"
MCP_PROTOCOL_VERSION = "2024-11-05"


def _error_response(id: Any, code: int, message: str) -> Dict[str, Any]:
    """Build a JSON-RPC error response."""
    return {
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "error": {"code": code, "message": message},
    }


def _success_response(id: Any, result: Any) -> Dict[str, Any]:
    """Build a JSON-RPC success response."""
    return {
        "jsonrpc": JSONRPC_VERSION,
        "id": id,
        "result": result,
    }


def _tool_entry_to_mcp_schema(entry: Dict[str, Any]) -> Dict[str, Any]:
    """Convert a kernel ToolEntry dict to MCP tool schema.

    Reverse of MCPClientAdapter._json_schema_to_param_dict:
    ToolEntry params (Dict[str, str]) → JSON Schema.
    """
    params = entry.get("parameters", {})
    properties = {}
    required = []

    for name, type_desc in params.items():
        # Parse type description like "string: description (required)"
        is_required = "(required)" in type_desc
        clean_desc = type_desc.replace("(required)", "").strip()

        # Extract type prefix
        json_type = "string"
        description = clean_desc
        if ": " in clean_desc:
            type_part, desc_part = clean_desc.split(": ", 1)
            if type_part in ("string", "integer", "number", "boolean", "array", "object"):
                json_type = type_part
                description = desc_part
            else:
                description = clean_desc

        properties[name] = {"type": json_type, "description": description}

        if is_required:
            required.append(name)

    return {
        "name": entry.get("name", ""),
        "description": entry.get("description", ""),
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
        },
    }


@mcp_router.post("/")
async def mcp_jsonrpc(request: Request) -> JSONResponse:
    """MCP JSON-RPC endpoint — handles initialize, tools/list, tools/call."""
    try:
        body = await request.json()
    except Exception:
        return JSONResponse(
            _error_response(None, -32700, "Parse error"),
            status_code=400,
        )

    method = body.get("method", "")
    params = body.get("params", {})
    req_id = body.get("id")

    if method == "initialize":
        return JSONResponse(_success_response(req_id, {
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {"listChanged": False},
            },
            "serverInfo": {
                "name": "jeeves-core",
                "version": "0.0.1",
            },
        }))

    elif method == "notifications/initialized":
        # Acknowledgement — no response needed for notifications
        return JSONResponse(_success_response(req_id, {}))

    elif method == "tools/list":
        return await _handle_tools_list(request, req_id, params)

    elif method == "tools/call":
        return await _handle_tools_call(request, req_id, params)

    else:
        return JSONResponse(
            _error_response(req_id, -32601, f"Method not found: {method}"),
            status_code=400,
        )


async def _handle_tools_list(request: Request, req_id: Any, params: dict) -> JSONResponse:
    """Handle tools/list — return all tools from kernel."""
    kernel_client = getattr(request.app.state, "context", None)
    if kernel_client is None:
        return JSONResponse(_error_response(req_id, -32603, "Kernel not available"))

    kc = kernel_client.kernel_client if hasattr(kernel_client, "kernel_client") else kernel_client

    try:
        tools = await kc.list_tools()
    except Exception as e:
        logger.error("mcp_tools_list_error", extra={"error": str(e)})
        return JSONResponse(_error_response(req_id, -32603, f"Failed to list tools: {e}"))

    mcp_tools = [_tool_entry_to_mcp_schema(t) for t in tools]

    return JSONResponse(_success_response(req_id, {"tools": mcp_tools}))


async def _handle_tools_call(request: Request, req_id: Any, params: dict) -> JSONResponse:
    """Handle tools/call — execute a tool and return result."""
    tool_name = params.get("name")
    arguments = params.get("arguments", {})

    if not tool_name:
        return JSONResponse(_error_response(req_id, -32602, "Missing tool name"))

    # Get tool executor from app state
    ctx = getattr(request.app.state, "context", None)
    if ctx is None:
        return JSONResponse(_error_response(req_id, -32603, "Context not available"))

    tool_executor = getattr(ctx, "tool_executor", None)
    if tool_executor is None:
        return JSONResponse(_error_response(req_id, -32603, "Tool executor not available"))

    try:
        result = await tool_executor.execute(tool_name, arguments)

        # Wrap in MCP content blocks
        content = []
        if isinstance(result, dict):
            data = result.get("data")
            if isinstance(data, str):
                content.append({"type": "text", "text": data})
            elif isinstance(data, dict):
                content.append({"type": "text", "text": json.dumps(data)})
            else:
                content.append({"type": "text", "text": json.dumps(result)})
        else:
            content.append({"type": "text", "text": str(result)})

        is_error = isinstance(result, dict) and result.get("status") == "error"

        return JSONResponse(_success_response(req_id, {
            "content": content,
            "isError": is_error,
        }))

    except Exception as e:
        logger.error("mcp_tool_call_error", extra={"tool": tool_name, "error": str(e)})
        return JSONResponse(_success_response(req_id, {
            "content": [{"type": "text", "text": f"Error: {e}"}],
            "isError": True,
        }))
