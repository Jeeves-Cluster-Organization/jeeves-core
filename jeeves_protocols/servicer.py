"""Capability Servicer Protocol.

Constitutional Reference:
- Avionics R3: No Domain Logic - infrastructure provides transport, not business logic
- Avionics R4: Swappable Implementations - capabilities register their own resources

This module defines the protocol that capability servicers must implement
to be invokable via the mission system's FlowServicer. This enables
type-safe, capability-agnostic orchestration.

Usage:
    # In capability layer (e.g., jeeves-capability-code-analyser)
    from jeeves_protocols.servicer import CapabilityServicerProtocol

    class CodeAnalysisServicer:
        async def process_request(
            self,
            user_id: str,
            session_id: Optional[str],
            message: str,
            context: Optional[dict],
        ) -> AsyncIterator[Any]:
            # Implementation
            yield event

    # Type checker verifies CodeAnalysisServicer implements CapabilityServicerProtocol

    # In infrastructure (mission_system)
    from jeeves_protocols.servicer import CapabilityServicerProtocol

    class FlowServicer:
        def __init__(self, capability_servicer: CapabilityServicerProtocol):
            self._servicer = capability_servicer
"""

from typing import Protocol, AsyncIterator, Any, Optional, runtime_checkable


@runtime_checkable
class CapabilityServicerProtocol(Protocol):
    """Protocol for capability orchestrator servicers.

    Any capability that wants to be invoked via FlowServicer must implement this.
    The protocol defines the minimal interface required for the mission system
    to delegate requests to a capability without knowing its implementation details.

    This enables:
    - Type safety for capability implementations
    - IDE autocomplete and error checking
    - Documentation of required interface
    - No coupling to specific capability (e.g., code_analysis)

    Example implementation:
        class MyCapabilityServicer:
            async def process_request(
                self,
                user_id: str,
                session_id: Optional[str],
                message: str,
                context: Optional[dict],
            ) -> AsyncIterator[dict]:
                # Process the request
                yield {"type": "started", "session_id": session_id}
                # ... do work ...
                yield {"type": "completed", "result": result}
    """

    async def process_request(
        self,
        user_id: str,
        session_id: Optional[str],
        message: str,
        context: Optional[dict],
    ) -> AsyncIterator[Any]:
        """Process a capability request and yield events.

        This is the main entry point for capability invocation. The mission
        system calls this method and streams the yielded events to clients.

        Args:
            user_id: Unique identifier for the user making the request.
            session_id: Optional session identifier for conversation continuity.
                       If None, the capability may create a new session.
            message: The user's message/query to process.
            context: Optional context dictionary with additional metadata.
                    May include repo_path, preferences, prior context, etc.

        Yields:
            Flow events in implementation-specific format. Common patterns:
            - {"type": "started", ...} - Request processing began
            - {"type": "agent.started", "agent": "...", ...} - Agent began work
            - {"type": "agent.completed", "agent": "...", ...} - Agent finished
            - {"type": "tool.called", "tool": "...", ...} - Tool invocation
            - {"type": "completed", "result": ..., ...} - Final result

        Raises:
            Any exceptions should be caught and yielded as error events,
            or allowed to propagate for infrastructure error handling.
        """
        ...  # Protocol method - implementation required
        # This yield is needed to make the type checker recognize this as an async generator
        yield  # type: ignore


__all__ = [
    "CapabilityServicerProtocol",
]
