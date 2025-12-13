"""Gateway Event Bus - Pub/Sub for internal gateway events.

Constitutional Pattern:
- Routers PUBLISH events (fire and forget)
- WebSocket handler SUBSCRIBES to events
- Zero coupling between routers and WebSocket implementation

This breaks the circular dependency between chat.py and main.py by
introducing an intermediary that both can depend on.

Usage:
    # In router (publisher)
    from jeeves_avionics.gateway.event_bus import gateway_events
    await gateway_events.publish("agent.started", {"agent": "planner"})

    # In main.py (subscriber)
    from jeeves_avionics.gateway.event_bus import gateway_events
    gateway_events.subscribe("agent.*", broadcast_handler)
"""

import asyncio
import fnmatch
from dataclasses import dataclass, field
from typing import Any, Awaitable, Callable, Dict, List, Set
from weakref import WeakSet

from jeeves_avionics.logging import get_current_logger


@dataclass
class GatewayEvent:
    """Event published on the gateway bus."""
    name: str
    payload: Dict[str, Any]


# Type alias for event handlers
EventHandler = Callable[[GatewayEvent], Awaitable[None]]


class GatewayEventBus:
    """Simple async event bus for gateway internal communication.

    Features:
    - Pattern-based subscriptions (e.g., "agent.*" matches "agent.started")
    - Async handlers
    - Fire-and-forget publishing (errors logged, not propagated)

    Constitutional Compliance:
    - Decouples publishers from subscribers
    - Routers don't need to know about WebSocket implementation
    - WebSocket handler doesn't need to know about router internals
    """

    def __init__(self):
        self._handlers: Dict[str, List[EventHandler]] = {}
        self._logger = None

    @property
    def logger(self):
        if self._logger is None:
            self._logger = get_current_logger()
        return self._logger

    def subscribe(self, pattern: str, handler: EventHandler) -> None:
        """Subscribe to events matching a pattern.

        Args:
            pattern: Event name pattern (supports * wildcards)
                     e.g., "agent.*" matches "agent.started", "agent.completed"
            handler: Async function to call with GatewayEvent
        """
        if pattern not in self._handlers:
            self._handlers[pattern] = []
        self._handlers[pattern].append(handler)
        self.logger.debug("event_bus_subscribed", pattern=pattern)

    def unsubscribe(self, pattern: str, handler: EventHandler) -> bool:
        """Unsubscribe a handler from a pattern.

        Args:
            pattern: Event name pattern
            handler: Handler to remove

        Returns:
            True if handler was found and removed
        """
        if pattern in self._handlers:
            try:
                self._handlers[pattern].remove(handler)
                return True
            except ValueError:
                pass
        return False

    async def publish(self, event_name: str, payload: Dict[str, Any]) -> int:
        """Publish an event to all matching subscribers.

        Fire-and-forget: errors in handlers are logged but not propagated.

        Args:
            event_name: Name of the event (e.g., "agent.started")
            payload: Event data dictionary

        Returns:
            Number of handlers that were invoked
        """
        event = GatewayEvent(name=event_name, payload=payload)
        handlers_invoked = 0

        for pattern, handlers in self._handlers.items():
            if fnmatch.fnmatch(event_name, pattern):
                for handler in handlers:
                    try:
                        await handler(event)
                        handlers_invoked += 1
                    except Exception as e:
                        self.logger.error(
                            "event_bus_handler_error",
                            event=event_name,
                            pattern=pattern,
                            error=str(e),
                            error_type=type(e).__name__,
                        )

        return handlers_invoked

    def clear(self) -> None:
        """Clear all subscriptions (for testing)."""
        self._handlers.clear()


# Global event bus instance for gateway
gateway_events = GatewayEventBus()


__all__ = [
    "GatewayEvent",
    "GatewayEventBus",
    "gateway_events",
]
