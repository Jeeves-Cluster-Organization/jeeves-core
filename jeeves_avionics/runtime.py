"""Agent Runtime - Composition target for vertical agents.

This module provides AgentRuntime, which verticals COMPOSE with rather than
inheriting from Core's Agent base class.

Constitutional Pattern (Amendment XXII):
- Core provides protocols (interfaces) and internal implementations
- Infra provides runtime services that wrap Core capabilities
- Verticals compose with runtime, never import from Core directly

Usage:
    from jeeves_avionics import AgentRuntime, create_agent_logger

    logger = create_agent_logger("MyAgent", envelope_id="...")

    class MyVerticalAgent:
        def __init__(self, runtime: AgentRuntime):
            self._runtime = runtime
            self.name = runtime.name

        async def process(self, envelope):
            await self._runtime.log_start(...)
            await self._runtime.emit_started()
            # ... agent logic ...
            await self._runtime.emit_completed(status="success")
            return envelope
"""

import time
from typing import Any, Optional, TYPE_CHECKING

from jeeves_protocols import (
    PersistenceProtocol,
    EventContextProtocol,
    LoggerProtocol,
)

if TYPE_CHECKING:
    from jeeves_avionics.logging import JeevesLogger


class TimingContext:
    """Context manager for tracking operation duration."""

    def __init__(self):
        self.start_time: float = 0
        self.end_time: float = 0

    def __enter__(self) -> "TimingContext":
        self.start_time = time.perf_counter()
        return self

    def __exit__(self, *args):
        self.end_time = time.perf_counter()

    @property
    def duration_ms(self) -> int:
        """Duration in milliseconds (minimum 1ms)."""
        if self.end_time == 0:
            elapsed = time.perf_counter() - self.start_time
        else:
            elapsed = self.end_time - self.start_time
        return max(1, int(elapsed * 1000))


class AgentRuntime:
    """Runtime services for vertical agents.

    This is the infra adapter that wraps Core runtime capabilities
    and exposes them to verticals in a decoupled way.

    Verticals COMPOSE with this class, they don't inherit from it.
    This ensures Core can be extracted without breaking verticals.

    Provides:
    - Structured logging via injected LoggerProtocol
    - Timing utilities for performance tracking
    - Event emission via EventContextProtocol
    - Persistence access via PersistenceProtocol

    Example:
        from jeeves_avionics import create_agent_logger

        logger = create_agent_logger("MyAgent", envelope_id="env-123")
        runtime = AgentRuntime(
            name="MyAgent",
            logger=logger,
            persistence=db_client,
            event_context=event_ctx,
        )

        class MyAgent:
            def __init__(self, runtime: AgentRuntime):
                self._runtime = runtime
    """

    def __init__(
        self,
        name: str,
        *,
        logger: Optional[LoggerProtocol] = None,
        persistence: Optional[PersistenceProtocol] = None,
        event_context: Optional[EventContextProtocol] = None,
    ):
        """Initialize agent runtime.

        Args:
            name: Agent name for logging and events
            logger: LoggerProtocol implementation (REQUIRED for production)
            persistence: Optional persistence protocol implementation
            event_context: Optional event context for streaming/audit events
        """
        self.name = name
        self._persistence = persistence
        self._event_context = event_context
        self._timing: Optional[TimingContext] = None

        # Use injected logger or create default (for backwards compat during migration)
        if logger is not None:
            self.logger = logger.bind(agent=name)
        else:
            # Fallback: create logger via central module
            # This path should only be used during migration
            from jeeves_avionics.logging import create_agent_logger
            self.logger = create_agent_logger(name)

    @property
    def db(self) -> PersistenceProtocol:
        """Get persistence interface.

        Raises:
            RuntimeError: If persistence not configured
        """
        if self._persistence is None:
            raise RuntimeError(
                f"{self.name}: Persistence not configured. "
                "Inject a PersistenceProtocol implementation at construction."
            )
        return self._persistence

    @property
    def persistence(self) -> Optional[PersistenceProtocol]:
        """Get persistence interface (returns None if not configured)."""
        return self._persistence

    @property
    def event_context(self) -> Optional[EventContextProtocol]:
        """Get the current event context."""
        return self._event_context

    def set_event_context(self, context: Optional[EventContextProtocol]) -> None:
        """Set the event context for this runtime.

        Args:
            context: EventContextProtocol to use, or None to disable events
        """
        self._event_context = context

    @property
    def agent_name_key(self) -> str:
        """Get the agent name key used for event emission.

        Maps internal agent names to event keys using generic rules:
        - Remove "Agent" suffix if present
        - Convert to lowercase

        Constitutional Pattern (Avionics R3):
        No capability-specific handling here. Capabilities should name their
        agents appropriately (e.g., "PlannerAgent" -> "planner").
        """
        name = self.name
        if name.endswith("Agent"):
            name = name[:-5]
        return name.lower()

    def start_timing(self) -> TimingContext:
        """Start timing an operation."""
        self._timing = TimingContext()
        self._timing.__enter__()
        return self._timing

    def stop_timing(self) -> int:
        """Stop timing and return duration in ms."""
        if self._timing:
            self._timing.__exit__(None, None, None)
            return self._timing.duration_ms
        return 0

    async def log_start(self, **context) -> None:
        """Log agent start with context."""
        self.logger.info(
            f"{self.agent_name_key}_started",
            **context
        )

    async def log_complete(self, **context) -> None:
        """Log agent completion with context."""
        self.logger.info(
            f"{self.agent_name_key}_completed",
            **context
        )

    async def log_error(self, error: Exception, **context) -> None:
        """Log agent error with context."""
        self.logger.error(
            f"{self.agent_name_key}_error",
            error=str(error),
            error_type=type(error).__name__,
            **context
        )

    async def emit_started(self) -> None:
        """Emit agent started event."""
        if self._event_context:
            await self._event_context.emit_agent_started(
                agent_name=self.agent_name_key
            )

    async def emit_completed(
        self,
        status: str = "success",
        error: Optional[str] = None,
        **extra_data
    ) -> None:
        """Emit agent completed event with optional extra data.

        Args:
            status: Completion status ("success", "error")
            error: Error message if status is "error"
            **extra_data: Additional data to include in event
        """
        if self._event_context:
            await self._event_context.emit_agent_completed(
                agent_name=self.agent_name_key,
                status=status,
                error=error,
                **extra_data
            )
