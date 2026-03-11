"""MCP Client Adapter — Consume external MCP tool servers.

Discovers tools from MCP servers, registers them in the kernel,
and proxies tool calls transparently through ToolExecutionCore.

Temporal coherence: MCP tools are opaque tool executor backends.
Agent runs MCP tool → result in envelope.outputs → kernel evaluates routing.
No kernel changes needed.

Usage:
    adapter = MCPClientAdapter(
        server_name="filesystem",
        command="npx",
        args=["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
    )
    await adapter.connect_and_discover()
    result = await adapter.execute_tool("read_file", {"path": "/tmp/foo.txt"})
"""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

logger = logging.getLogger(__name__)


@dataclass
class MCPServerConfig:
    """Configuration for an MCP server connection."""
    name: str
    command: Optional[str] = None  # For stdio transport
    args: List[str] = field(default_factory=list)
    env: Dict[str, str] = field(default_factory=dict)
    url: Optional[str] = None  # For HTTP+SSE transport
    transport: str = "stdio"  # "stdio" or "sse"


@dataclass
class MCPToolInfo:
    """Tool metadata discovered from an MCP server."""
    name: str
    description: str
    input_schema: Dict[str, Any]
    server_name: str


def _json_schema_to_param_dict(schema: Dict[str, Any]) -> Dict[str, str]:
    """Convert JSON Schema properties to ToolCatalogEntry-style Dict[str, str].

    Maps JSON Schema types to simple type description strings used by
    ToolCatalogEntry.parameters.
    """
    properties = schema.get("properties", {})
    required = set(schema.get("required", []))
    params: Dict[str, str] = {}

    for name, prop in properties.items():
        json_type = prop.get("type", "string")
        desc = prop.get("description", "")
        suffix = " (required)" if name in required else ""
        params[name] = f"{json_type}: {desc}{suffix}" if desc else f"{json_type}{suffix}"

    return params


class MCPClientAdapter:
    """Adapter that connects to an MCP server and proxies tool calls.

    Lifecycle:
    1. connect_and_discover() — initialize MCP session, list tools
    2. execute_tool(name, params) — proxy tool call to MCP server
    3. close() — shut down MCP session

    Tools are registered in the kernel via kernel_client after discovery.
    """

    def __init__(
        self,
        server_name: str,
        command: Optional[str] = None,
        args: Optional[List[str]] = None,
        env: Optional[Dict[str, str]] = None,
        url: Optional[str] = None,
        transport: str = "stdio",
    ):
        self._config = MCPServerConfig(
            name=server_name,
            command=command,
            args=args or [],
            env=env or {},
            url=url,
            transport=transport,
        )
        self._session = None
        self._tools: Dict[str, MCPToolInfo] = {}
        self._connected = False

    @property
    def server_name(self) -> str:
        return self._config.name

    @property
    def tools(self) -> Dict[str, MCPToolInfo]:
        return dict(self._tools)

    async def connect_and_discover(self) -> List[MCPToolInfo]:
        """Connect to MCP server, initialize session, and discover tools.

        Returns list of discovered MCPToolInfo.
        Requires: pip install jeeves-core[mcp]
        """
        try:
            from mcp import ClientSession, StdioServerParameters
            from mcp.client.stdio import stdio_client
        except ImportError:
            raise ImportError(
                "MCP SDK required: pip install jeeves-core[mcp] or pip install mcp"
            )

        if self._config.transport == "stdio":
            if not self._config.command:
                raise ValueError(f"MCP stdio transport requires 'command' for server '{self._config.name}'")

            server_params = StdioServerParameters(
                command=self._config.command,
                args=self._config.args,
                env=self._config.env or None,
            )

            # Connect via stdio
            self._stdio_ctx = stdio_client(server_params)
            streams = await self._stdio_ctx.__aenter__()
            read_stream, write_stream = streams

            self._session_ctx = ClientSession(read_stream, write_stream)
            self._session = await self._session_ctx.__aenter__()

            # Initialize
            await self._session.initialize()

        elif self._config.transport == "sse":
            if not self._config.url:
                raise ValueError(f"MCP SSE transport requires 'url' for server '{self._config.name}'")

            from mcp.client.sse import sse_client

            self._sse_ctx = sse_client(self._config.url)
            streams = await self._sse_ctx.__aenter__()
            read_stream, write_stream = streams

            self._session_ctx = ClientSession(read_stream, write_stream)
            self._session = await self._session_ctx.__aenter__()

            await self._session.initialize()
        else:
            raise ValueError(f"Unsupported MCP transport: {self._config.transport}")

        # Discover tools
        tools_result = await self._session.list_tools()

        discovered = []
        for tool in tools_result.tools:
            info = MCPToolInfo(
                name=tool.name,
                description=tool.description or "",
                input_schema=tool.inputSchema if hasattr(tool, "inputSchema") else {},
                server_name=self._config.name,
            )
            self._tools[tool.name] = info
            discovered.append(info)

        self._connected = True
        logger.info(
            "mcp_server_connected",
            extra={
                "server": self._config.name,
                "tools_discovered": len(discovered),
                "tool_names": [t.name for t in discovered],
            },
        )

        return discovered

    async def execute_tool(self, tool_name: str, params: Dict[str, Any]) -> Dict[str, Any]:
        """Proxy a tool call to the MCP server.

        Args:
            tool_name: Name of the tool to call
            params: Tool parameters

        Returns:
            {"status": "success"|"error", "data": ..., "content": [...]}
        """
        if not self._session or not self._connected:
            raise RuntimeError(f"MCP client not connected to '{self._config.name}'")

        if tool_name not in self._tools:
            raise ValueError(
                f"Tool '{tool_name}' not found on MCP server '{self._config.name}'. "
                f"Available: {list(self._tools.keys())}"
            )

        try:
            result = await self._session.call_tool(tool_name, params)

            # Parse MCP content blocks into a flat result
            content_items = []
            text_parts = []
            for block in result.content:
                if hasattr(block, "text"):
                    content_items.append({"type": "text", "text": block.text})
                    text_parts.append(block.text)
                elif hasattr(block, "data"):
                    content_items.append({"type": "resource", "data": block.data})

            is_error = getattr(result, "isError", False)

            return {
                "status": "error" if is_error else "success",
                "data": "\n".join(text_parts) if text_parts else None,
                "content": content_items,
            }

        except Exception as e:
            logger.error(
                "mcp_tool_call_error",
                extra={"server": self._config.name, "tool": tool_name, "error": str(e)},
            )
            return {"status": "error", "error": str(e)}

    async def register_tools_in_kernel(self, kernel_client) -> int:
        """Register all discovered tools in the Rust kernel.

        Args:
            kernel_client: KernelClient instance

        Returns:
            Number of tools registered
        """
        count = 0
        for tool_name, info in self._tools.items():
            prefixed_name = f"mcp_{self._config.name}_{tool_name}"
            params = _json_schema_to_param_dict(info.input_schema)

            tool_entry = {
                "name": prefixed_name,
                "description": f"[MCP:{self._config.name}] {info.description}",
                "parameters": params,
                "category": "mcp",
                "metadata": {
                    "mcp_server": self._config.name,
                    "mcp_tool": tool_name,
                    "input_schema": info.input_schema,
                },
            }

            await kernel_client.register_tool(tool_entry)
            count += 1

        # Register MCP server as a service
        await kernel_client.register_service(
            name=f"mcp_{self._config.name}",
            service_type="tool_server",
            metadata={
                "transport": self._config.transport,
                "tool_count": len(self._tools),
                "tool_names": list(self._tools.keys()),
            },
        )

        logger.info(
            "mcp_tools_registered_in_kernel",
            extra={"server": self._config.name, "count": count},
        )
        return count

    async def close(self):
        """Shut down the MCP session."""
        if self._session:
            try:
                await self._session_ctx.__aexit__(None, None, None)
            except Exception:
                pass

        if hasattr(self, "_stdio_ctx"):
            try:
                await self._stdio_ctx.__aexit__(None, None, None)
            except Exception:
                pass

        if hasattr(self, "_sse_ctx"):
            try:
                await self._sse_ctx.__aexit__(None, None, None)
            except Exception:
                pass

        self._session = None
        self._connected = False
        logger.info("mcp_client_closed", extra={"server": self._config.name})

    def get_tool_schemas(self) -> List[Dict[str, Any]]:
        """Get OpenAI-format tool schemas for all discovered tools.

        Useful for passing to LLM providers via chat() options.tools.
        """
        schemas = []
        for name, info in self._tools.items():
            schemas.append({
                "type": "function",
                "function": {
                    "name": f"mcp_{self._config.name}_{name}",
                    "description": info.description,
                    "parameters": info.input_schema,
                },
            })
        return schemas
