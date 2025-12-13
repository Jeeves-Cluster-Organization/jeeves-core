"""
Jeeves Gateway - HTTP API layer.

This package provides the HTTP-facing gateway that:
- Exposes REST endpoints for chat, kanban, journal, governance
- Provides SSE streaming for real-time events
- Acts as gRPC client to internal orchestrator service

The gateway knows nothing about agents, LLM, or database internals.
It only translates HTTP <-> gRPC.
"""

__version__ = "0.1.0"
