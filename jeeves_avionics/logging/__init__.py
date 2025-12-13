"""Centralized Logging Infrastructure for Jeeves Runtime.

This module extends jeeves_shared.logging with OTEL tracing support.
All base logging functionality comes from jeeves_shared.logging.

Constitutional Compliance:
- Avionics owns OTEL configuration (tracing, spans)
- Base logging utilities come from jeeves_shared (L0)
- Other layers only use injected LoggerProtocol

Usage:
    from jeeves_avionics.logging import create_logger, configure_logging

    # At application startup (once)
    configure_logging(level="INFO", json_output=True, enable_otel=True)

    # Create logger for injection
    logger = create_logger("my_component", envelope_id="...")

    # Inject into services/agents
    service = MyService(logger=logger)
"""

from __future__ import annotations

import time
from contextlib import contextmanager
from dataclasses import dataclass, field
from typing import Any, Dict, Optional, TYPE_CHECKING
from uuid import uuid4

import structlog

from jeeves_protocols import LoggerProtocol

# Import base logging from jeeves_shared (L0)
from jeeves_shared.logging import (
    JeevesLogger as _BaseJeevesLogger,
    configure_logging as _base_configure_logging,
    create_logger,
    create_agent_logger,
    create_capability_logger,
    create_tool_logger,
    get_component_logger,
    get_current_logger as _base_get_current_logger,
    set_current_logger,
    get_request_context,
    set_request_context,
    request_scope,
)

if TYPE_CHECKING:
    from jeeves_avionics.feature_flags import FeatureFlags

# Module state for OTEL
_OTEL_ENABLED = False
_ACTIVE_SPANS: Dict[str, "Span"] = {}


# =============================================================================
# Span Support for OTEL-Style Tracing
# =============================================================================

@dataclass
class Span:
    """OTEL-compatible span for distributed tracing.

    Used for Planner → Traverser → Synthesizer → Critic flow.
    """
    name: str
    trace_id: str
    span_id: str = field(default_factory=lambda: str(uuid4())[:16])
    parent_span_id: Optional[str] = None
    start_time: float = field(default_factory=time.time)
    end_time: Optional[float] = None
    attributes: Dict[str, Any] = field(default_factory=dict)
    status: str = "OK"

    def set_attribute(self, key: str, value: Any) -> None:
        """Set span attribute."""
        self.attributes[key] = value

    def end(self, status: str = "OK") -> None:
        """End the span."""
        self.end_time = time.time()
        self.status = status

    @property
    def duration_ms(self) -> int:
        """Duration in milliseconds."""
        end = self.end_time or time.time()
        return int((end - self.start_time) * 1000)

    def to_dict(self) -> Dict[str, Any]:
        """Export span data."""
        return {
            "name": self.name,
            "trace_id": self.trace_id,
            "span_id": self.span_id,
            "parent_span_id": self.parent_span_id,
            "start_time": self.start_time,
            "end_time": self.end_time,
            "duration_ms": self.duration_ms,
            "attributes": self.attributes,
            "status": self.status,
        }


# =============================================================================
# Extended Logger with Span Support
# =============================================================================

class JeevesLogger(_BaseJeevesLogger):
    """Extended JeevesLogger with OTEL span support.

    Inherits base functionality from jeeves_shared.logging.JeevesLogger
    and adds span context enrichment.
    """

    def __init__(
        self,
        base_logger: Any = None,
        context: Optional[Dict[str, Any]] = None,
        span: Optional[Span] = None,
    ):
        """Initialize logger.

        Args:
            base_logger: Underlying structlog logger (created if None)
            context: Bound context fields
            span: Active OTEL span for this logger
        """
        super().__init__(base_logger=base_logger, context=context)
        self._span = span

    def debug(self, msg: str, **kwargs: Any) -> None:
        """Log debug message with span enrichment."""
        self._log_with_span("debug", msg, **kwargs)

    def info(self, msg: str, **kwargs: Any) -> None:
        """Log info message with span enrichment."""
        self._log_with_span("info", msg, **kwargs)

    def warning(self, msg: str, **kwargs: Any) -> None:
        """Log warning message with span enrichment."""
        self._log_with_span("warning", msg, **kwargs)

    def error(self, msg: str, **kwargs: Any) -> None:
        """Log error message with span enrichment."""
        self._log_with_span("error", msg, **kwargs)

    def critical(self, msg: str, **kwargs: Any) -> None:
        """Log critical message with span enrichment."""
        self._log_with_span("critical", msg, **kwargs)

    def bind(self, **kwargs: Any) -> "JeevesLogger":
        """Create child logger with additional context."""
        new_context = {**self._context, **kwargs}
        return JeevesLogger(
            base_logger=structlog.get_logger(),
            context=new_context,
            span=self._span,
        )

    def with_span(self, span: Span) -> "JeevesLogger":
        """Create child logger with active span."""
        return JeevesLogger(
            base_logger=structlog.get_logger(),
            context=self._context,
            span=span,
        )

    def _log_with_span(self, level: str, msg: str, **kwargs: Any) -> None:
        """Internal logging with span enrichment."""
        # Add span context if active
        if self._span and _OTEL_ENABLED:
            kwargs["trace_id"] = self._span.trace_id
            kwargs["span_id"] = self._span.span_id
            if self._span.parent_span_id:
                kwargs["parent_span_id"] = self._span.parent_span_id

        getattr(self._logger, level)(msg, **kwargs)


# =============================================================================
# Configuration with OTEL Support
# =============================================================================

def configure_logging(
    level: str = "INFO",
    *,
    json_output: bool = True,
    enable_otel: bool = False,
    component_levels: Optional[Dict[str, str]] = None,
) -> None:
    """Configure logging for the Jeeves runtime with OTEL support.

    This should be called ONCE at application startup.
    All subsequent logging uses the configuration set here.

    Args:
        level: Default log level (DEBUG, INFO, WARNING, ERROR)
        json_output: If True, output JSON; if False, console format
        enable_otel: If True, enable OTEL span tracking
        component_levels: Override levels for specific components
    """
    global _OTEL_ENABLED
    _OTEL_ENABLED = enable_otel

    # Delegate to base configuration
    _base_configure_logging(
        level=level,
        json_output=json_output,
        component_levels=component_levels,
    )


def configure_from_flags(flags: "FeatureFlags") -> None:
    """Configure logging from feature flags.

    Args:
        flags: FeatureFlags instance
    """
    configure_logging(
        level="DEBUG" if flags.enable_debug_logging else "INFO",
        json_output=True,
        enable_otel=flags.enable_tracing,
    )


def get_current_logger() -> LoggerProtocol:
    """Get current logger for context-based access.

    Returns extended JeevesLogger if no context logger is set.
    """
    logger = _base_get_current_logger()
    # If we got the default base logger, return our extended version
    if isinstance(logger, _BaseJeevesLogger) and not isinstance(logger, JeevesLogger):
        return JeevesLogger()
    return logger


# =============================================================================
# OTEL Span Context Managers
# =============================================================================

@contextmanager
def trace_agent(
    agent_name: str,
    envelope_id: str,
    parent_span_id: Optional[str] = None,
):
    """Context manager for tracing agent execution.

    Creates an OTEL-style span for the agent's execution.

    Args:
        agent_name: Name of the agent
        envelope_id: Envelope ID (used as trace_id)
        parent_span_id: Parent span for nested tracing

    Yields:
        Tuple of (Span, LoggerProtocol with span context)

    Example:
        with trace_agent("planner", envelope.envelope_id) as (span, logger):
            span.set_attribute("plan_steps", 5)
            logger.info("planning_started")
            # ... do work ...
    """
    span = Span(
        name=f"agent.{agent_name}",
        trace_id=envelope_id,
        parent_span_id=parent_span_id,
    )
    span.set_attribute("agent.name", agent_name)

    # Register span
    _ACTIVE_SPANS[span.span_id] = span

    # Create logger with span context
    logger = JeevesLogger(
        context={"agent": agent_name, "envelope_id": envelope_id},
        span=span,
    )

    try:
        yield span, logger
        span.end("OK")
    except Exception as e:
        span.set_attribute("error", str(e))
        span.set_attribute("error_type", type(e).__name__)
        span.end("ERROR")
        raise
    finally:
        # Emit span completion log
        if _OTEL_ENABLED:
            structlog.get_logger().info(
                "span_completed",
                **span.to_dict(),
            )
        # Unregister span
        _ACTIVE_SPANS.pop(span.span_id, None)


@contextmanager
def trace_tool(
    tool_name: str,
    envelope_id: str,
    parent_span_id: Optional[str] = None,
    caller_agent: Optional[str] = None,
):
    """Context manager for tracing tool execution.

    Args:
        tool_name: Name of the tool
        envelope_id: Envelope ID (trace_id)
        parent_span_id: Parent span ID
        caller_agent: Agent requesting tool execution

    Yields:
        Tuple of (Span, LoggerProtocol)
    """
    span = Span(
        name=f"tool.{tool_name}",
        trace_id=envelope_id,
        parent_span_id=parent_span_id,
    )
    span.set_attribute("tool.name", tool_name)
    if caller_agent:
        span.set_attribute("caller_agent", caller_agent)

    _ACTIVE_SPANS[span.span_id] = span

    logger = JeevesLogger(
        context={"tool": tool_name, "envelope_id": envelope_id},
        span=span,
    )

    try:
        yield span, logger
        span.end("OK")
    except Exception as e:
        span.set_attribute("error", str(e))
        span.end("ERROR")
        raise
    finally:
        if _OTEL_ENABLED:
            structlog.get_logger().info("span_completed", **span.to_dict())
        _ACTIVE_SPANS.pop(span.span_id, None)


def get_current_span(span_id: str) -> Optional[Span]:
    """Get an active span by ID."""
    return _ACTIVE_SPANS.get(span_id)


# =============================================================================
# Exports
# =============================================================================

# Re-export from adapter module (ADR-001)
from jeeves_avionics.logging.adapter import StructlogAdapter, create_structlog_adapter

# Re-export from context module (ADR-001)
from jeeves_avionics.logging.context import (
    bind_logger_context,
    request_context,
)


__all__ = [
    # Configuration
    "configure_logging",
    "configure_from_flags",
    # Logger creation (re-exported from jeeves_shared)
    "create_logger",
    "create_agent_logger",
    "create_capability_logger",
    "create_tool_logger",
    "get_component_logger",
    # Types
    "JeevesLogger",
    "Span",
    # Tracing
    "trace_agent",
    "trace_tool",
    "get_current_span",
    # ADR-001: Adapter
    "StructlogAdapter",
    "create_structlog_adapter",
    # Context (re-exported from jeeves_shared)
    "get_request_context",
    "set_request_context",
    "get_current_logger",
    "set_current_logger",
    "request_scope",
    # ADR-001: Context extensions
    "bind_logger_context",
    "request_context",
]
