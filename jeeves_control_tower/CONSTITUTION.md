# Jeeves Control Tower Constitution

**Version:** 1.1
**Updated:** 2025-12-13
**Parent:** [Core Constitution](../docs/CONSTITUTION.md)

---

## Overview

This constitution defines the rules for **jeeves_control_tower**—the kernel layer that sits between the Gateway and Mission System, managing request lifecycle, resource allocation, service dispatch, and event streaming.

**Architecture Analogy:** Control Tower is the "kernel" of the Jeeves OS architecture.

| OS Concept | Control Tower Equivalent |
|------------|--------------------------|
| Kernel | ControlTower |
| Process Scheduler | LifecycleManager |
| cgroups | ResourceTracker |
| IPC Manager | CommBusCoordinator |
| Interrupt Handler | EventAggregator |
| Process States | ProcessState enum |
| PCB | ProcessControlBlock |
| System Calls | jeeves_protocols |

---

## Purpose

**Control Tower responsibilities:**
- **Request Lifecycle Management**: Track requests from submission to completion (process scheduling)
- **Resource Quota Enforcement**: Manage LLM tokens, database connections, time budgets (cgroups equivalent)
- **Service Dispatch**: Route requests to registered services via CommBus (IPC equivalent)
- **Event Streaming**: Aggregate and emit events for real-time monitoring (interrupt handling)

**Control Tower does NOT:**
- Implement business logic (Mission System)
- Manage database connections directly (Avionics)
- Execute agent pipelines (Mission System verticals)
- Call LLM APIs directly (Avionics)

---

## Core Principles (Inherited from Core Constitution)

From [Core Constitution](../JEEVES_CORE_CONSTITUTION.md):
- **P1: Accuracy First** — Evidence-based responses
- **P2: Code Context Priority** — Understand before acting
- **P3: Bounded Efficiency** — Graceful degradation

**All core principles apply to this component.**

---

## Component Rules

### R1: Pure Abstraction (Import Boundaries)

Control Tower **ONLY imports from** `jeeves_protocols` and `jeeves_shared`:

```python
# CORRECT: Import from protocols
from jeeves_protocols import GenericEnvelope, ServiceProtocol
from jeeves_protocols import InterruptKind, RateLimitConfig  # Interrupt/rate limit types
from jeeves_shared import get_component_logger  # Shared logging utilities

# FORBIDDEN: Direct imports from other layers
# from jeeves_mission_system import FlowService  # NO
# from jeeves_avionics import DatabaseClient     # NO
# from jeeves_memory_module import MemoryService # NO
```

**Amendment (2025-12):** Interrupt types (InterruptKind, InterruptStatus, FlowInterrupt, etc.)
and rate limit types (RateLimitConfig, RateLimitResult) are defined in `jeeves_protocols.interrupts`
for clean layer separation. Control Tower implements the protocols, avionics consumes them.

**Rationale:** Control Tower is the kernel. It dispatches TO services, not imports FROM them.

### R2: Service Registration Contract

Services must register with Control Tower at startup:

```python
# Service registration pattern
kernel.register_service(
    name="my_capability",
    service_type="flow",
    handler=capability_handler,
    resource_quota=ResourceQuota(
        max_llm_tokens=50000,
        max_time_seconds=300,
    ),
)
```

**Registration requirements:**
- Unique service name
- Service type (flow, worker, vertical)
- Handler function matching `async def handler(envelope: GenericEnvelope) -> EnvelopeResult`
- Optional resource quota overrides

### R3: Request Lifecycle States

All requests transition through defined states:

```
PENDING → READY → RUNNING → (SUSPENDED) → COMPLETED/FAILED
                     ↓
              WAITING_CLARIFICATION
```

| State | Description |
|-------|-------------|
| `PENDING` | Submitted, awaiting scheduling |
| `READY` | Resources allocated, ready to dispatch |
| `RUNNING` | Actively being processed by service |
| `SUSPENDED` | Paused (e.g., awaiting clarification) |
| `WAITING_CLARIFICATION` | Blocked on user input |
| `COMPLETED` | Successfully finished |
| `FAILED` | Error occurred, cleanup done |

**State transitions are logged** via EventAggregator.

### R4: Resource Quota Enforcement

ResourceTracker enforces quotas BEFORE dispatch:

```python
# Quota check before dispatch
async def submit_request(self, envelope: GenericEnvelope) -> EnvelopeResult:
    # 1. Check quotas
    if not self.resource_tracker.can_allocate(envelope):
        raise ResourceQuotaExceeded(
            "Insufficient quota for request"
        )

    # 2. Allocate resources
    self.resource_tracker.allocate(envelope)

    # 3. Dispatch to service
    result = await self.dispatch(envelope)

    # 4. Release resources
    self.resource_tracker.release(envelope)

    return result
```

**Default quotas:**
- `max_llm_tokens`: 50,000 per request
- `max_time_seconds`: 300 (5 minutes)
- `max_concurrent_requests`: 10 per user

### R5: Event Emission Contract

EventAggregator emits events for all significant operations:

```python
# Required events
class KernelEvent:
    REQUEST_SUBMITTED = "request.submitted"
    REQUEST_DISPATCHED = "request.dispatched"
    REQUEST_COMPLETED = "request.completed"
    REQUEST_FAILED = "request.failed"

    STATE_CHANGED = "state.changed"
    QUOTA_WARNING = "quota.warning"
    QUOTA_EXCEEDED = "quota.exceeded"

    CLARIFICATION_REQUESTED = "clarification.requested"
    CLARIFICATION_RECEIVED = "clarification.received"
```

**Event payload requirements:**
- `envelope_id`: Request identifier
- `timestamp`: ISO 8601 timestamp
- `event_type`: From KernelEvent enum
- `payload`: Event-specific data

### R6: Clarification Protocol

When a service requires user clarification:

```python
# Service signals clarification needed
async def handle_clarification(
    self,
    pid: str,
    clarification_type: str,
    question: str,
) -> None:
    # 1. Transition state to WAITING_CLARIFICATION
    self.lifecycle_manager.transition(pid, ProcessState.WAITING_CLARIFICATION)

    # 2. Emit clarification event
    self.event_aggregator.emit(KernelEvent.CLARIFICATION_REQUESTED, {
        "pid": pid,
        "question": question,
        "clarification_type": clarification_type,
    })

    # 3. Suspend request (preserve state)
    # Request remains suspended until resume_request() called
```

**Resume pattern:**
```python
result = await kernel.resume_request(
    pid=request_id,
    response_data={"clarification_response": user_response},
)
```

---

## Subsystem Specifications

### LifecycleManager

Manages ProcessControlBlock (PCB) for each request:

```python
@dataclass
class ProcessControlBlock:
    pid: str                    # Process/request ID
    state: ProcessState         # Current state
    priority: SchedulingPriority
    envelope: GenericEnvelope   # Request data
    resource_usage: ResourceUsage
    created_at: datetime
    updated_at: datetime
    service_name: str           # Target service
```

### ResourceTracker

Tracks and enforces resource quotas:

```python
@dataclass
class ResourceQuota:
    max_llm_tokens: int = 50000
    max_time_seconds: int = 300
    max_concurrent_requests: int = 10

@dataclass
class ResourceUsage:
    llm_tokens_used: int = 0
    time_elapsed_seconds: float = 0.0
    concurrent_requests: int = 0
```

### CommBusCoordinator

Routes requests to registered services:

```python
@dataclass
class ServiceDescriptor:
    name: str
    service_type: str
    handler: Callable[[GenericEnvelope], Awaitable[EnvelopeResult]]
    resource_quota: Optional[ResourceQuota] = None
```

### EventAggregator

Aggregates and streams events:

```python
# Event subscription pattern
async def subscribe(
    self,
    event_types: List[str],
    callback: Callable[[KernelEvent], Awaitable[None]],
) -> str:
    """Subscribe to kernel events. Returns subscription ID."""
```

---

## Usage Examples

### Basic Request Flow

```python
from jeeves_control_tower import ControlTower
from jeeves_protocols import GenericEnvelope

# 1. Create kernel
kernel = ControlTower(logger=logger)

# 2. Register services (at startup)
kernel.register_service(
    name="my_capability",
    service_type="flow",
    handler=capability_service.handle,
)

# 3. Submit request
envelope = GenericEnvelope(
    user_input="How does authentication work?",
    user_id="user123",
    session_id="session456",
)
result = await kernel.submit_request(envelope)

# 4. Handle clarification if needed
if result.clarification_pending:
    user_response = await get_user_input(result.clarification_question)
    result = await kernel.resume_request(
        pid=result.envelope_id,
        response_data={"clarification_response": user_response},
    )
```

### Event Streaming

```python
# Subscribe to events
async def event_handler(event: KernelEvent):
    await websocket.send_json({
        "type": event.event_type,
        "data": event.payload,
    })

subscription_id = await kernel.event_aggregator.subscribe(
    event_types=[KernelEvent.STATE_CHANGED, KernelEvent.REQUEST_COMPLETED],
    callback=event_handler,
)
```

---

## Cross-References

- **Core Constitution:** [../docs/CONSTITUTION.md](../docs/CONSTITUTION.md)
- **Protocols:** [../jeeves_protocols/](../jeeves_protocols/)
- **Shared Utilities:** [../jeeves_shared/](../jeeves_shared/)
- **Mission System:** [../jeeves_mission_system/CONSTITUTION.md](../jeeves_mission_system/CONSTITUTION.md)
- **Avionics:** [../jeeves_avionics/CONSTITUTION.md](../jeeves_avionics/CONSTITUTION.md)
- **Capability:** [../jeeves-capability-code-analyser/CONSTITUTION.md](../jeeves-capability-code-analyser/CONSTITUTION.md)

---

*This constitution defines the kernel layer contract. Control Tower manages what runs, when, and with what resources—but never implements domain logic.*
