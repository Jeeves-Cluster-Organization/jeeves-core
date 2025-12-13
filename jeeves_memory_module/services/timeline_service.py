"""
Timeline service for unified event and trace queries.

Provides:
- Combined view of domain events and agent traces
- Request lifecycle reconstruction
- Time-range queries
- Aggregate-level history
"""

from typing import List, Dict, Any, Optional, Union
from datetime import datetime, timezone
from dataclasses import dataclass
from enum import Enum

from jeeves_memory_module.repositories.event_repository import EventRepository, DomainEvent
from jeeves_memory_module.repositories.trace_repository import TraceRepository, AgentTrace
from jeeves_shared import get_component_logger
from jeeves_protocols import LoggerProtocol



class TimelineEntryType(str, Enum):
    """Types of timeline entries."""
    DOMAIN_EVENT = "domain_event"
    AGENT_TRACE = "agent_trace"


@dataclass
class TimelineEntry:
    """
    Unified timeline entry combining events and traces.

    Per Memory Contract: Timeline view reconstructs request lifecycle.
    """
    entry_id: str
    entry_type: TimelineEntryType
    timestamp: datetime
    label: str
    correlation_id: Optional[str]
    user_id: str
    details: Dict[str, Any]

    # Event-specific fields
    aggregate_type: Optional[str] = None
    aggregate_id: Optional[str] = None
    event_type: Optional[str] = None

    # Trace-specific fields
    agent_name: Optional[str] = None
    stage: Optional[str] = None
    status: Optional[str] = None
    latency_ms: Optional[int] = None
    confidence: Optional[float] = None

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "entry_id": self.entry_id,
            "entry_type": self.entry_type.value,
            "timestamp": self.timestamp.isoformat() if self.timestamp else None,
            "label": self.label,
            "correlation_id": self.correlation_id,
            "user_id": self.user_id,
            "details": self.details,
            "aggregate_type": self.aggregate_type,
            "aggregate_id": self.aggregate_id,
            "event_type": self.event_type,
            "agent_name": self.agent_name,
            "stage": self.stage,
            "status": self.status,
            "latency_ms": self.latency_ms,
            "confidence": self.confidence
        }


class TimelineService:
    """
    Service for unified timeline queries.

    Per Constitution P6: All operations logged and traceable.
    Per Memory Contract L2: Event log + traces provide complete audit trail.
    """

    def __init__(
        self,
        event_repository: EventRepository,
        trace_repository: TraceRepository,
        logger: Optional[LoggerProtocol] = None
    ):
        """
        Initialize timeline service.

        Args:
            event_repository: Repository for domain events
            trace_repository: Repository for agent traces

            logger: Optional logger instance (ADR-001 DI)
        """
        self.events = event_repository
        self.traces = trace_repository
        self._logger = get_component_logger("timeline_service", logger)

    async def get_request_timeline(
        self,
        correlation_id: str,
        include_events: bool = True,
        include_traces: bool = True
    ) -> List[TimelineEntry]:
        """
        Get complete timeline for a request.

        Args:
            correlation_id: Correlation ID
            include_events: Include domain events
            include_traces: Include agent traces

        Returns:
            List of timeline entries ordered by timestamp
        """
        entries: List[TimelineEntry] = []

        # Fetch events
        if include_events:
            try:
                events = await self.events.get_by_correlation(correlation_id)
                for event in events:
                    entries.append(self._event_to_entry(event))
            except Exception as e:
                self._logger.error(
                    "timeline_events_failed",
                    error=str(e),
                    correlation_id=correlation_id
                )

        # Fetch traces
        if include_traces:
            try:
                traces = await self.traces.get_by_correlation(correlation_id)
                for trace in traces:
                    entries.append(self._trace_to_entry(trace))
            except Exception as e:
                self._logger.error(
                    "timeline_traces_failed",
                    error=str(e),
                    correlation_id=correlation_id
                )

        # Sort by timestamp
        entries.sort(key=lambda e: e.timestamp)

        self._logger.debug(
            "request_timeline_fetched",
            correlation_id=correlation_id,
            entry_count=len(entries)
        )

        return entries

    async def get_entity_history(
        self,
        aggregate_type: str,
        aggregate_id: str,
        since: Optional[datetime] = None,
        limit: int = 100
    ) -> List[TimelineEntry]:
        """
        Get history for a specific entity (e.g., a task's full history).

        Args:
            aggregate_type: Type of aggregate
            aggregate_id: Aggregate ID
            since: Only events after this timestamp
            limit: Maximum entries to return

        Returns:
            List of timeline entries for the entity
        """
        entries: List[TimelineEntry] = []

        try:
            events = await self.events.get_by_aggregate(
                aggregate_type=aggregate_type,
                aggregate_id=aggregate_id,
                since=since,
                limit=limit
            )
            for event in events:
                entries.append(self._event_to_entry(event))

        except Exception as e:
            self._logger.error(
                "entity_history_failed",
                error=str(e),
                aggregate_type=aggregate_type,
                aggregate_id=aggregate_id
            )

        return entries

    async def get_recent_activity(
        self,
        user_id: str,
        limit: int = 50,
        aggregate_types: Optional[List[str]] = None,
        include_traces: bool = False
    ) -> List[TimelineEntry]:
        """
        Get recent activity for a user.

        Args:
            user_id: User ID
            limit: Maximum entries to return
            aggregate_types: Optional filter by aggregate types
            include_traces: Include agent traces

        Returns:
            List of timeline entries (most recent first)
        """
        entries: List[TimelineEntry] = []

        # Fetch recent events
        try:
            events = await self.events.get_recent(
                user_id=user_id,
                limit=limit,
                aggregate_types=aggregate_types
            )
            for event in events:
                entries.append(self._event_to_entry(event))
        except Exception as e:
            self._logger.error(
                "recent_events_failed",
                error=str(e),
                user_id=user_id
            )

        # Optionally fetch recent traces
        if include_traces:
            try:
                # Get traces for all agents
                for agent_name in ["planner", "executor", "validator", "critic", "meta_validator"]:
                    traces = await self.traces.get_by_agent(
                        agent_name=agent_name,
                        user_id=user_id,
                        limit=limit // 5  # Divide limit across agents
                    )
                    for trace in traces:
                        entries.append(self._trace_to_entry(trace))
            except Exception as e:
                self._logger.error(
                    "recent_traces_failed",
                    error=str(e),
                    user_id=user_id
                )

        # Sort by timestamp (most recent first)
        entries.sort(key=lambda e: e.timestamp, reverse=True)

        # Apply limit
        return entries[:limit]

    async def get_agent_decision_chain(
        self,
        request_id: str
    ) -> List[TimelineEntry]:
        """
        Get the decision chain for a single request (traces only).

        Useful for debugging: shows how each agent processed the request.

        Args:
            request_id: Request ID

        Returns:
            List of trace entries ordered by started_at
        """
        entries: List[TimelineEntry] = []

        try:
            traces = await self.traces.get_by_request(request_id)
            for trace in traces:
                entries.append(self._trace_to_entry(trace))
        except Exception as e:
            self._logger.error(
                "decision_chain_failed",
                error=str(e),
                request_id=request_id
            )

        return entries

    async def get_state_changes(
        self,
        user_id: str,
        event_types: Optional[List[str]] = None,
        since: Optional[datetime] = None,
        limit: int = 100
    ) -> List[TimelineEntry]:
        """
        Get state change events only (no traces).

        Args:
            user_id: User ID
            event_types: Optional filter by event types
            since: Only events after this timestamp
            limit: Maximum entries to return

        Returns:
            List of event entries
        """
        entries: List[TimelineEntry] = []

        try:
            if event_types:
                # Fetch for each event type
                for event_type in event_types:
                    events = await self.events.get_by_type(
                        event_type=event_type,
                        user_id=user_id,
                        since=since,
                        limit=limit // len(event_types)
                    )
                    for event in events:
                        entries.append(self._event_to_entry(event))
            else:
                # Fetch all recent events
                events = await self.events.get_recent(
                    user_id=user_id,
                    limit=limit
                )
                for event in events:
                    entries.append(self._event_to_entry(event))

        except Exception as e:
            self._logger.error(
                "state_changes_failed",
                error=str(e),
                user_id=user_id
            )

        # Sort by timestamp
        entries.sort(key=lambda e: e.timestamp, reverse=True)

        return entries[:limit]

    async def get_event_counts(
        self,
        user_id: str,
        since: Optional[datetime] = None
    ) -> Dict[str, int]:
        """
        Get counts of events by type.

        Args:
            user_id: User ID
            since: Only count events after this timestamp

        Returns:
            Dictionary mapping event types to counts
        """
        return await self.events.count_by_type(user_id, since)

    def _event_to_entry(self, event: DomainEvent) -> TimelineEntry:
        """Convert DomainEvent to TimelineEntry."""
        return TimelineEntry(
            entry_id=event.event_id,
            entry_type=TimelineEntryType.DOMAIN_EVENT,
            timestamp=event.timestamp,
            label=event.event_type,
            correlation_id=event.correlation_id,
            user_id=event.user_id,
            details=event.payload,
            aggregate_type=event.aggregate_type,
            aggregate_id=event.aggregate_id,
            event_type=event.event_type
        )

    def _trace_to_entry(self, trace: AgentTrace) -> TimelineEntry:
        """Convert AgentTrace to TimelineEntry."""
        return TimelineEntry(
            entry_id=trace.trace_id,
            entry_type=TimelineEntryType.AGENT_TRACE,
            timestamp=trace.started_at,
            label=f"{trace.agent_name}:{trace.stage}",
            correlation_id=trace.correlation_id,
            user_id=trace.user_id,
            details={
                "input_preview": str(trace.input_data)[:200] if trace.input_data else None,
                "output_preview": str(trace.output_data)[:200] if trace.output_data else None,
                "llm_model": trace.llm_model
            },
            agent_name=trace.agent_name,
            stage=trace.stage,
            status=trace.status,
            latency_ms=trace.latency_ms,
            confidence=trace.confidence
        )
