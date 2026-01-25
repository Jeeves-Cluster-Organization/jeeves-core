# mission_system.services - Application Services

**Package:** `mission_system.services`  
**Purpose:** Business logic and application services  
**Updated:** 2026-01-23

---

## Overview

The `services` package provides application-level services for:
- Chat session and message management
- Time-travel debugging
- Distributed worker coordination

### Unified Interrupt System (v4.0)

Note: `ConfirmationCoordinator` has been **removed**. All interrupt handling now goes through `control_tower.services.InterruptService`.

---

## Module Index

| Module | Description |
|--------|-------------|
| `__init__.py` | Package exports |
| `chat_service.py` | `ChatService` - Session and message management |
| `debug_api.py` | `DebugAPIService` - Time-travel debugging |
| `worker_coordinator.py` | `WorkerCoordinator` - Distributed execution |

---

## ChatService (`chat_service.py`)

Centralized chat operations with WebSocket event broadcasting.

### Class Definition

```python
class ChatService:
    """
    Centralized chat operations service.
    
    Provides session and message management for the chat UI.
    """
    
    def __init__(
        self,
        db: DatabaseClientProtocol,
        event_manager: Optional[WebSocketEventManager] = None,
        logger: Optional[LoggerProtocol] = None,
    )
```

### Session Operations

| Method | Description |
|--------|-------------|
| `create_session(user_id, title)` | Create a new chat session |
| `list_sessions(user_id, filter, ...)` | List sessions for user |
| `get_session(session_id, user_id)` | Get single session by ID |
| `update_session(session_id, user_id, updates)` | Update session (rename, archive) |
| `delete_session(session_id, user_id, soft)` | Delete session (soft/hard) |

### Message Operations

| Method | Description |
|--------|-------------|
| `list_messages(session_id, user_id, ...)` | List messages in session |
| `delete_message(message_id, user_id, soft)` | Delete a message |
| `edit_message(message_id, user_id, new_content)` | Edit user message |

### Search & Export

| Method | Description |
|--------|-------------|
| `search_messages(user_id, query, limit)` | Full-text search across messages |
| `export_session(session_id, user_id, format)` | Export session (json/txt/md) |

### Event Broadcasting

All mutations broadcast events via WebSocketEventManager:
- `chat.session.created`
- `chat.session.updated`
- `chat.session.deleted`
- `chat.message.deleted`
- `chat.message.edited`

---

## DebugAPIService (`debug_api.py`)

Time-travel debugging support (Constitutional Amendment XXIII).

### Class Definition

```python
class DebugAPIService:
    """
    Service providing debug inspection and replay capabilities.
    """
    
    def __init__(
        self,
        checkpoint_adapter: CheckpointProtocol,
        logger: Optional[LoggerProtocol] = None,
    )
```

### Data Types

```python
@dataclass
class ExecutionTimeline:
    """Timeline of execution checkpoints for a request."""
    envelope_id: str
    checkpoints: List[CheckpointRecord]
    current_stage: int
    is_terminal: bool
    terminal_reason: Optional[str]

@dataclass
class InspectionResult:
    """Result of inspecting a checkpoint."""
    checkpoint_id: str
    envelope_state: Dict[str, Any]
    agent_name: str
    stage_order: int
    outputs: Dict[str, Any]
    processing_records: List[Dict[str, Any]]

@dataclass
class ReplayResult:
    """Result of replaying from a checkpoint."""
    original_checkpoint_id: str
    forked_envelope_id: str
    new_checkpoint_id: str
    envelope: Optional[Envelope]
```

### Methods

| Method | Description |
|--------|-------------|
| `get_timeline(envelope_id)` | Get execution timeline for a request |
| `inspect_checkpoint(checkpoint_id)` | Inspect a specific checkpoint's state |
| `replay_from_checkpoint(checkpoint_id, ...)` | Create replay branch from checkpoint |
| `compare_executions(envelope_id_a, envelope_id_b)` | Compare two execution timelines |
| `cleanup_old_checkpoints(envelope_id, keep_last_n)` | Clean up old checkpoints |

### Usage

```python
debug = DebugAPIService(checkpoint_adapter)

# Get execution timeline
timeline = await debug.get_timeline("env_123")

# Inspect specific checkpoint
result = await debug.inspect_checkpoint("ckpt_abc")

# Replay from checkpoint with modifications
replay = await debug.replay_from_checkpoint(
    "ckpt_abc",
    modifications={"user_message": "different input"},
)
```

---

## WorkerCoordinator (`worker_coordinator.py`)

Distributed worker coordination (Constitutional Amendment XXIV).

### Data Types

```python
@dataclass
class WorkerConfig:
    """Configuration for a distributed worker."""
    worker_id: str
    queues: List[str]
    max_concurrent_tasks: int = 5
    heartbeat_interval_seconds: int = 30
    task_timeout_seconds: int = 300

@dataclass
class WorkerStatus:
    """Current status of a worker."""
    worker_id: str
    status: str  # starting, running, stopping, stopped
    active_tasks: int
    completed_tasks: int
    failed_tasks: int
    last_heartbeat: Optional[datetime]
    queues: List[str]
```

### Class Definition

```python
class WorkerCoordinator:
    """
    Coordinates distributed workers for pipeline execution.
    
    Control Tower Integration:
    - Creates PCB for each submitted envelope
    - Allocates resource quotas
    - Tracks resource usage from workers
    - Reports lifecycle events
    """
    
    def __init__(
        self,
        distributed_bus: DistributedBusProtocol,
        checkpoint_adapter: Optional[CheckpointProtocol] = None,
        runtime: Optional[Runtime] = None,
        logger: Optional[LoggerProtocol] = None,
        control_tower: Optional[ControlTowerProtocol] = None,
    )
```

### Methods

| Method | Description |
|--------|-------------|
| `submit_envelope(envelope, queue, ...)` | Submit envelope for distributed processing |
| `run_worker(config, agent_handler)` | Run as a distributed worker |
| `stop_worker(worker_id)` | Signal worker to stop gracefully |
| `get_worker_status(worker_id)` | Get status of a specific worker |
| `list_workers()` | List all known workers |
| `get_queue_stats(queue_name)` | Get statistics for a queue |
| `list_queues()` | List all active queues |

### DistributedPipelineRunner

Higher-level abstraction for distributed pipeline execution.

```python
class DistributedPipelineRunner:
    """
    Runs pipelines across distributed workers.
    """
    
    def __init__(
        self,
        coordinator: WorkerCoordinator,
        pipeline_config: PipelineConfig,
        logger: Optional[LoggerProtocol] = None,
    )
    
    async def run(self, envelope: Envelope) -> str:
        """Run pipeline on envelope, returns task ID."""
```

### Usage

```python
coordinator = WorkerCoordinator(
    distributed_bus=redis_bus,
    checkpoint_adapter=postgres_checkpoint,
    runtime=unified_runtime,
    control_tower=control_tower,
)

# Submit work
task_id = await coordinator.submit_envelope(envelope, "agent:planner")

# Or run as worker
await coordinator.run_worker(WorkerConfig(
    worker_id="worker-1",
    queues=["agent:*"],
))
```

---

## All Exports

```python
from mission_system.services import (
    # Chat Service
    ChatService,
    
    # Amendment XXIII: Time-Travel Debugging
    DebugAPIService,
    ExecutionTimeline,
    InspectionResult,
    ReplayResult,
    
    # Amendment XXIV: Horizontal Scaling
    WorkerCoordinator,
    WorkerConfig,
    WorkerStatus,
    DistributedPipelineRunner,
)
```

---

## Navigation

- [← Back to README](README.md)
- [← API Documentation](api.md)
- [← Orchestrator Documentation](orchestrator.md)
- [← Events Documentation](events.md)
