# Jeeves Control Tower

**Package:** `control_tower`  
**Layer:** L1 (Kernel Layer)  
**Version:** 1.3  
**Updated:** 2026-01-23

---

## Overview

The Control Tower is the **kernel layer** of the Jeeves architecture. It acts as a microkernel that sits between the Gateway (HTTP/gRPC) and the Mission System services, managing:

- **Request Lifecycle** (process scheduling)
- **Resource Allocation** (quotas and rate limiting)
- **Service Dispatch** (IPC via CommBus)
- **Event Streaming** (interrupts and notifications)

### Architecture Position

```
Gateway (HTTP/gRPC)
       │
       ▼
┌──────────────────────┐
│   CONTROL TOWER      │  ◄── YOU ARE HERE (L1 - Kernel)
│   (jeeves_control_   │
│    tower)            │
└──────────────────────┘
       │
       ▼
Memory Module (L2 - Event Sourcing, Session State, etc.)
       │
       ▼
Avionics (L3 - LLM, Database, Gateway, etc.)
       │
       ▼
Mission System (L4 - Orchestration, API, etc.)
```

### OS Analogy

Control Tower mirrors operating system kernel concepts:

| OS Concept | Control Tower Equivalent | Module |
|------------|--------------------------|--------|
| Kernel | `ControlTower` | `kernel.py` |
| Process Scheduler | `LifecycleManager` | `lifecycle/manager.py` |
| cgroups | `ResourceTracker` | `resources/tracker.py` |
| IPC Manager | `CommBusCoordinator` | `ipc/coordinator.py` |
| Interrupt Handler | `EventAggregator` | `events/aggregator.py` |
| Process States | `ProcessState` enum | `types.py` |
| PCB | `ProcessControlBlock` | `types.py` |
| System Calls | `protocols` | External package |

---

## Core Responsibilities

### What Control Tower DOES:

1. **Request Lifecycle Management** - Track requests from submission to completion
2. **Resource Quota Enforcement** - Manage LLM tokens, tool calls, time budgets
3. **Service Dispatch** - Route requests to registered services via CommBus
4. **Event Streaming** - Aggregate and emit events for real-time monitoring
5. **Interrupt Handling** - Unified handling of clarifications, confirmations, errors

### What Control Tower does NOT do:

- Implement business logic (that's Mission System)
- Manage database connections directly (that's Avionics)
- Execute agent pipelines (that's Mission System verticals)
- Call LLM APIs directly (that's Avionics)

---

## Package Structure

```
control_tower/
├── __init__.py              # Public API exports
├── kernel.py                # Main ControlTower class
├── protocols.py             # Protocol interfaces (ABIs)
├── types.py                 # OS-level type definitions
├── CONSTITUTION.md          # Component rules and contracts
│
├── events/                  # Event/Interrupt handling
│   ├── __init__.py
│   └── aggregator.py        # EventAggregator implementation
│
├── ipc/                     # Inter-process communication
│   ├── __init__.py
│   ├── commbus.py           # InMemoryCommBus implementation
│   ├── coordinator.py       # CommBusCoordinator
│   └── adapters/            # External bus adapters
│       └── __init__.py
│
├── lifecycle/               # Process lifecycle management
│   ├── __init__.py
│   └── manager.py           # LifecycleManager implementation
│
├── resources/               # Resource management
│   ├── __init__.py
│   ├── rate_limiter.py      # Sliding window rate limiter
│   └── tracker.py           # ResourceTracker implementation
│
└── services/                # Kernel services
    ├── __init__.py
    └── interrupt_service.py # Unified interrupt handling
```

---

## Quick Start

### Basic Usage

```python
from control_tower import ControlTower
from protocols import GenericEnvelope

# 1. Create kernel instance
kernel = ControlTower(logger=logger)

# 2. Register services at startup
kernel.register_service(
    name="my_capability",
    service_type="flow",
    handler=capability_service.handle,
)

# 3. Submit a request
envelope = GenericEnvelope(
    user_input="Analyze this codebase",
    user_id="user-123",
    session_id="session-456",
)
result = await kernel.submit_request(envelope)

# 4. Handle clarification if needed
if result.interrupt_pending:
    user_response = await get_user_input(result.interrupt.question)
    result = await kernel.resume_request(
        pid=result.envelope_id,
        interrupt_id=result.interrupt.id,
        response=InterruptResponse(text=user_response),
    )
```

---

## Documentation Index

| Document | Description |
|----------|-------------|
| [kernel.md](./kernel.md) | ControlTower kernel and core protocols |
| [events.md](./events.md) | Event aggregation and interrupt handling |
| [ipc.md](./ipc.md) | Inter-process communication and CommBus |
| [lifecycle.md](./lifecycle.md) | Process lifecycle and state machine |
| [resources.md](./resources.md) | Resource tracking and rate limiting |
| [services.md](./services.md) | Kernel services (InterruptService) |

---

## Public API

### Main Classes

| Class | Description |
|-------|-------------|
| `ControlTower` | Main kernel interface - composes all subsystems |
| `ControlTowerProtocol` | Protocol interface for kernel operations |

### Types (from `types.py`)

| Type | Description |
|------|-------------|
| `ProcessState` | Process lifecycle states (NEW, READY, RUNNING, etc.) |
| `SchedulingPriority` | Priority levels (REALTIME, HIGH, NORMAL, LOW, IDLE) |
| `ProcessControlBlock` | Kernel's metadata about a request |
| `ResourceQuota` | Resource limits for a process |
| `ResourceUsage` | Current resource usage tracking |
| `ServiceDescriptor` | Registered service metadata |
| `DispatchTarget` | Target for request dispatch |
| `KernelEvent` | Events emitted by the kernel |

### Re-exported Types

| Type | Source | Description |
|------|--------|-------------|
| `InterruptKind` | `protocols` | Types of interrupts |
| `InterruptResponse` | `protocols` | Response to an interrupt |
| `InterruptService` | `services/` | Unified interrupt handling service |

---

## Import Boundaries (Constitutional Rule R1)

Control Tower follows strict import boundaries as defined in CONSTITUTION.md:

```python
# ALLOWED: Import from protocols and shared only
from protocols import GenericEnvelope, LoggerProtocol
from shared import utc_now

# FORBIDDEN: Direct imports from other layers
# from mission_system import FlowService  # NO
# from avionics import DatabaseClient     # NO
# from memory_module import MemoryService # NO
```

**Rationale:** Control Tower is the kernel. It dispatches TO services, not imports FROM them.

---

## Cross-References

- **CONSTITUTION.md** - Component rules and contracts
- **Architectural Contracts** - `../docs/CONTRACTS.md`
- **Protocols Package** - `../protocols/`
- **Shared Utilities** - `../shared/`
- **Mission System** - `../mission_system/`
- **Avionics** - `../avionics/`
