"""
Unified Event Schema - Constitutional Event Standard

This module defines the canonical event schema that all Jeeves event systems
must implement. This ensures standardization across:
- AgentEvent (real-time streaming)
- DomainEvent (audit/persistence)
- GatewayEvent (internal pub/sub)

Constitutional Authority:
- Avionics CONSTITUTION.md R2 (Configuration Over Code)
- Avionics CONSTITUTION.md R3 (No Domain Logic)
- Layer L0 principle (Pure types, zero dependencies)

Version: 1.0
Created: 2025-12-14
"""

from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, Any, Optional, Callable, Awaitable, Protocol
import uuid


class EventCategory(Enum):
    """Canonical event categories for classification and filtering."""

    AGENT_LIFECYCLE = "agent_lifecycle"      # Agent start/complete
    TOOL_EXECUTION = "tool_execution"        # Tool start/complete
    PIPELINE_FLOW = "pipeline_flow"          # Flow start/complete/error
    CRITIC_DECISION = "critic_decision"      # Critic verdicts
    STAGE_TRANSITION = "stage_transition"    # Multi-stage progress
    DOMAIN_EVENT = "domain_event"            # Business events (task, session, etc.)
    SESSION_EVENT = "session_event"          # Session lifecycle
    WORKFLOW_EVENT = "workflow_event"        # Workflow operations


class EventSeverity(Enum):
    """Event severity for filtering and alerting."""

    DEBUG = "debug"
    INFO = "info"
    WARNING = "warning"
    ERROR = "error"
    CRITICAL = "critical"


@dataclass
class Event:
    """
    CONSTITUTIONAL EVENT SCHEMA

    All event systems MUST emit this format after conversion.
    This schema provides:
    - Standardized timestamps (ISO + epoch milliseconds)
    - Correlation tracking (request_id, session_id, correlation_id)
    - Versioning support (version field)
    - Category-based routing (category enum)
    - Severity-based filtering (severity enum)

    Usage:
        event = Event(
            event_id=str(uuid.uuid4()),
            event_type="agent.started",
            category=EventCategory.AGENT_LIFECYCLE,
            timestamp_iso=datetime.now(timezone.utc).isoformat(),
            timestamp_ms=int(datetime.now(timezone.utc).timestamp() * 1000),
            request_id="req_123",
            session_id="sess_456",
            payload={"agent": "planner"},
        )
    """

    # Identity
    event_id: str                              # UUID v4
    event_type: str                            # Namespaced: "agent.started", "critic.decision"
    category: EventCategory                    # Broad classification

    # Timing (STANDARDIZED - solves RC-2)
    timestamp_iso: str                         # ISO 8601 format (UTC) e.g. "2025-12-14T01:58:26Z"
    timestamp_ms: int                          # Epoch milliseconds for easy ordering

    # Context (REQUIRED for correlation)
    request_id: str                            # Request correlation ID
    session_id: str                            # Session correlation ID
    user_id: Optional[str] = None              # User (for multi-tenancy)

    # Payload
    payload: Dict[str, Any] = field(default_factory=dict)  # Event-specific data

    # Metadata
    severity: EventSeverity = EventSeverity.INFO
    source: str = "unknown"                    # Component that emitted event
    version: str = "1.0"                       # Event schema version

    # Audit
    correlation_id: Optional[str] = None       # Causation chain (for event sourcing)

    def to_dict(self) -> Dict[str, Any]:
        """
        Convert to JSON-serializable dict.

        Returns:
            Dictionary with all fields, enums converted to values.
        """
        return {
            "event_id": self.event_id,
            "event_type": self.event_type,
            "category": self.category.value,
            "timestamp_iso": self.timestamp_iso,
            "timestamp_ms": self.timestamp_ms,
            "request_id": self.request_id,
            "session_id": self.session_id,
            "user_id": self.user_id,
            "payload": self.payload,
            "severity": self.severity.value,
            "source": self.source,
            "version": self.version,
            "correlation_id": self.correlation_id,
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Event":
        """
        Parse from dict.

        Args:
            data: Dictionary with event fields

        Returns:
            Event instance
        """
        return cls(
            event_id=data["event_id"],
            event_type=data["event_type"],
            category=EventCategory(data["category"]),
            timestamp_iso=data["timestamp_iso"],
            timestamp_ms=data["timestamp_ms"],
            request_id=data["request_id"],
            session_id=data["session_id"],
            user_id=data.get("user_id"),
            payload=data.get("payload", {}),
            severity=EventSeverity(data.get("severity", "info")),
            source=data.get("source", "unknown"),
            version=data.get("version", "1.0"),
            correlation_id=data.get("correlation_id"),
        )

    @classmethod
    def create_now(
        cls,
        event_type: str,
        category: EventCategory,
        request_id: str,
        session_id: str,
        payload: Optional[Dict[str, Any]] = None,
        user_id: Optional[str] = None,
        severity: EventSeverity = EventSeverity.INFO,
        source: str = "unknown",
        correlation_id: Optional[str] = None,
    ) -> "Event":
        """
        Factory method to create event with current timestamp.

        Args:
            event_type: Namespaced event type (e.g., "agent.started")
            category: Event category enum
            request_id: Request correlation ID
            session_id: Session correlation ID
            payload: Event-specific data
            user_id: Optional user ID
            severity: Event severity
            source: Component that emitted the event
            correlation_id: Optional causation chain ID

        Returns:
            Event with auto-generated ID and current timestamp
        """
        now = datetime.now(timezone.utc)
        return cls(
            event_id=str(uuid.uuid4()),
            event_type=event_type,
            category=category,
            timestamp_iso=now.isoformat(),
            timestamp_ms=int(now.timestamp() * 1000),
            request_id=request_id,
            session_id=session_id,
            user_id=user_id,
            payload=payload or {},
            severity=severity,
            source=source,
            version="1.0",
            correlation_id=correlation_id,
        )


class EventEmitterProtocol(Protocol):
    """
    Protocol all event emitters must implement.

    This protocol ensures all event systems can be swapped without
    changing consumer code (Avionics R4: Swappable Implementations).
    """

    async def emit(self, event: Event) -> None:
        """
        Emit a unified event.

        Args:
            event: Event to emit
        """
        ...

    async def subscribe(
        self,
        pattern: str,
        handler: Callable[[Event], Awaitable[None]],
    ) -> str:
        """
        Subscribe to events matching pattern.

        Args:
            pattern: Event type pattern (supports wildcards like "agent.*")
            handler: Async callback that receives Event

        Returns:
            Subscription ID for later unsubscription
        """
        ...

    async def unsubscribe(self, subscription_id: str) -> None:
        """
        Unsubscribe from events.

        Args:
            subscription_id: ID returned from subscribe()
        """
        ...


# Standard event type constants for common events
class StandardEventTypes:
    """Standard event type strings for consistency."""

    # Agent lifecycle
    AGENT_STARTED = "agent.started"
    AGENT_COMPLETED = "agent.completed"
    AGENT_FAILED = "agent.failed"

    # Pipeline stages
    PERCEPTION_STARTED = "perception.started"
    PERCEPTION_COMPLETED = "perception.completed"
    INTENT_STARTED = "intent.started"
    INTENT_COMPLETED = "intent.completed"
    PLANNER_STARTED = "planner.started"
    PLANNER_COMPLETED = "planner.plan_created"
    EXECUTOR_STARTED = "executor.started"
    EXECUTOR_COMPLETED = "executor.completed"
    SYNTHESIZER_STARTED = "synthesizer.started"
    SYNTHESIZER_COMPLETED = "synthesizer.completed"
    CRITIC_STARTED = "critic.started"
    CRITIC_DECISION = "critic.decision"
    INTEGRATION_STARTED = "integration.started"
    INTEGRATION_COMPLETED = "integration.completed"

    # Tool execution
    TOOL_STARTED = "executor.tool_started"
    TOOL_COMPLETED = "executor.tool_completed"
    TOOL_FAILED = "executor.tool_failed"

    # Pipeline flow
    FLOW_STARTED = "orchestrator.started"
    FLOW_COMPLETED = "orchestrator.completed"
    FLOW_ERROR = "orchestrator.error"
    STAGE_TRANSITION = "orchestrator.stage_transition"

    # Session events
    SESSION_CREATED = "session.created"
    SESSION_UPDATED = "session.updated"
    SESSION_DELETED = "session.deleted"

    # Message events
    MESSAGE_CREATED = "message.created"
    MESSAGE_UPDATED = "message.updated"
