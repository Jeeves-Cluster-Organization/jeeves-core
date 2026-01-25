# Events Module

**Module:** `control_tower.events`  
**Main Class:** `EventAggregator`

---

## Overview

The events module implements the kernel's event and interrupt handling system, analogous to an OS interrupt handler. It provides:

- **Process Interrupts** - Like software interrupts (clarification, confirmation)
- **Kernel Events** - Like hardware interrupts (state changes, resource events)
- **Event History** - Like dmesg (ring buffer for debugging)

---

## EventAggregator Class

### Purpose

The `EventAggregator` manages kernel events and process interrupts:

- Interrupt queue per process (only ONE interrupt pending at a time, like CPU)
- Event bus for kernel events with subscriber notification
- Event history as a ring buffer for debugging and monitoring

### Constructor

```python
class EventAggregator(EventAggregatorProtocol):
    def __init__(
        self,
        logger: LoggerProtocol,
        history_size: int = 10000,
    ) -> None:
```

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `logger` | `LoggerProtocol` | - | Logger instance |
| `history_size` | `int` | 10000 | Max events to keep in history |

---

## Interrupt Management

### raise_interrupt

Raise an interrupt for a process. Only one interrupt can be pending at a time.

```python
def raise_interrupt(
    self,
    pid: str,
    interrupt_type: InterruptKind,
    data: Dict[str, Any],
) -> None:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `pid` | `str` | Process ID |
| `interrupt_type` | `InterruptKind` | Type of interrupt |
| `data` | `Dict[str, Any]` | Interrupt-specific data |

**Behavior:**
- If an interrupt already exists for the process, it's overwritten with a warning log
- Emits a kernel event for the interrupt

**Example:**

```python
aggregator.raise_interrupt(
    pid="env-123",
    interrupt_type=InterruptKind.CLARIFICATION,
    data={"question": "Which file should I analyze?"},
)
```

---

### get_pending_interrupt

Get the pending interrupt for a process.

```python
def get_pending_interrupt(
    self,
    pid: str,
) -> Optional[Tuple[InterruptKind, Dict[str, Any]]]:
```

**Returns:** Tuple of `(interrupt_type, data)` or `None` if no interrupt pending.

**Example:**

```python
interrupt = aggregator.get_pending_interrupt("env-123")
if interrupt:
    interrupt_type, data = interrupt
    question = data.get("question")
```

---

### clear_interrupt

Clear the pending interrupt for a process.

```python
def clear_interrupt(self, pid: str) -> bool:
```

**Returns:** `True` if an interrupt was cleared, `False` if none existed.

---

### has_pending_interrupt

Check if a process has a pending interrupt.

```python
def has_pending_interrupt(self, pid: str) -> bool:
```

---

### get_all_pending_interrupts

Get all pending interrupts across all processes.

```python
def get_all_pending_interrupts(self) -> Dict[str, InterruptKind]:
```

**Returns:** Dictionary mapping PID to interrupt type.

---

## Event Emission

### emit_event

Emit a kernel event to all subscribers.

```python
def emit_event(self, event: KernelEvent) -> None:
```

**Behavior:**
1. Add event to global history (ring buffer)
2. Add to per-process history if applicable
3. Update event counters
4. Notify all type-specific subscribers
5. Notify all wildcard subscribers

**Example:**

```python
aggregator.emit_event(
    KernelEvent.process_state_changed(
        pid="env-123",
        old_state=ProcessState.READY,
        new_state=ProcessState.RUNNING,
    )
)
```

---

## Event Subscription

### subscribe

Subscribe to kernel events of a specific type or all events.

```python
def subscribe(
    self,
    event_type: str,
    handler: EventHandler,
) -> None:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `event_type` | `str` | Event type to subscribe to, or `"*"` for all events |
| `handler` | `Callable[[KernelEvent], None]` | Handler function |

**Example:**

```python
def on_state_change(event: KernelEvent):
    print(f"Process {event.pid} changed to {event.data['new_state']}")

aggregator.subscribe("process.state_changed", on_state_change)

# Subscribe to all events
aggregator.subscribe("*", log_all_events)
```

---

### unsubscribe

Unsubscribe from kernel events.

```python
def unsubscribe(
    self,
    event_type: str,
    handler: EventHandler,
) -> None:
```

---

### get_subscriber_count

Get the number of subscribers for an event type.

```python
def get_subscriber_count(self, event_type: str) -> int:
```

---

## Event History

### get_event_history

Get event history with optional filtering.

```python
def get_event_history(
    self,
    pid: Optional[str] = None,
    event_type: Optional[str] = None,
    limit: int = 100,
) -> List[KernelEvent]:
```

**Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `pid` | `str` | `None` | Filter by process |
| `event_type` | `str` | `None` | Filter by event type |
| `limit` | `int` | 100 | Max events to return |

**Returns:** List of events, newest first.

**Example:**

```python
# Get last 50 state changes for a process
events = aggregator.get_event_history(
    pid="env-123",
    event_type="process.state_changed",
    limit=50,
)
```

---

### get_recent_events

Get events from the last N seconds.

```python
def get_recent_events(
    self,
    seconds: float = 60.0,
) -> List[KernelEvent]:
```

---

### get_history_size

Get current history size (number of events in buffer).

```python
def get_history_size(self) -> int:
```

---

## Utility Methods

### get_event_counts

Get event counts by type.

```python
def get_event_counts(self) -> Dict[str, int]:
```

**Returns:**

```python
{
    "process.created": 150,
    "process.state_changed": 9000,
    "interrupt.raised": 50,
    "resource.exhausted": 5,
}
```

---

### cleanup_process

Clean up all data for a terminated process.

```python
def cleanup_process(self, pid: str) -> None:
```

**Cleans:**
- Pending interrupts
- Per-process event history

---

## KernelEvent Types

The `KernelEvent` dataclass provides factory methods for common event types:

### process_created

```python
KernelEvent.process_created(pid="env-123", request_id="req-456")
```

Emits: `process.created`

### process_state_changed

```python
KernelEvent.process_state_changed(
    pid="env-123",
    old_state=ProcessState.READY,
    new_state=ProcessState.RUNNING,
)
```

Emits: `process.state_changed`

### interrupt_raised

```python
KernelEvent.interrupt_raised(
    pid="env-123",
    interrupt_type=InterruptKind.CLARIFICATION,
    data={"question": "Which file?"},
)
```

Emits: `interrupt.raised`

### resource_exhausted

```python
KernelEvent.resource_exhausted(
    pid="env-123",
    resource="max_llm_calls",
    usage=10,
    quota=10,
)
```

Emits: `resource.exhausted`

---

## Thread Safety

The `EventAggregator` is thread-safe using `threading.RLock`:

- All state modifications are protected by the lock
- Handlers are called outside the lock to avoid deadlocks
- Handler errors are caught and logged, never propagated

---

## Protocol

```python
@runtime_checkable
class EventAggregatorProtocol(Protocol):
    def raise_interrupt(
        self,
        pid: str,
        interrupt_type: InterruptKind,
        data: Dict[str, Any],
    ) -> None: ...
    
    def get_pending_interrupt(
        self,
        pid: str,
    ) -> Optional[tuple[InterruptKind, Dict[str, Any]]]: ...
    
    def clear_interrupt(self, pid: str) -> bool: ...
    
    def emit_event(self, event: KernelEvent) -> None: ...
    
    def subscribe(
        self,
        event_type: str,
        handler: Any,
    ) -> None: ...
    
    def unsubscribe(
        self,
        event_type: str,
        handler: Any,
    ) -> None: ...
    
    def get_event_history(
        self,
        pid: Optional[str] = None,
        event_type: Optional[str] = None,
        limit: int = 100,
    ) -> List[KernelEvent]: ...
```

---

## Navigation

- [← Kernel](./kernel.md)
- [Back to README](./README.md)
- [IPC →](./ipc.md)
