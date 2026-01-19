"""IPC management - CommBus coordinator and message bus.

This module implements the kernel's IPC management:
- Service registration and discovery
- Message routing
- Request dispatch
- Event publish/subscribe
"""

from jeeves_control_tower.ipc.coordinator import CommBusCoordinator
from jeeves_control_tower.ipc.commbus import (
    InMemoryCommBus,
    CommBusProtocol,
    Message,
    Event,
    Command,
    Query,
    Middleware,
    MiddlewareContext,
    get_commbus,
    reset_commbus,
    get_message_type,
)

__all__ = [
    # Coordinator
    "CommBusCoordinator",
    # CommBus
    "InMemoryCommBus",
    "CommBusProtocol",
    "Message",
    "Event",
    "Command",
    "Query",
    "Middleware",
    "MiddlewareContext",
    "get_commbus",
    "reset_commbus",
    "get_message_type",
]
