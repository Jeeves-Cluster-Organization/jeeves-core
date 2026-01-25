"""In-Memory CommBus - Python message bus matching Go design.

This provides the same semantics as commbus/bus.go:
- Publish: Fan-out events to all subscribers
- Send: Fire-and-forget commands to single handler
- Query: Request-response with timeout

Constitutional Reference:
- Control Tower CONSTITUTION: CommBus communication for service dispatch
- Memory Module CONSTITUTION P4: Memory operations publish events via CommBus

Usage:
    bus = InMemoryCommBus()

    # Subscribe to events
    bus.subscribe("MemoryStored", handle_memory_stored)

    # Register handlers
    bus.register_handler("GetSessionState", handle_get_session_state)

    # Publish events
    await bus.publish(MemoryStored(item_id="123", ...))

    # Send commands
    await bus.send(ClearSession(session_id="..."))

    # Query with response
    result = await bus.query(GetSessionState(session_id="..."))
"""

from __future__ import annotations

import asyncio
import logging
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Callable, Dict, List, Optional, Protocol, TypeVar, runtime_checkable

from protocols import LoggerProtocol


# =============================================================================
# MESSAGE PROTOCOLS
# =============================================================================


@runtime_checkable
class Message(Protocol):
    """Base protocol for all CommBus messages."""
    category: str  # "event", "command", or "query"


@runtime_checkable
class Event(Protocol):
    """Event message protocol - fan-out to all subscribers."""
    category: str


@runtime_checkable
class Command(Protocol):
    """Command message protocol - fire-and-forget to single handler."""
    category: str


@runtime_checkable
class Query(Protocol):
    """Query message protocol - request-response with single handler."""
    category: str


# Type for handlers
T = TypeVar("T", bound=Message)
HandlerFunc = Callable[[Any], Any]
AsyncHandlerFunc = Callable[[Any], "asyncio.Future[Any]"]


# =============================================================================
# MIDDLEWARE
# =============================================================================


@dataclass
class MiddlewareContext:
    """Context passed to middleware."""
    message: Any
    message_type: str
    timestamp: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    metadata: Dict[str, Any] = field(default_factory=dict)


class Middleware(Protocol):
    """Middleware protocol - matches Go middleware.Middleware."""

    async def before(self, ctx: MiddlewareContext, message: Any) -> Optional[Any]:
        """Called before handler. Return None to abort, or modified message."""
        ...

    async def after(
        self, ctx: MiddlewareContext, message: Any, result: Any, error: Optional[Exception]
    ) -> Optional[Any]:
        """Called after handler. Can modify result."""
        ...


# =============================================================================
# COMMBUS PROTOCOL
# =============================================================================


@runtime_checkable
class CommBusProtocol(Protocol):
    """Protocol for CommBus implementations."""

    async def publish(self, event: Any) -> None:
        """Publish event to all subscribers."""
        ...

    async def send(self, command: Any) -> None:
        """Send command to handler (fire-and-forget)."""
        ...

    async def query(self, query: Any, timeout: float = 30.0) -> Any:
        """Send query and wait for response."""
        ...

    def subscribe(self, event_type: str, handler: HandlerFunc) -> Callable[[], None]:
        """Subscribe to event type. Returns unsubscribe function."""
        ...

    def register_handler(self, message_type: str, handler: HandlerFunc) -> None:
        """Register handler for command/query type."""
        ...


# =============================================================================
# IN-MEMORY IMPLEMENTATION
# =============================================================================


def get_message_type(message: Any) -> str:
    """Get message type name for routing."""
    return type(message).__name__


class InMemoryCommBus:
    """In-memory CommBus implementation matching Go design.

    Thread-safe, async-compatible message bus for single-process deployments.

    Features:
    - Event fan-out to multiple subscribers
    - Query request-response with timeout
    - Command fire-and-forget
    - Middleware chain for cross-cutting concerns
    - Handler introspection
    """

    def __init__(
        self,
        query_timeout: float = 30.0,
        logger: Optional[LoggerProtocol] = None,
    ):
        """Initialize CommBus.

        Args:
            query_timeout: Default timeout for queries in seconds
            logger: Optional logger instance
        """
        self._handlers: Dict[str, HandlerFunc] = {}
        self._subscribers: Dict[str, List[HandlerFunc]] = {}
        self._middleware: List[Middleware] = []
        self._query_timeout = query_timeout
        self._lock = asyncio.Lock()
        self._logger = logger or logging.getLogger(__name__)

    # =========================================================================
    # MESSAGING
    # =========================================================================

    async def publish(self, event: Any) -> None:
        """Publish event to all subscribers.

        Events are processed concurrently by all subscribers.
        Subscriber errors are logged but don't stop other subscribers.
        """
        event_type = get_message_type(event)

        # Run middleware before
        processed = await self._run_middleware_before(event)
        if processed is None:
            self._logger.info("commbus_event_aborted_by_middleware", event_type=event_type)
            return

        # Get subscribers (copy for thread safety)
        async with self._lock:
            subscribers = list(self._subscribers.get(event_type, []))

        if not subscribers:
            self._logger.debug("commbus_no_subscribers", event_type=event_type)
            await self._run_middleware_after(event, None, None)
            return

        # Fan-out to all subscribers concurrently
        tasks = []
        for handler in subscribers:
            tasks.append(self._call_handler(handler, processed))

        results = await asyncio.gather(*tasks, return_exceptions=True)

        # Log errors
        first_error = None
        for i, result in enumerate(results):
            if isinstance(result, Exception):
                if first_error is None:
                    first_error = result
                self._logger.warning(
                    "commbus_subscriber_error",
                    event_type=event_type,
                    subscriber_index=i,
                    error=str(result),
                )

        await self._run_middleware_after(event, None, first_error)

    async def send(self, command: Any) -> None:
        """Send command to handler (fire-and-forget).

        Commands have exactly one handler. Handler errors are logged.
        """
        message_type = get_message_type(command)

        # Run middleware before
        processed = await self._run_middleware_before(command)
        if processed is None:
            self._logger.info("commbus_command_aborted_by_middleware", message_type=message_type)
            return

        # Get handler
        async with self._lock:
            handler = self._handlers.get(message_type)

        if handler is None:
            self._logger.debug("commbus_no_handler", message_type=message_type)
            return

        # Execute
        try:
            await self._call_handler(handler, processed)
            await self._run_middleware_after(command, None, None)
        except Exception as e:
            self._logger.warning(
                "commbus_command_handler_error",
                message_type=message_type,
                error=str(e),
            )
            await self._run_middleware_after(command, None, e)

    async def query(self, query: Any, timeout: Optional[float] = None) -> Any:
        """Send query and wait for response.

        Queries have exactly one handler and require a response.

        Args:
            query: The query message
            timeout: Timeout in seconds (default: query_timeout from constructor)

        Returns:
            Query response

        Raises:
            asyncio.TimeoutError: If query times out
            ValueError: If no handler registered
        """
        message_type = get_message_type(query)
        effective_timeout = timeout or self._query_timeout

        # Run middleware before
        processed = await self._run_middleware_before(query)
        if processed is None:
            raise ValueError(f"Query {message_type} aborted by middleware")

        # Get handler
        async with self._lock:
            handler = self._handlers.get(message_type)

        if handler is None:
            raise ValueError(f"No handler for query {message_type}")

        # Execute with timeout
        try:
            result = await asyncio.wait_for(
                self._call_handler(handler, processed),
                timeout=effective_timeout,
            )
            await self._run_middleware_after(query, result, None)
            return result
        except asyncio.TimeoutError:
            self._logger.warning(
                "commbus_query_timeout",
                message_type=message_type,
                timeout=effective_timeout,
            )
            await self._run_middleware_after(query, None, asyncio.TimeoutError())
            raise
        except Exception as e:
            await self._run_middleware_after(query, None, e)
            raise

    # =========================================================================
    # REGISTRATION
    # =========================================================================

    def subscribe(self, event_type: str, handler: HandlerFunc) -> Callable[[], None]:
        """Subscribe to event type.

        Args:
            event_type: Event type name (e.g., "MemoryStored")
            handler: Handler function (sync or async)

        Returns:
            Unsubscribe function for cleanup
        """
        # Note: This is sync to match Go API, but we use asyncio.Lock
        # Can be called from sync context safely
        loop = asyncio.get_event_loop()
        if loop.is_running():
            # Schedule async registration
            asyncio.create_task(self._subscribe_async(event_type, handler))
        else:
            # Sync context
            if event_type not in self._subscribers:
                self._subscribers[event_type] = []
            self._subscribers[event_type].append(handler)

        self._logger.debug("commbus_subscribed", event_type=event_type)

        # Return unsubscribe function
        def unsubscribe():
            if event_type in self._subscribers:
                try:
                    self._subscribers[event_type].remove(handler)
                    self._logger.debug("commbus_unsubscribed", event_type=event_type)
                except ValueError:
                    pass

        return unsubscribe

    async def _subscribe_async(self, event_type: str, handler: HandlerFunc) -> None:
        """Async subscribe helper."""
        async with self._lock:
            if event_type not in self._subscribers:
                self._subscribers[event_type] = []
            self._subscribers[event_type].append(handler)

    def register_handler(self, message_type: str, handler: HandlerFunc) -> None:
        """Register handler for command/query type.

        Only one handler per message type is allowed.

        Args:
            message_type: Message type name (e.g., "GetSessionState")
            handler: Handler function (sync or async)

        Raises:
            ValueError: If handler already registered
        """
        if message_type in self._handlers:
            raise ValueError(f"Handler already registered for {message_type}")

        self._handlers[message_type] = handler
        self._logger.debug("commbus_handler_registered", message_type=message_type)

    def add_middleware(self, middleware: Middleware) -> None:
        """Add middleware to the bus.

        Middleware is executed in registration order.
        """
        self._middleware.append(middleware)
        self._logger.debug("commbus_middleware_added")

    # =========================================================================
    # INTROSPECTION
    # =========================================================================

    def has_handler(self, message_type: str) -> bool:
        """Check if handler is registered for message type."""
        return message_type in self._handlers

    def get_subscribers(self, event_type: str) -> List[HandlerFunc]:
        """Get all subscribers for event type."""
        return list(self._subscribers.get(event_type, []))

    def get_registered_types(self) -> List[str]:
        """Get all registered message types."""
        types = set(self._handlers.keys()) | set(self._subscribers.keys())
        return list(types)

    # =========================================================================
    # LIFECYCLE
    # =========================================================================

    def clear(self) -> None:
        """Clear all handlers, subscribers, and middleware."""
        self._handlers.clear()
        self._subscribers.clear()
        self._middleware.clear()
        self._logger.debug("commbus_cleared")

    # =========================================================================
    # INTERNAL HELPERS
    # =========================================================================

    async def _call_handler(self, handler: HandlerFunc, message: Any) -> Any:
        """Call handler, handling both sync and async functions."""
        result = handler(message)
        if asyncio.iscoroutine(result):
            return await result
        return result

    async def _run_middleware_before(self, message: Any) -> Optional[Any]:
        """Run middleware before chain."""
        current = message
        ctx = MiddlewareContext(
            message=message,
            message_type=get_message_type(message),
        )

        for mw in self._middleware:
            try:
                result = mw.before(ctx, current)
                if asyncio.iscoroutine(result):
                    result = await result
                if result is None:
                    return None
                current = result
            except Exception as e:
                self._logger.warning("commbus_middleware_before_error", error=str(e))
                return None

        return current

    async def _run_middleware_after(
        self, message: Any, result: Any, error: Optional[Exception]
    ) -> Optional[Any]:
        """Run middleware after chain (reverse order)."""
        current_result = result
        ctx = MiddlewareContext(
            message=message,
            message_type=get_message_type(message),
        )

        for mw in reversed(self._middleware):
            try:
                after_result = mw.after(ctx, message, current_result, error)
                if asyncio.iscoroutine(after_result):
                    after_result = await after_result
                if after_result is not None:
                    current_result = after_result
            except Exception as e:
                self._logger.warning("commbus_middleware_after_error", error=str(e))

        return current_result


# =============================================================================
# SINGLETON INSTANCE
# =============================================================================

_global_bus: Optional[InMemoryCommBus] = None


def get_commbus(logger: Optional[LoggerProtocol] = None) -> InMemoryCommBus:
    """Get the global CommBus instance.

    Creates the instance lazily on first call.

    Args:
        logger: Optional logger to use (only used on creation)

    Returns:
        The global InMemoryCommBus instance
    """
    global _global_bus
    if _global_bus is None:
        _global_bus = InMemoryCommBus(logger=logger)
    return _global_bus


def reset_commbus() -> None:
    """Reset the global CommBus instance (for testing)."""
    global _global_bus
    if _global_bus is not None:
        _global_bus.clear()
    _global_bus = None


__all__ = [
    "Message",
    "Event",
    "Command",
    "Query",
    "Middleware",
    "MiddlewareContext",
    "CommBusProtocol",
    "InMemoryCommBus",
    "get_commbus",
    "reset_commbus",
    "get_message_type",
]
