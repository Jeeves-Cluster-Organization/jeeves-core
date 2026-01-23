# Lifecycle Module

**Module:** `jeeves_control_tower.lifecycle`  
**Main Class:** `LifecycleManager`

---

## Overview

The lifecycle module implements the kernel's process scheduling system:

- **Process creation and termination**
- **State machine transitions**
- **Priority-based scheduling** (using a heap)
- **Process table management** (like OS process table)

---

## LifecycleManager Class

### Purpose

The `LifecycleManager` is the kernel's process scheduler, managing the lifecycle of all requests ("processes") from submission to completion.

### Constructor

```python
class LifecycleManager(LifecycleManagerProtocol):
    def __init__(
        self,
        logger: LoggerProtocol,
        default_quota: Optional[ResourceQuota] = None,
    ) -> None:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `logger` | `LoggerProtocol` | Logger instance |
| `default_quota` | `ResourceQuota` | Default resource quota for new processes |

---

## Process States

```python
class ProcessState(str, Enum):
    NEW = "new"           # Just created, not yet scheduled
    READY = "ready"       # Ready to run, waiting for CPU
    RUNNING = "running"   # Currently executing
    WAITING = "waiting"   # Waiting for I/O or event (clarification)
    BLOCKED = "blocked"   # Blocked on resource (quota exceeded)
    TERMINATED = "terminated"  # Finished execution
    ZOMBIE = "zombie"     # Terminated but not yet cleaned up
```

### State Transition Diagram

```
         submit()
            │
            ▼
    ┌───────────────┐
    │      NEW      │
    └───────────────┘
            │
            │ schedule()
            ▼
    ┌───────────────┐  ◄───────────────────┐
    │     READY     │                      │
    └───────────────┘                      │
            │                              │
            │ get_next_runnable()          │
            ▼                              │
    ┌───────────────┐                      │
    │    RUNNING    │──────────────────────┤ resume()
    └───────────────┘                      │
         │  │  │                           │
         │  │  └─── wait() ──────► WAITING─┘
         │  │
         │  └─── block() ───────► BLOCKED──┘
         │
         │ terminate()
         ▼
    ┌───────────────┐
    │  TERMINATED   │
    └───────────────┘
            │
            │ cleanup()
            ▼
    ┌───────────────┐
    │    ZOMBIE     │ (removed from table)
    └───────────────┘
```

### Valid Transitions

| From State | Valid Transitions To |
|------------|---------------------|
| NEW | READY, TERMINATED |
| READY | RUNNING, TERMINATED |
| RUNNING | READY (preempted), WAITING, BLOCKED, TERMINATED |
| WAITING | READY, TERMINATED |
| BLOCKED | READY, TERMINATED |
| TERMINATED | ZOMBIE |
| ZOMBIE | (removed - terminal) |

---

## Scheduling Priority

```python
class SchedulingPriority(str, Enum):
    REALTIME = "realtime"  # Highest priority (system critical)
    HIGH = "high"          # User-interactive
    NORMAL = "normal"      # Default
    LOW = "low"            # Background tasks
    IDLE = "idle"          # Only when nothing else to do
```

**Internal Priority Values** (for heap ordering, lower = higher priority):

| Priority | Value |
|----------|-------|
| REALTIME | 0 |
| HIGH | 1 |
| NORMAL | 2 |
| LOW | 3 |
| IDLE | 4 |

---

## Process Management

### submit

Submit a new request for processing. Creates a PCB in NEW state.

```python
def submit(
    self,
    envelope: Envelope,
    priority: SchedulingPriority = SchedulingPriority.NORMAL,
    quota: Optional[ResourceQuota] = None,
) -> ProcessControlBlock:
```

**Behavior:**
- Creates PCB with unique PID (envelope_id)
- Handles duplicate PIDs (returns existing PCB with warning)
- Sets initial state to NEW

**Example:**

```python
pcb = lifecycle.submit(
    envelope=envelope,
    priority=SchedulingPriority.HIGH,
    quota=ResourceQuota(max_llm_calls=20),
)
```

---

### schedule

Schedule a process for execution. Transitions NEW → READY.

```python
def schedule(self, pid: str) -> bool:
```

**Behavior:**
- Validates process exists and is in NEW state
- Adds to ready queue (priority heap)
- Returns `False` if not schedulable

---

### get_next_runnable

Get the next process to run. Returns highest priority READY process.

```python
def get_next_runnable(self) -> Optional[ProcessControlBlock]:
```

**Behavior:**
- Pops from priority heap until finding a valid READY process
- Transitions READY → RUNNING
- Updates `started_at` and `last_scheduled_at` timestamps
- Returns `None` if queue is empty

---

### transition_state

Transition a process to a new state.

```python
def transition_state(
    self,
    pid: str,
    new_state: ProcessState,
    reason: Optional[str] = None,
) -> bool:
```

**Behavior:**
- Validates transition is allowed
- Updates state and timestamps
- Re-adds to ready queue if transitioning to READY
- Logs state change with reason

---

### get_process

Get a process by ID.

```python
def get_process(self, pid: str) -> Optional[ProcessControlBlock]:
```

---

### list_processes

List processes matching criteria.

```python
def list_processes(
    self,
    state: Optional[ProcessState] = None,
    user_id: Optional[str] = None,
) -> List[ProcessControlBlock]:
```

---

### terminate

Terminate a process.

```python
def terminate(
    self,
    pid: str,
    reason: str,
    force: bool = False,
) -> bool:
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `pid` | `str` | Process ID |
| `reason` | `str` | Termination reason |
| `force` | `bool` | Force kill even if running |

---

### cleanup

Clean up a terminated process. Removes from process table.

```python
def cleanup(self, pid: str) -> bool:
```

Only valid for TERMINATED or ZOMBIE states.

---

## Utility Methods

### get_queue_depth

Get number of processes in ready queue.

```python
def get_queue_depth(self) -> int:
```

---

### get_process_count

Get count of processes by state.

```python
def get_process_count(self) -> Dict[ProcessState, int]:
```

**Returns:**

```python
{
    ProcessState.NEW: 2,
    ProcessState.READY: 5,
    ProcessState.RUNNING: 10,
    ProcessState.WAITING: 3,
    ProcessState.TERMINATED: 130,
}
```

---

## ProcessControlBlock

The kernel's metadata about a running "process" (request).

```python
@dataclass
class ProcessControlBlock:
    # Identity
    pid: str                  # Process ID (envelope_id)
    request_id: str
    user_id: str
    session_id: str
    
    # State
    state: ProcessState = ProcessState.NEW
    priority: SchedulingPriority = SchedulingPriority.NORMAL
    
    # Resource tracking
    quota: ResourceQuota = field(default_factory=ResourceQuota)
    usage: ResourceUsage = field(default_factory=ResourceUsage)
    
    # Scheduling
    created_at: datetime
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    last_scheduled_at: Optional[datetime] = None
    
    # Current execution
    current_stage: str = ""
    current_service: str = ""
    
    # Interrupt handling
    pending_interrupt: Optional[InterruptKind] = None
    interrupt_data: Dict[str, Any] = field(default_factory=dict)
    
    # Parent/child relationships
    parent_pid: Optional[str] = None
    child_pids: List[str] = field(default_factory=list)
```

### PCB Methods

```python
def can_schedule(self) -> bool:
    """Check if process can be scheduled (NEW or READY)."""

def is_runnable(self) -> bool:
    """Check if process is runnable (READY)."""

def is_terminated(self) -> bool:
    """Check if process has terminated (TERMINATED or ZOMBIE)."""
```

---

## Thread Safety

The `LifecycleManager` is thread-safe using `threading.RLock`:

- All process table modifications are protected
- Ready queue operations are atomic
- Safe for concurrent access from multiple coroutines

---

## Protocol

```python
@runtime_checkable
class LifecycleManagerProtocol(Protocol):
    def submit(
        self,
        envelope: Envelope,
        priority: SchedulingPriority = SchedulingPriority.NORMAL,
        quota: Optional[ResourceQuota] = None,
    ) -> ProcessControlBlock: ...
    
    def schedule(self, pid: str) -> bool: ...
    
    def get_next_runnable(self) -> Optional[ProcessControlBlock]: ...
    
    def transition_state(
        self,
        pid: str,
        new_state: ProcessState,
        reason: Optional[str] = None,
    ) -> bool: ...
    
    def get_process(self, pid: str) -> Optional[ProcessControlBlock]: ...
    
    def list_processes(
        self,
        state: Optional[ProcessState] = None,
        user_id: Optional[str] = None,
    ) -> List[ProcessControlBlock]: ...
    
    def terminate(
        self,
        pid: str,
        reason: str,
        force: bool = False,
    ) -> bool: ...
    
    def cleanup(self, pid: str) -> bool: ...
```

---

## Usage Example

```python
from jeeves_control_tower.lifecycle import LifecycleManager
from jeeves_control_tower.types import ProcessState, SchedulingPriority

lifecycle = LifecycleManager(logger)

# Submit a new request
pcb = lifecycle.submit(envelope, priority=SchedulingPriority.HIGH)
print(f"Created process {pcb.pid} in state {pcb.state}")

# Schedule for execution
if lifecycle.schedule(pcb.pid):
    print(f"Process {pcb.pid} is now READY")

# Get next process to run
runnable = lifecycle.get_next_runnable()
if runnable:
    print(f"Running process {runnable.pid}")
    
    # Process needs clarification
    lifecycle.transition_state(
        runnable.pid,
        ProcessState.WAITING,
        reason="Awaiting user clarification",
    )
    
    # User responds, resume
    lifecycle.transition_state(
        runnable.pid,
        ProcessState.READY,
    )

# Terminate and cleanup
lifecycle.terminate(pcb.pid, reason="Completed", force=True)
lifecycle.cleanup(pcb.pid)
```

---

## Navigation

- [← IPC](./ipc.md)
- [Back to README](./README.md)
- [Resources →](./resources.md)
