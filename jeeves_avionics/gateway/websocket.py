"""WebSocket client management and event subscription.

Constitutional Pattern:
- WebSocket handler subscribes to gateway_events
- Routers publish events, don't know about WebSocket
- This module bridges events to WebSocket clients

Usage:
    # In main.py at startup
    from jeeves_avionics.gateway.websocket import setup_websocket_subscriptions
    setup_websocket_subscriptions()
"""

from typing import Set
from fastapi import WebSocket

from jeeves_avionics.logging import get_current_logger


# Connected WebSocket clients (module-level state)
_connected_clients: Set[WebSocket] = set()


def get_connected_clients() -> Set[WebSocket]:
    """Get the set of connected WebSocket clients."""
    return _connected_clients


def register_client(websocket: WebSocket) -> int:
    """Register a WebSocket client.

    Returns:
        Current count of connected clients
    """
    _connected_clients.add(websocket)
    return len(_connected_clients)


def unregister_client(websocket: WebSocket) -> int:
    """Unregister a WebSocket client.

    Returns:
        Current count of connected clients
    """
    _connected_clients.discard(websocket)
    return len(_connected_clients)


async def broadcast_to_clients(message: dict) -> int:
    """Broadcast a message to all connected WebSocket clients.

    Returns:
        Number of clients message was sent to
    """
    if not _connected_clients:
        return 0

    _logger = get_current_logger()
    disconnected = set()
    sent_count = 0

    for client in _connected_clients:
        try:
            await client.send_json(message)
            sent_count += 1
        except Exception as e:
            _logger.debug(
                "websocket_send_failed",
                error=str(e),
                error_type=type(e).__name__,
            )
            disconnected.add(client)

    # Clean up disconnected clients
    for client in disconnected:
        _connected_clients.discard(client)

    return sent_count


# =============================================================================
# Event Bus Integration
# =============================================================================

async def _handle_agent_event(event) -> None:
    """Handle agent events by broadcasting to WebSocket clients.

    This is the subscriber that bridges gateway events to WebSocket.
    """
    await broadcast_to_clients({
        "event": event.name,
        "payload": event.payload,
    })


def setup_websocket_subscriptions() -> None:
    """Subscribe to gateway events for WebSocket broadcasting.

    Call this once at application startup (in main.py lifespan).
    """
    from jeeves_avionics.gateway.event_bus import gateway_events

    # Subscribe to all agent-related events
    gateway_events.subscribe("agent.*", _handle_agent_event)
    gateway_events.subscribe("orchestrator.*", _handle_agent_event)
    gateway_events.subscribe("planner.*", _handle_agent_event)
    gateway_events.subscribe("executor.*", _handle_agent_event)
    gateway_events.subscribe("critic.*", _handle_agent_event)
    gateway_events.subscribe("synthesizer.*", _handle_agent_event)


__all__ = [
    "get_connected_clients",
    "register_client",
    "unregister_client",
    "broadcast_to_clients",
    "setup_websocket_subscriptions",
]
