"""CommBus adapters for Control Tower IPC.

This module provides adapters to bridge Python CommBusCoordinator
with external message buses (Go CommBus, HTTP endpoints, etc.).
"""

from jeeves_control_tower.ipc.adapters.http_adapter import HttpCommBusAdapter

__all__ = ["HttpCommBusAdapter"]
