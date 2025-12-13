"""IPC management - CommBus coordinator.

This module implements the kernel's IPC management:
- Service registration and discovery
- Message routing
- Request dispatch
"""

from jeeves_control_tower.ipc.coordinator import CommBusCoordinator

__all__ = ["CommBusCoordinator"]
