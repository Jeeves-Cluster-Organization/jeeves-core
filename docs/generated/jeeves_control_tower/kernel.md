# Control Tower Kernel

**Module:** `control_tower.kernel`  
**Protocol:** `control_tower.protocols`

---

## Overview

The `ControlTower` class is the main kernel entry point that composes all subsystems:

- **LifecycleManager** - Process scheduler
- **ResourceTracker** - Resource quotas (cgroups)
- **CommBusCoordinator** - IPC manager
- **EventAggregator** - Interrupt handler
- **InterruptService** - Unified interrupt handling

---

## ControlTower Class

### Constructor

```python
class ControlTower(ControlTowerProtocol):
    def __init__(
        self,
        logger: LoggerProtocol,
        default_quota: Optional[ResourceQuota] = None,
        default_service: str = "flow_service",
        db: Optional[Any] = None,
        webhook_service: Optional[Any] = None,
        otel_adapter: Optional[Any] = None,
    ) -> None:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `logger` | `LoggerProtocol` | Logger instance for structured logging |
| `default_quota` | `ResourceQuota` | Default resource quota for requests |
| `default_service` | `str` | Default service to dispatch to |
| `db` | `Any` | Optional database for interrupt persistence |
| `webhook_service` | `Any` | Optional webhook service for interrupt events |
| `otel_adapter` | `Any` | Optional OpenTelemetry adapter for tracing |

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `lifecycle` | `LifecycleManagerProtocol` | Process lifecycle manager |
| `resources` | `ResourceTrackerProtocol` | Resource tracker |
| `ipc` | `CommBusCoordinatorProtocol` | IPC coordinator |
| `events` | `EventAggregatorProtocol` | Event aggregator |
| `interrupts` | `InterruptService` | Interrupt service |

---

## Main Methods

### submit_request

Submit a request for processing. This is the main entry point for all requests.

```python
async def submit_request(
    self,
    envelope: Envelope,
    priority: SchedulingPriority = SchedulingPriority.NORMAL,
    quota: Optional[ResourceQuota] = None,
) -> Envelope:
```

**Flow:**
1. Create PCB (Process Control Block)
2. Emit `process.created` event
3. Allocate resources
4. Schedule for execution (NEW → READY)
5. Dispatch to service (READY → RUNNING)
6. Handle result/interrupt
7. Return completed envelope

**Example:**

```python
envelope = Envelope(
    user_input="Analyze authentication flow",
    user_id="user-123",
    session_id="session-456",
)

result = await kernel.submit_request(
    envelope,
    priority=SchedulingPriority.HIGH,
    quota=ResourceQuota(max_llm_calls=20),
)
```

---

### resume_request

Resume a waiting request after an interrupt is resolved.

```python
async def resume_request(
    self,
    pid: str,
    interrupt_id: str,
    response: InterruptResponse,
) -> Envelope:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `pid` | `str` | Process ID (envelope_id) |
| `interrupt_id` | `str` | ID of the interrupt being responded to |
| `response` | `InterruptResponse` | Response data (text, approved, decision, etc.) |

**Flow:**
1. Get the PCB for the process
2. Resolve the interrupt via InterruptService
3. Clear kernel-level interrupt
4. Transition WAITING → READY
5. Re-execute the process

**Example:**

```python
result = await kernel.resume_request(
    pid="env-123",
    interrupt_id="int-456",
    response=InterruptResponse(text="Yes, analyze the main module"),
)
```

---

### get_request_status

Get the current status of a request.

```python
def get_request_status(self, pid: str) -> Optional[Dict[str, Any]]:
```

**Returns:**

```python
{
    "pid": "env-123",
    "state": "running",
    "priority": "normal",
    "current_stage": "executing",
    "created_at": "2026-01-23T10:00:00Z",
    "started_at": "2026-01-23T10:00:01Z",
    "usage": {
        "llm_calls": 3,
        "tool_calls": 5,
        "elapsed_seconds": 45.2,
    },
    "remaining": {
        "llm_calls": 7,
        "tool_calls": 45,
        "time_seconds": 254.8,
    },
    "has_interrupt": False,
    "interrupt_type": None,
}
```

---

### cancel_request

Cancel a running request.

```python
async def cancel_request(self, pid: str, reason: str) -> bool:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `pid` | `str` | Process ID |
| `reason` | `str` | Cancellation reason |

**Returns:** `True` if cancelled successfully

---

## Resource Tracking Helpers

### record_llm_call

Record an LLM call for a process.

```python
def record_llm_call(
    self,
    pid: str,
    tokens_in: int = 0,
    tokens_out: int = 0,
) -> Optional[str]:
```

**Returns:** Quota exceeded reason if any, `None` otherwise.

### record_tool_call

```python
def record_tool_call(self, pid: str) -> Optional[str]:
```

### record_agent_hop

```python
def record_agent_hop(self, pid: str) -> Optional[str]:
```

---

## Service Registration

### register_service

Register a service with the kernel.

```python
def register_service(
    self,
    name: str,
    service_type: str,
    handler: Any,
    capabilities: Optional[list] = None,
    max_concurrent: int = 10,
) -> bool:
```

**Example:**

```python
kernel.register_service(
    name="code_analysis",
    service_type="flow",
    handler=analysis_service.handle,
    capabilities=["analyze", "explain", "refactor"],
    max_concurrent=5,
)
```

---

## System Metrics

### get_system_status

Get overall system status.

```python
def get_system_status(self) -> Dict[str, Any]:
```

**Returns:**

```python
{
    "processes": {
        "total": 150,
        "by_state": {
            "new": 2,
            "ready": 5,
            "running": 10,
            "waiting": 3,
            "terminated": 130,
        },
        "queue_depth": 5,
    },
    "resources": {
        "total_processes": 150,
        "active_processes": 20,
        "system_llm_calls": 1500,
        "system_tool_calls": 5000,
        "system_tokens_in": 500000,
        "system_tokens_out": 200000,
    },
    "events": {
        "total": 10000,
        "by_type": {"process.created": 150, "state.changed": 9000, ...},
        "history_size": 10000,
    },
    "services": {
        "total": 3,
        "healthy": 3,
        "names": ["flow_service", "worker_service", "vertical_registry"],
    },
}
```

---

## Protocols

### ControlTowerProtocol

The main protocol interface that higher layers use.

```python
@runtime_checkable
class ControlTowerProtocol(Protocol):
    @property
    def lifecycle(self) -> LifecycleManagerProtocol: ...
    
    @property
    def resources(self) -> ResourceTrackerProtocol: ...
    
    @property
    def ipc(self) -> CommBusCoordinatorProtocol: ...
    
    @property
    def events(self) -> EventAggregatorProtocol: ...
    
    async def submit_request(
        self,
        envelope: Envelope,
        priority: SchedulingPriority = SchedulingPriority.NORMAL,
        quota: Optional[ResourceQuota] = None,
    ) -> Envelope: ...
    
    async def resume_request(
        self,
        pid: str,
        interrupt_id: str,
        response: InterruptResponse,
    ) -> Envelope: ...
    
    def get_request_status(self, pid: str) -> Optional[Dict[str, Any]]: ...
    
    async def cancel_request(self, pid: str, reason: str) -> bool: ...
```

---

## Internal Flow: _execute_process

The internal execution loop handles:

1. Get runnable process from ready queue
2. Emit state change event (READY → RUNNING)
3. Create dispatch target for default service
4. Dispatch to service via IPC
5. Check for resource exhaustion
6. Check for pending interrupt
7. Handle interrupts or mark as terminated
8. Return result envelope

```python
async def _execute_process(
    self,
    pid: str,
    envelope: Envelope,
) -> Envelope:
```

---

## Internal Flow: _handle_interrupt

Handles process interrupts by:

1. Transitioning process to WAITING state
2. Emitting state change event
3. Handling terminal interrupts (TIMEOUT, RESOURCE_EXHAUSTED)
4. Creating unified interrupt via InterruptService
5. Setting envelope interrupt state

```python
async def _handle_interrupt(
    self,
    pid: str,
    envelope: Envelope,
    interrupt_kind: InterruptKind,
    interrupt_data: Dict[str, Any],
) -> Envelope:
```

---

## Navigation

- [Back to README](./README.md)
- [Events →](./events.md)
- [IPC →](./ipc.md)
- [Lifecycle →](./lifecycle.md)
- [Resources →](./resources.md)
- [Services →](./services.md)
