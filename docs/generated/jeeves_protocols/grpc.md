# jeeves_protocols.grpc_client

**Layer**: L0 (Foundation)  
**Purpose**: gRPC client for Go runtime

## Overview

This module provides the gRPC client for communicating with the Go runtime. The Go gRPC server is **REQUIRED** - there are no fallbacks. If the Go server is not running, operations fail immediately.

## Constitutional Compliance

- **Go-Only Mode** (HANDOFF.md): All operations require Go server
- **Contract 11** (CONTRACTS.md): Envelope round-trip must be lossless
- **Contract 12** (CONTRACTS.md): Go is authoritative for bounds checking

## Exceptions

### GrpcConnectionError

Raised when connection to Go server fails.

```python
class GrpcConnectionError(Exception):
    """Raised when connection to Go server fails."""
    pass
```

---

### GrpcCallError

Raised when a gRPC call fails.

```python
class GrpcCallError(Exception):
    """Raised when a gRPC call fails."""
    pass
```

---

### GoServerNotRunningError

Raised when Go gRPC server is not running.

```python
class GoServerNotRunningError(Exception):
    """Raised when Go gRPC server is not running."""
    pass
```

---

## Result Types

### BoundsResult

Result from bounds check - Go is authoritative.

```python
@dataclass
class BoundsResult:
    can_continue: bool                   # Whether pipeline can continue
    terminal_reason: Optional[str]       # Reason if terminated
    llm_calls_remaining: int             # Remaining LLM calls
    agent_hops_remaining: int            # Remaining agent hops
    iterations_remaining: int            # Remaining iterations
```

---

### ExecutionEvent

Event from pipeline execution stream.

```python
@dataclass
class ExecutionEvent:
    type: str                            # Event type (STAGE_STARTED, etc.)
    stage: str                           # Stage name
    timestamp_ms: int                    # Timestamp in milliseconds
    payload: Optional[Dict[str, Any]]    # Event-specific data
    envelope: Optional[Envelope]  # Updated envelope
```

**Event Types**:
- `STAGE_STARTED`
- `STAGE_COMPLETED`
- `STAGE_FAILED`
- `PIPELINE_COMPLETED`
- `INTERRUPT_RAISED`
- `CHECKPOINT_CREATED`
- `BOUNDS_EXCEEDED`

---

### AgentResult

Result from single agent execution.

```python
@dataclass
class AgentResult:
    success: bool                        # Whether execution succeeded
    output: Optional[Dict[str, Any]]     # Agent output
    error: Optional[str]                 # Error message if failed
    duration_ms: int                     # Execution duration
    llm_calls: int                       # LLM calls made
    envelope: Optional[Envelope]  # Updated envelope
```

---

## GrpcGoClient

gRPC client for Go runtime.

```python
class GrpcGoClient:
    DEFAULT_ADDRESS = "localhost:50051"
    CONNECT_TIMEOUT = 5.0
```

### Constructor

```python
def __init__(self, address: Optional[str] = None, timeout: float = 30.0):
    """Initialize gRPC client.
    
    Args:
        address: gRPC server address (default: localhost:50051)
        timeout: Default timeout for RPC calls in seconds
    """
```

### Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `connect` | `() -> None` | Establish connection to Go server |
| `close` | `() -> None` | Close the gRPC channel |
| `create_envelope` | `(raw_input, user_id, session_id, ...) -> Envelope` | Create envelope via Go runtime |
| `update_envelope` | `(envelope, agent_name, output, llm_calls_made) -> Envelope` | Update envelope after agent execution |
| `check_bounds` | `(envelope) -> BoundsResult` | Check bounds - Go is authoritative |
| `clone_envelope` | `(envelope) -> Envelope` | Clone envelope via Go |
| `execute_agent` | `(envelope, agent_name, agent_config) -> AgentResult` | Execute a single agent via Go |
| `execute_pipeline` | `(envelope, thread_id, pipeline_config) -> Iterator[ExecutionEvent]` | Execute pipeline with streaming events |

### Context Manager Support

```python
with GrpcGoClient() as client:
    envelope = client.create_envelope(...)
    result = client.check_bounds(envelope)
```

---

## Usage Examples

### Basic Connection

```python
from jeeves_protocols.grpc_client import GrpcGoClient, GoServerNotRunningError

try:
    client = GrpcGoClient(address="localhost:50051")
    client.connect()
except GoServerNotRunningError:
    print("Start Go server: go run cmd/envelope/main.go")
    raise
```

### Create and Update Envelope

```python
# Create envelope via Go
envelope = client.create_envelope(
    raw_input="Analyze authentication module",
    user_id="user-123",
    session_id="session-456",
    stage_order=["perception", "intent", "planner", "executor"],
)

# Check bounds before processing
bounds = client.check_bounds(envelope)
if not bounds.can_continue:
    print(f"Cannot continue: {bounds.terminal_reason}")

# Update after agent processing
envelope = client.update_envelope(
    envelope=envelope,
    agent_name="planner",
    output={"steps": ["step1", "step2"]},
    llm_calls_made=1,
)
```

### Streaming Pipeline Execution

```python
for event in client.execute_pipeline(envelope, thread_id="thread-123"):
    if event.type == "STAGE_STARTED":
        print(f"Starting: {event.stage}")
    elif event.type == "STAGE_COMPLETED":
        print(f"Completed: {event.stage}")
    elif event.type == "BOUNDS_EXCEEDED":
        print(f"Bounds exceeded!")
        break
```

---

## Convenience Functions

Module-level singleton and convenience functions:

```python
def get_client(address: Optional[str] = None) -> GrpcGoClient:
    """Get or create the default gRPC client singleton."""

def create_envelope(...) -> Envelope:
    """Create envelope via Go gRPC (convenience function)."""

def update_envelope(...) -> Envelope:
    """Update envelope via Go gRPC (convenience function)."""

def check_bounds(envelope: Envelope) -> BoundsResult:
    """Check bounds via Go gRPC (convenience function)."""

def clone_envelope(envelope: Envelope) -> Envelope:
    """Clone envelope via Go gRPC (convenience function)."""
```

---

## grpc_stub Module

Re-exports from generated protobuf classes.

```python
from jeeves_protocols import grpc_stub

# Messages
grpc_stub.CreateEnvelopeRequest
grpc_stub.UpdateEnvelopeRequest
grpc_stub.CloneRequest
grpc_stub.Envelope
grpc_stub.BoundsResult
grpc_stub.ExecuteRequest
grpc_stub.ExecuteAgentRequest
grpc_stub.ExecutionEvent
grpc_stub.AgentResult
grpc_stub.FlowInterrupt
grpc_stub.InterruptResponse

# Enums
grpc_stub.TerminalReason
grpc_stub.InterruptKind
grpc_stub.ExecutionEventType

# Service
grpc_stub.EngineServiceStub
grpc_stub.EngineServiceServicer
grpc_stub.add_EngineServiceServicer_to_server
```

---

## Navigation

- [Back to README](README.md)
- [Previous: Interrupts](interrupts.md)
- [Next: Utils](utils.md)
