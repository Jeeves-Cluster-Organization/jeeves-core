"""
Event Bridge Adapters - Convert Legacy Events to UnifiedEvent

This module provides adapters that convert legacy event systems to the
constitutional UnifiedEvent schema defined in jeeves_protocols.events.

Adapters:
- AgentEventAdapter: AgentEvent → UnifiedEvent
- DomainEventAdapter: DomainEvent → UnifiedEvent (future)

Constitutional Authority:
- Avionics CONSTITUTION.md R4 (Swappable Implementations)
- Mission System event standardization requirement

Version: 1.0
Created: 2025-12-14
"""

from datetime import datetime, timezone
import uuid

from jeeves_protocols.events import (
    UnifiedEvent,
    EventCategory,
    EventSeverity,
    StandardEventTypes,
)
from jeeves_mission_system.orchestrator.agent_events import AgentEvent, AgentEventType


class AgentEventAdapter:
    """
    Converts AgentEvent (legacy) to UnifiedEvent (constitutional).

    This adapter ensures backward compatibility while migrating to the
    unified event schema. All AgentEvents are converted to UnifiedEvents
    before being emitted to the gateway event bus.
    """

    EVENT_TYPE_MAP = {
        AgentEventType.AGENT_STARTED: StandardEventTypes.AGENT_STARTED,
        AgentEventType.AGENT_COMPLETED: StandardEventTypes.AGENT_COMPLETED,
        AgentEventType.PERCEPTION_STARTED: StandardEventTypes.PERCEPTION_STARTED,
        AgentEventType.PERCEPTION_COMPLETED: StandardEventTypes.PERCEPTION_COMPLETED,
        AgentEventType.INTENT_STARTED: StandardEventTypes.INTENT_STARTED,
        AgentEventType.INTENT_COMPLETED: StandardEventTypes.INTENT_COMPLETED,
        AgentEventType.PLANNER_STARTED: StandardEventTypes.PLANNER_STARTED,
        AgentEventType.PLANNER_COMPLETED: StandardEventTypes.PLANNER_COMPLETED,
        AgentEventType.TRAVERSER_STARTED: StandardEventTypes.EXECUTOR_STARTED,
        AgentEventType.TRAVERSER_COMPLETED: StandardEventTypes.EXECUTOR_COMPLETED,
        AgentEventType.TOOL_STARTED: StandardEventTypes.TOOL_STARTED,
        AgentEventType.TOOL_COMPLETED: StandardEventTypes.TOOL_COMPLETED,
        AgentEventType.SYNTHESIZER_STARTED: StandardEventTypes.SYNTHESIZER_STARTED,
        AgentEventType.SYNTHESIZER_COMPLETED: StandardEventTypes.SYNTHESIZER_COMPLETED,
        AgentEventType.CRITIC_STARTED: StandardEventTypes.CRITIC_STARTED,
        AgentEventType.CRITIC_DECISION: StandardEventTypes.CRITIC_DECISION,
        AgentEventType.INTEGRATION_STARTED: StandardEventTypes.INTEGRATION_STARTED,
        AgentEventType.INTEGRATION_COMPLETED: StandardEventTypes.INTEGRATION_COMPLETED,
        AgentEventType.STAGE_TRANSITION: StandardEventTypes.STAGE_TRANSITION,
        AgentEventType.FLOW_STARTED: StandardEventTypes.FLOW_STARTED,
        AgentEventType.FLOW_COMPLETED: StandardEventTypes.FLOW_COMPLETED,
        AgentEventType.FLOW_ERROR: StandardEventTypes.FLOW_ERROR,
    }

    @classmethod
    def to_unified(cls, agent_event: AgentEvent) -> UnifiedEvent:
        """
        Convert AgentEvent to UnifiedEvent.

        Args:
            agent_event: Legacy AgentEvent instance

        Returns:
            UnifiedEvent with standardized schema
        """
        # Convert timestamp_ms to ISO string
        dt = datetime.fromtimestamp(agent_event.timestamp_ms / 1000, tz=timezone.utc)

        # Map event type
        event_type = cls.EVENT_TYPE_MAP.get(
            agent_event.event_type,
            "agent.unknown"
        )

        # Determine category
        category = cls._get_category(agent_event.event_type)

        # Determine severity
        severity = EventSeverity.ERROR if agent_event.event_type == AgentEventType.FLOW_ERROR else EventSeverity.INFO

        # Build payload (include agent name and original payload)
        payload = {
            "agent": agent_event.agent_name,
            **agent_event.payload,
        }

        return UnifiedEvent(
            event_id=str(uuid.uuid4()),
            event_type=event_type,
            category=category,
            timestamp_iso=dt.isoformat(),
            timestamp_ms=agent_event.timestamp_ms,
            request_id=agent_event.request_id,
            session_id=agent_event.session_id,
            user_id=None,  # AgentEvent doesn't have user_id
            payload=payload,
            severity=severity,
            source="agent_emitter",
            version="1.0",
            correlation_id=agent_event.request_id,
        )

    @classmethod
    def _get_category(cls, event_type: AgentEventType) -> EventCategory:
        """
        Map AgentEventType to EventCategory.

        Args:
            event_type: Legacy AgentEventType enum

        Returns:
            Appropriate EventCategory
        """
        if event_type in [AgentEventType.TOOL_STARTED, AgentEventType.TOOL_COMPLETED]:
            return EventCategory.TOOL_EXECUTION
        elif event_type == AgentEventType.CRITIC_DECISION:
            return EventCategory.CRITIC_DECISION
        elif event_type == AgentEventType.STAGE_TRANSITION:
            return EventCategory.STAGE_TRANSITION
        elif event_type in [
            AgentEventType.FLOW_STARTED,
            AgentEventType.FLOW_COMPLETED,
            AgentEventType.FLOW_ERROR,
        ]:
            return EventCategory.PIPELINE_FLOW
        else:
            return EventCategory.AGENT_LIFECYCLE


# Future: DomainEventAdapter for converting DomainEvent → UnifiedEvent
# This will be implemented when we migrate the audit/persistence layer
