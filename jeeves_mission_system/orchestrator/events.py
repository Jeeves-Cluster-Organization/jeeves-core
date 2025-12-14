"""
Unified Event Orchestrator - Single entry point for all event emission.

Phase 7 Constitutional Compliance:
- Mission System R7: Single Responsibility Per Component
- Consolidates event_context + agent_events + event_emitter into unified facade
- CommBus P3: Protocol-First Design

This module provides:
- EventOrchestrator: Unified facade combining all event systems
- All event-related types re-exported for convenience

Usage:
    from jeeves_mission_system.orchestrator.events import (
        EventOrchestrator,
        AgentEventType,
        AgentEvent,
    )

    orchestrator = EventOrchestrator(
        session_id="sess_123",
        request_id="req_456",
        user_id="user_789",
        event_repository=repo,
    )

    # Emit agent lifecycle events (real-time + audit)
    await orchestrator.emit_agent_started("planner")
    await orchestrator.emit_agent_completed("planner", status="success")

    # Emit tool events
    await orchestrator.emit_tool_started("grep_search", params={"query": "test"})
    await orchestrator.emit_tool_completed("grep_search", status="success")

    # Emit domain events (audit only)
    await orchestrator.emit_plan_created(plan_id="p123", intent="find code", ...)
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, TYPE_CHECKING, AsyncIterator

from jeeves_avionics.logging import get_current_logger

# Re-export all types from component modules
from jeeves_mission_system.orchestrator.agent_events import (
    AgentEventType,
    AgentEvent,
    AgentEventEmitter,
    create_agent_event_emitter,
)
from jeeves_mission_system.orchestrator.event_context import (
    AgentEventContext,
    create_event_context,
)

if TYPE_CHECKING:
    from jeeves_protocols import LoggerProtocol
    from jeeves_memory_module.repositories.event_repository import EventRepository
    from jeeves_avionics.gateway.event_bus import GatewayEventBus


@dataclass
class EventOrchestrator:
    """
    Unified orchestrator for all event operations.

    Provides single facade over:
    - AgentEventEmitter (real-time streaming via gRPC to gateway)
    - AgentEventContext (unified emission context)
    - EventEmitter (domain event persistence)

    This is the ONLY entry point for event emission in the mission system.

    Event Flow Architecture:
    - Orchestrator emits to AgentEventEmitter (in-memory queue)
    - Events streamed via gRPC to Gateway
    - Gateway converts to UnifiedEvent and broadcasts to WebSocket clients
    - NO direct gateway_event_bus injection (orchestrator and gateway are separate processes)

    Attributes:
        session_id: Current session identifier
        request_id: Current request identifier
        user_id: User identifier
        event_repository: Optional repository for domain event persistence
        enable_streaming: Whether to enable real-time event streaming
        enable_persistence: Whether to persist domain events
    """

    session_id: str
    request_id: str
    user_id: str
    event_repository: Optional["EventRepository"] = None
    enable_streaming: bool = True
    enable_persistence: bool = True
    correlation_id: Optional[str] = None
    _logger: Any = field(default=None, repr=False)

    # Internal state
    _agent_emitter: Optional[AgentEventEmitter] = field(default=None, repr=False)
    _domain_emitter: Optional[Any] = field(default=None, repr=False)
    _context: Optional[AgentEventContext] = field(default=None, repr=False)
    _initialized: bool = field(default=False, repr=False)

    def __post_init__(self):
        if self._logger is None:
            self._logger = get_current_logger()
        self._logger = self._logger.bind(
            component="EventOrchestrator",
            session_id=self.session_id,
            request_id=self.request_id,
        )
        if self.correlation_id is None:
            self.correlation_id = self.request_id

    def _ensure_initialized(self) -> None:
        """Lazy initialization of internal components."""
        if self._initialized:
            return

        # Create streaming emitter if enabled
        if self.enable_streaming:
            self._agent_emitter = create_agent_event_emitter()

        # Create domain emitter if enabled and repository provided
        if self.enable_persistence and self.event_repository:
            from jeeves_memory_module.services.event_emitter import EventEmitter
            self._domain_emitter = EventEmitter(
                event_repository=self.event_repository,
                logger=self._logger,
            )

        # Create unified context
        self._context = create_event_context(
            session_id=self.session_id,
            request_id=self.request_id,
            user_id=self.user_id,
            agent_event_emitter=self._agent_emitter,
            domain_event_emitter=self._domain_emitter,
            correlation_id=self.correlation_id,
        )

        self._initialized = True
        self._logger.debug("event_orchestrator_initialized")

    # =========================================================================
    # Delegated methods - Agent Lifecycle Events
    # =========================================================================

    async def emit_agent_started(
        self,
        agent_name: str,
        **payload,
    ) -> None:
        """
        Emit agent started event.

        Emission:
        1. Real-time queue (AgentEvent) → gRPC stream → Gateway
        2. Domain persistence (if enabled)

        Note: Gateway receives events via gRPC and converts to UnifiedEvent.
        No direct gateway_event_bus injection needed (cross-process).
        """
        self._ensure_initialized()
        await self._context.emit_agent_started(agent_name, **payload)

    async def emit_agent_completed(
        self,
        agent_name: str,
        status: str = "success",
        error: Optional[str] = None,
        **payload,
    ) -> None:
        """
        Emit agent completed event.

        Emission:
        1. Real-time queue (AgentEvent) → gRPC stream → Gateway
        2. Domain persistence (if enabled)

        Note: Gateway receives events via gRPC and converts to UnifiedEvent.
        No direct gateway_event_bus injection needed (cross-process).
        """
        self._ensure_initialized()
        await self._context.emit_agent_completed(
            agent_name, status=status, error=error, **payload
        )

    # =========================================================================
    # Delegated methods - Tool Events
    # =========================================================================

    async def emit_tool_started(
        self,
        tool_name: str,
        params: Optional[Dict[str, Any]] = None,
        step_number: Optional[int] = None,
        total_steps: Optional[int] = None,
        **payload,
    ) -> None:
        """Emit tool execution started event."""
        self._ensure_initialized()
        await self._context.emit_tool_started(
            tool_name=tool_name,
            params=params,
            step_number=step_number,
            total_steps=total_steps,
            **payload,
        )

    async def emit_tool_completed(
        self,
        tool_name: str,
        status: str = "success",
        execution_time_ms: Optional[int] = None,
        error: Optional[str] = None,
        error_type: Optional[str] = None,
    ) -> Optional[str]:
        """Emit tool execution completed event."""
        self._ensure_initialized()
        return await self._context.emit_tool_completed(
            tool_name=tool_name,
            status=status,
            execution_time_ms=execution_time_ms,
            error=error,
            error_type=error_type,
        )

    # =========================================================================
    # Delegated methods - Planner Events
    # =========================================================================

    async def emit_plan_created(
        self,
        plan_id: str,
        intent: str,
        confidence: float,
        tools: List[str],
        step_count: int,
        clarification_needed: bool = False,
    ) -> Optional[str]:
        """Emit plan created event (domain event for audit)."""
        self._ensure_initialized()
        return await self._context.emit_plan_created(
            plan_id=plan_id,
            intent=intent,
            confidence=confidence,
            tools=tools,
            step_count=step_count,
            clarification_needed=clarification_needed,
        )

    # =========================================================================
    # Delegated methods - Critic Events
    # =========================================================================

    async def emit_critic_decision(
        self,
        action: str,
        confidence: float,
        issue: Optional[str] = None,
        feedback: Optional[str] = None,
    ) -> Optional[str]:
        """Emit critic decision event (domain event for audit)."""
        self._ensure_initialized()
        return await self._context.emit_critic_decision(
            action=action,
            confidence=confidence,
            issue=issue,
            feedback=feedback,
        )

    # =========================================================================
    # Delegated methods - Flow Events
    # =========================================================================

    async def emit_flow_started(self) -> None:
        """Emit flow started event."""
        self._ensure_initialized()
        await self._context.emit_flow_started()

    async def emit_flow_completed(self, status: str = "success") -> None:
        """Emit flow completed event."""
        self._ensure_initialized()
        await self._context.emit_flow_completed(status=status)

    async def emit_flow_error(self, error: str) -> None:
        """Emit flow error event."""
        self._ensure_initialized()
        await self._context.emit_flow_error(error=error)

    # =========================================================================
    # Delegated methods - Stage Transition Events
    # =========================================================================

    async def emit_stage_transition(
        self,
        from_stage: int,
        to_stage: int,
        satisfied_goals: Optional[List[str]] = None,
        remaining_goals: Optional[List[str]] = None,
    ) -> None:
        """Emit stage transition event (multi-stage workflows)."""
        self._ensure_initialized()
        await self._context.emit_stage_transition(
            from_stage=from_stage,
            to_stage=to_stage,
            satisfied_goals=satisfied_goals,
            remaining_goals=remaining_goals,
        )

    # =========================================================================
    # Delegated methods - Retry Events
    # =========================================================================

    async def emit_retry_attempt(
        self,
        retry_type: str,
        attempt_number: int,
        reason: str,
        feedback: Optional[str] = None,
    ) -> Optional[str]:
        """Emit retry attempt event (domain event for audit)."""
        self._ensure_initialized()
        return await self._context.emit_retry_attempt(
            retry_type=retry_type,
            attempt_number=attempt_number,
            reason=reason,
            feedback=feedback,
        )

    # =========================================================================
    # Delegated methods - Response Events
    # =========================================================================

    async def emit_response_drafted(
        self,
        response_id: str,
        response_preview: str,
        validation_status: str = "pending",
    ) -> Optional[str]:
        """Emit response drafted event (domain event for audit)."""
        self._ensure_initialized()
        return await self._context.emit_response_drafted(
            response_id=response_id,
            response_preview=response_preview,
            validation_status=validation_status,
        )

    # =========================================================================
    # Delegated methods - Reasoning Events
    # =========================================================================

    async def emit_agent_reasoning(
        self,
        agent_name: str,
        reasoning_type: str,
        reasoning_text: str,
        confidence: Optional[float] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> None:
        """Emit agent reasoning event (config-gated)."""
        self._ensure_initialized()
        await self._context.emit_agent_reasoning(
            agent_name=agent_name,
            reasoning_type=reasoning_type,
            reasoning_text=reasoning_text,
            confidence=confidence,
            metadata=metadata,
        )

    # =========================================================================
    # Direct Domain Event Methods (for events without real-time component)
    # =========================================================================

    async def emit_task_created(
        self,
        task_id: str,
        title: str,
        description: Optional[str] = None,
        priority: Optional[int] = None,
    ) -> Optional[str]:
        """Emit task_created domain event."""
        self._ensure_initialized()
        if self._domain_emitter:
            return await self._domain_emitter.emit_task_created(
                task_id=task_id,
                user_id=self.user_id,
                title=title,
                description=description,
                priority=priority,
                session_id=self.session_id,
            )
        return None

    async def emit_task_completed(self, task_id: str) -> Optional[str]:
        """Emit task_completed domain event."""
        self._ensure_initialized()
        if self._domain_emitter:
            return await self._domain_emitter.emit_task_completed(
                task_id=task_id,
                user_id=self.user_id,
                session_id=self.session_id,
            )
        return None

    # =========================================================================
    # Streaming Access
    # =========================================================================

    async def events(self) -> AsyncIterator[AgentEvent]:
        """
        Iterate over emitted real-time events.

        Use this in gRPC servicer to stream events to frontend.

        Yields:
            AgentEvent instances as they are emitted
        """
        self._ensure_initialized()
        if self._agent_emitter:
            async for event in self._agent_emitter.events():
                yield event

    def get_event_nowait(self) -> Optional[AgentEvent]:
        """Get an event without waiting (for polling consumers)."""
        self._ensure_initialized()
        if self._agent_emitter:
            return self._agent_emitter.get_nowait()
        return None

    # =========================================================================
    # Lifecycle
    # =========================================================================

    async def close(self) -> None:
        """Close the event orchestrator and signal end of streaming."""
        if self._context:
            await self._context.close()
        self._logger.debug("event_orchestrator_closed")

    @property
    def context(self) -> Optional[AgentEventContext]:
        """Access to underlying context for advanced usage."""
        self._ensure_initialized()
        return self._context


def create_event_orchestrator(
    session_id: str,
    request_id: str,
    user_id: str,
    event_repository: Optional["EventRepository"] = None,
    enable_streaming: bool = True,
    enable_persistence: bool = True,
    correlation_id: Optional[str] = None,
) -> EventOrchestrator:
    """
    Factory function to create an EventOrchestrator.

    Events flow: Orchestrator → gRPC → Gateway → WebSocket
    No direct gateway injection (separate processes).

    Args:
        session_id: Current session identifier
        request_id: Current request identifier
        user_id: User identifier
        event_repository: Optional repository for domain event persistence
        enable_streaming: Whether to enable real-time event streaming
        enable_persistence: Whether to persist domain events
        correlation_id: Optional correlation ID for domain events

    Returns:
        Configured EventOrchestrator instance
    """
    return EventOrchestrator(
        session_id=session_id,
        request_id=request_id,
        user_id=user_id,
        event_repository=event_repository,
        enable_streaming=enable_streaming,
        enable_persistence=enable_persistence,
        correlation_id=correlation_id,
    )


__all__ = [
    # Unified orchestrator
    "EventOrchestrator",
    "create_event_orchestrator",
    # Types from agent_events
    "AgentEventType",
    "AgentEvent",
    "AgentEventEmitter",
    "create_agent_event_emitter",
    # Types from event_context
    "AgentEventContext",
    "create_event_context",
]
