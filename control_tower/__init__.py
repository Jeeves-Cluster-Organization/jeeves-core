"""Control Tower - the Jeeves kernel.

This package provides the central coordinator that manages:
- Request lifecycle (process scheduling)
- Resource allocation (quotas)
- Service dispatch (IPC)
- Event streaming (interrupts)

Exports:
    ControlTower: Main kernel class
    ControlTowerProtocol: Protocol interface for the kernel
    ResourceQuota: Resource limits for requests
    ServiceDescriptor: Service registration metadata
    ProcessState: Process lifecycle states
    SchedulingPriority: Priority levels for scheduling
    DispatchTarget: Target for request dispatch
    KernelEvent: Events emitted by the kernel
    InterruptService: Unified interrupt handling service
    InterruptKind: Types of interrupts (from protocols)
    InterruptResponse: Response to an interrupt (from protocols)
"""

from control_tower.kernel import ControlTower
from control_tower.protocols import ControlTowerProtocol
from control_tower.types import (
    DispatchTarget,
    KernelEvent,
    ProcessState,
    ResourceQuota,
    SchedulingPriority,
    ServiceDescriptor,
)
from control_tower.services.interrupt_service import InterruptService

# Re-export from protocols for convenience
from protocols import InterruptKind, InterruptResponse

__all__ = [
    "ControlTower",
    "ControlTowerProtocol",
    "DispatchTarget",
    "InterruptKind",
    "InterruptResponse",
    "InterruptService",
    "KernelEvent",
    "ProcessState",
    "ResourceQuota",
    "SchedulingPriority",
    "ServiceDescriptor",
]
