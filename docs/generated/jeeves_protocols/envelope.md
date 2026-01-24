# jeeves_protocols.envelope

**Layer**: L0 (Foundation)  
**Purpose**: Envelope types - mirrors Go coreengine/envelope

## Overview

This module provides the `Envelope` class, the primary state container for pipeline execution. It mirrors the Go implementation in `coreengine/envelope.go` for cross-language compatibility.

## Dataclasses

### ProcessingRecord

Record of an agent processing step within the pipeline.

```python
@dataclass
class ProcessingRecord:
    agent: str                              # Agent name
    stage_order: int                        # Order in pipeline
    started_at: datetime                    # When processing started
    completed_at: Optional[datetime] = None # When processing completed
    duration_ms: int = 0                    # Processing duration
    status: str = "running"                 # running, success, error, skipped
    error: Optional[str] = None             # Error message if failed
    llm_calls: int = 0                      # Number of LLM calls made
```

---

### Envelope

Primary state container for pipeline execution. This is a Python mirror of Go's `Envelope`.

```python
@dataclass
class Envelope:
    # ─── Identification ───
    envelope_id: str = ""           # Unique envelope identifier
    request_id: str = ""            # Request correlation ID
    user_id: str = ""               # User identifier
    session_id: str = ""            # Session identifier
    
    # ─── Input ───
    raw_input: str = ""             # Original user input
    received_at: Optional[datetime] = None
    
    # ─── Dynamic Outputs ───
    outputs: Dict[str, Dict[str, Any]] = field(default_factory=dict)
    
    # ─── Pipeline State ───
    current_stage: str = "start"    # Current pipeline stage
    stage_order: List[str] = field(default_factory=list)
    iteration: int = 0              # Current iteration count
    max_iterations: int = 3         # Maximum iterations allowed
    
    # ─── Bounds ───
    llm_call_count: int = 0         # Total LLM calls made
    max_llm_calls: int = 10         # Maximum LLM calls allowed
    agent_hop_count: int = 0        # Total agent hops
    max_agent_hops: int = 21        # Maximum agent hops allowed
    terminal_reason: Optional[TerminalReason] = None
    
    # ─── Control Flow ───
    terminated: bool = False
    termination_reason: Optional[str] = None
    
    # ─── Interrupt Handling ───
    interrupt_pending: bool = False
    interrupt: Optional[FlowInterrupt] = None
    
    # ─── Parallel Execution State ───
    active_stages: Dict[str, bool] = field(default_factory=dict)
    completed_stage_set: Dict[str, bool] = field(default_factory=dict)
    failed_stages: Dict[str, str] = field(default_factory=dict)
    parallel_mode: bool = False
    
    # ─── Multi-Stage ───
    completed_stages: List[Dict[str, Any]] = field(default_factory=list)
    current_stage_number: int = 1
    max_stages: int = 5
    all_goals: List[str] = field(default_factory=list)
    remaining_goals: List[str] = field(default_factory=list)
    goal_completion_status: Dict[str, str] = field(default_factory=dict)
    
    # ─── Retry ───
    prior_plans: List[Dict[str, Any]] = field(default_factory=list)
    loop_feedback: List[str] = field(default_factory=list)
    
    # ─── Audit ───
    processing_history: List[ProcessingRecord] = field(default_factory=list)
    errors: List[Dict[str, Any]] = field(default_factory=list)
    
    # ─── Timing ───
    created_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    
    # ─── Metadata ───
    metadata: Dict[str, Any] = field(default_factory=dict)
```

**Class Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `from_dict` | `(data: Dict[str, Any]) -> Envelope` | Create envelope from dictionary (Go JSON response) |

**Instance Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `initialize_goals` | `(goals: List[str]) -> None` | Initialize goals for multi-stage execution |
| `mark_goal_complete` | `(goal: str) -> None` | Mark a goal as complete |
| `advance_stage` | `() -> None` | Advance to the next stage |
| `get_stage_context` | `() -> Dict[str, Any]` | Get context for the current stage |
| `is_stuck` | `(min_progress_stages: int = 2) -> bool` | Check if pipeline is stuck |
| `to_dict` | `() -> Dict[str, Any]` | Convert to dictionary for Go JSON input |
| `to_state_dict` | `() -> Dict[str, Any]` | Convert to dictionary with ALL fields for complete state serialization |

---

## Usage Examples

### Creating an Envelope

```python
from jeeves_protocols import create_envelope

envelope = create_envelope(
    raw_input="Analyze the authentication module",
    user_id="user-123",
    session_id="session-456",
    request_id="req-789",
    metadata={"capability": "code_analysis", "repo_path": "/path/to/repo"}
)
```

### Working with Goals

```python
# Initialize goals from intent parsing
envelope.initialize_goals([
    "Find authentication entry point",
    "Trace login flow",
    "Identify security vulnerabilities"
])

# Mark goal complete after processing
envelope.mark_goal_complete("Find authentication entry point")

# Check remaining goals
print(envelope.remaining_goals)  # ["Trace login flow", "Identify security vulnerabilities"]

# Get stage context
context = envelope.get_stage_context()
# {
#     "current_stage_number": 1,
#     "max_stages": 5,
#     "all_goals": [...],
#     "remaining_goals": [...],
#     ...
# }
```

### Serialization

```python
# For Go JSON communication
json_data = envelope.to_dict()

# For checkpoint/persistence (includes ALL fields)
full_state = envelope.to_state_dict()

# Restore from dictionary
restored = Envelope.from_dict(json_data)
```

### Checking Pipeline Status

```python
# Check if pipeline is stuck
if envelope.is_stuck(min_progress_stages=2):
    # Pipeline has made no progress after 2 stages
    handle_stuck_pipeline()

# Check termination
if envelope.terminated:
    reason = envelope.terminal_reason  # TerminalReason enum
    print(f"Terminated: {reason.value}")
```

---

## Serialization Contract

The envelope supports lossless round-trip serialization between Python and Go:

1. **to_dict()**: Basic serialization for Go gRPC communication
2. **to_state_dict()**: Complete state serialization including:
   - All datetime fields as ISO format strings
   - All ProcessingRecord objects fully serialized
   - All nested structures preserved

This ensures Contract 11 compliance: Envelope round-trip must be lossless.

---

## Navigation

- [Back to README](README.md)
- [Previous: Enums](enums.md)
- [Next: Configuration](config.md)
