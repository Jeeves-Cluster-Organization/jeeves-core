"""MCP (Model Context Protocol) — Client and Server adapters.

Client: Consume external MCP tool servers, register tools in kernel.
Server: Expose jeeves tools as MCP-compatible endpoints.
"""

from .client import MCPClientAdapter
from .server import mcp_router

__all__ = ["MCPClientAdapter", "mcp_router"]
