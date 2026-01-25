# Jeeves-Core Architectural Contracts

**Last Updated:** 2026-01-25
**Version:** 1.3.1

This document defines the architectural contracts that govern the Jeeves-Core codebase. These contracts ensure consistency, maintainability, and proper separation of concerns.

---

## Layer Architecture

The codebase follows a strict layered architecture where each layer may only depend on layers below it.

```
┌─────────────────────────────────────────────────────────────────┐
│ L4: jeeves_mission_system                                       │
│     (API layer - orchestration, services, adapters)             │
├─────────────────────────────────────────────────────────────────┤
│ L3: jeeves_avionics                                             │
│     (Infrastructure - LLM, DB, Gateway, Observability)          │
├─────────────────────────────────────────────────────────────────┤
│ L2: jeeves_memory_module                                        │
│     (Event sourcing, semantic memory, repositories)             │
├─────────────────────────────────────────────────────────────────┤
│ L1: jeeves_control_tower                                        │
│     (OS-like kernel - process lifecycle, resources, IPC)        │
├─────────────────────────────────────────────────────────────────┤
│ L0: jeeves_shared, jeeves_protocols                             │
│     (Zero dependencies - types, utilities, protocols)           │
└─────────────────────────────────────────────────────────────────┘
```

### Import Rules

| Layer | May Import From |
|-------|-----------------|
| L4 (mission_system) | L3, L2, L1, L0 |
| L3 (avionics) | L2, L1, L0 |
| L2 (memory_module) | L1, L0 |
| L1 (control_tower) | L0 only |
| L0 (shared/protocols) | Nothing (base layer) |

### Documented Exceptions

**Memory Module → Avionics (L2 → L3):**
The Memory Module CONSTITUTION explicitly allows importing from `jeeves_avionics.database.factory` for database connection pooling. This is a controlled exception because:
- Database connection management requires avionics infrastructure
- The import is limited to factory functions, not domain logic
- Memory Module implementations need database access for persistence

```python
# Allowed exception per jeeves_memory_module/CONSTITUTION.md
from jeeves_avionics.database.factory import get_async_pool
```

---

## Contract 1: Protocol-Based State Management

### Rule
All state transitions in `jeeves_control_tower` MUST go through the appropriate manager protocols.

### Rationale
Direct state manipulation bypasses validation, logging, and event emission built into managers.

### Enforcement
```python
# CORRECT: Use protocol methods
self._lifecycle.transition_state(pid, ProcessState.READY)

# INCORRECT: Direct state manipulation
pcb.state = ProcessState.READY  # NEVER DO THIS
```

### Affected Components
- `LifecycleManager` for process state
- `ResourceTracker` for resource accounting
- `InterruptService` for interrupt handling

---

## Contract 2: Context-Aware Global State (Contextvars)

### Rule
Module-level state that may be accessed across async boundaries MUST use `contextvars.ContextVar`.

### Rationale
Global variables are not thread-safe and don't maintain proper isolation in async contexts.

### Implementation
```python
from contextvars import ContextVar

# CORRECT: Context-aware state
_otel_enabled: ContextVar[bool] = ContextVar("otel_enabled", default=False)
_active_spans: ContextVar[Dict[str, Span]] = ContextVar("active_spans")

def _get_otel_enabled() -> bool:
    return _otel_enabled.get()

def _set_otel_enabled(value: bool) -> None:
    _otel_enabled.set(value)
```

### Affected Modules
- `jeeves_avionics/logging/__init__.py` - OTEL state, active spans

---

## Contract 3: Dependency Injection via Adapters (Constitutional Compliance)

### Rule
Apps/verticals MUST access infrastructure services through `MissionSystemAdapters`, not direct imports.

### Rationale
Single access point for infrastructure services enables:
- Consistent dependency injection
- Easy mocking for tests
- Clear layer boundaries

### Pattern
```python
# CORRECT: Access via adapters
from jeeves_mission_system.adapters import get_logger, get_settings

logger = get_logger()
settings = get_settings()

# INCORRECT: Direct avionics import in apps
from jeeves_avionics.logging import get_current_logger  # DO NOT DO THIS IN APPS
```

### Architecture Enforcement
```
apps/verticals → mission_system (adapters) → avionics
```

---

## Contract 4: Absolute Imports in TYPE_CHECKING Blocks

### Rule
All imports within `TYPE_CHECKING` blocks MUST use absolute imports.

### Rationale
Relative imports in TYPE_CHECKING blocks can fail during static analysis and IDE tooling.

### Implementation
```python
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    # CORRECT: Absolute imports
    from jeeves_mission_system.orchestrator.agent_events import EventEmitter

    # INCORRECT: Relative imports
    # from orchestrator.agent_events import EventEmitter
```

---

## Contract 5: Single Source for Protocols

### Rule
Each protocol MUST be defined in exactly one place within `jeeves_protocols`.

### Rationale
Duplicate protocol definitions lead to:
- Type checker confusion
- Inconsistent implementations
- Maintenance burden

### Canonical Locations
| Protocol | Location |
|----------|----------|
| `LoggerProtocol` | `jeeves_protocols/protocols.py` |
| `LLMProviderProtocol` | `jeeves_protocols/protocols.py` |
| `PersistenceProtocol` | `jeeves_protocols/protocols.py` |
| `ToolRegistryProtocol` | `jeeves_protocols/protocols.py` |
| `SettingsProtocol` | `jeeves_protocols/protocols.py` |

---

## Contract 6: Dead Code Removal

### Rule
Unused code MUST be removed, not commented out or kept "for future use".

### Rationale
- Dead code increases cognitive load
- Increases maintenance burden
- Can contain security vulnerabilities
- Confuses static analysis tools

### Removed Dead Code (as of 2025-12-16)
- `LifecycleManager`: `update_current_stage()`, `set_interrupt()`, `clear_interrupt()`
- `CommBusCoordinator`: `subscribe()`, `unsubscribe()`, health management methods
- `IntentClassifier`: `get_thresholds()`, `update_thresholds()`
- `HttpCommBusAdapter`: Entire file

---

## Contract 7: Interrupt Handling Ownership

### Rule
Interrupt handling is owned by `EventAggregator`, not `LifecycleManager`.

### Rationale
Clear ownership prevents duplicate implementations and confusion about which component to use.

### Implementation
```python
# CORRECT: Use EventAggregator
self._events.raise_interrupt(pid, interrupt_kind, payload)
self._events.clear_interrupt(pid)

# INCORRECT: These methods no longer exist
self._lifecycle.set_interrupt(...)  # REMOVED
```

---

## Contract 8: Span Tracking Isolation

### Rule
Two separate span tracking mechanisms exist and serve different purposes:

| Mechanism | Location | Purpose |
|-----------|----------|---------|
| `_active_spans` | `logging/__init__.py` | Log enrichment, lightweight tracing |
| `_span_stacks` | `otel_adapter.py` | OpenTelemetry SDK integration |

### Rationale
- `_active_spans`: Works without OTEL dependency, provides basic span context for logs
- `_span_stacks`: Full OpenTelemetry integration with proper SDK spans

---

## Contract 9: Logger Fallback Pattern

### Rule
The pattern `logger = logger or get_current_logger()` is acceptable for optional logger injection.

### Rationale
Allows callers to optionally inject loggers while maintaining functionality without explicit injection.

### Usage
```python
def some_function(logger: Optional[LoggerProtocol] = None):
    logger = logger or get_current_logger()
    logger.info("operation_started")
```

---

## Enforcement

### Pre-Commit Checks
1. Layer import validation (no upward imports)
2. Dead code detection
3. Import style (absolute in TYPE_CHECKING)

### Code Review Requirements
1. Protocol changes require review
2. New global state must use contextvars
3. Infrastructure access must go through adapters

---

---

## Contract 10: Capability-Owned ToolId

### Rule
`ToolId` enums MUST be defined in the capability layer, NOT in avionics or mission_system.

### Rationale
- Prevents layer violations (L5 importing L2 for tool identifiers)
- Allows capabilities to define their own tool sets independently
- Maintains clean dependency direction (outer layers depend on inner, not vice versa)

### Implementation
```python
# ✅ CORRECT - ToolId in capability layer
# my_capability/tools/catalog.py
class ToolId(str, Enum):
    LOCATE = "locate"
    READ_CODE = "read_code"

# ❌ INCORRECT - ToolId in avionics (removed)
# from jeeves_avionics.tools.catalog import ToolId  # NO LONGER EXISTS

# ❌ INCORRECT - ToolId re-exported from contracts_core
# from jeeves_mission_system.contracts_core import ToolId  # REMOVED
```

### Affected Components
- `jeeves_avionics.tools.catalog` - Still provides generic ToolCategory, ToolDefinition
- `jeeves_mission_system.contracts_core` - No longer exports ToolId, ToolCatalog
- Capability `tools/catalog.py` - Defines capability-specific ToolId enum

---

## Contract 11: Envelope Sync (Go ↔ Python)

### Rule
The Go and Python `Envelope` implementations MUST produce identical serialization, verified by round-trip contract tests.

### Rationale
- Envelope flows: Go → Python → Go during request lifecycle
- Any field mismatch causes silent data loss or runtime errors
- Go has 983-line implementation with typed `FlowInterrupt`
- Python has 268-line implementation with `Dict[str, Any]` interrupt

### Enforcement
```python
# tests/contracts/test_envelope_roundtrip.py

def test_envelope_roundtrip_all_fields():
    """Go → Python → Go must be lossless."""
    grpc = GrpcGoClient()
    original = grpc.create_envelope(
        raw_input="test",
        user_id="u1",
        session_id="s1",
    )
    
    # Mutate envelope with all field types
    original.set_interrupt(InterruptKind.CLARIFICATION, "q1")
    original.set_output("perception", {"normalized": "..."})
    original.initialize_goals(["goal1", "goal2"])
    
    # Round-trip
    py_dict = original.to_state_dict()
    py_envelope = Envelope.from_dict(py_dict)
    go_dict = py_envelope.to_dict()
    
    # Compare
    assert_dicts_equal(py_dict, go_dict, ignore_timestamps=True)

def test_flow_interrupt_typing():
    """FlowInterrupt must serialize identically."""
    # Go: typed FlowInterrupt struct
    # Python: must match exactly (not Dict[str, Any])
    ...
```

### Affected Components
- `coreengine/envelope/envelope.go` - Go envelope
- `jeeves_protocols/envelope.py` - Python envelope
- `jeeves_protocols/grpc_client.py` - gRPC client (to be implemented)

---

## Contract 12: Bounds Authority Split

### Rule
Bounds enforcement follows defense-in-depth:
- **Go Runtime**: In-loop enforcement via `env.CanContinue()` (authoritative during execution)
- **Python ControlTower**: Post-hoc audit via `ResourceTracker.check_quota()` (catches discrepancies)

### Rationale
Go is the execution engine; it must enforce bounds before each agent. Python is the lifecycle manager; it validates final state and handles failures.

### Invariant
```python
# After every request completion:
assert envelope.llm_call_count == resource_tracker.get_usage(pid).llm_calls
assert envelope.agent_hop_count == resource_tracker.get_usage(pid).agent_hops
```

### Implementation
```go
// Go: in-loop check (coreengine/envelope/envelope.go)
func (e *Envelope) CanContinue() bool {
    if e.LLMCallCount >= e.MaxLLMCalls { return false }
    if e.AgentHopCount >= e.MaxAgentHops { return false }
    // ...
}
```

```python
# Python: post-hoc audit (jeeves_control_tower/kernel.py)
quota_exceeded = self._resources.check_quota(pid)
if quota_exceeded:
    # This should NEVER happen if Go enforced correctly
    logger.error("bounds_discrepancy", go_count=env.llm_call_count, ...)
```

---

## Contract 13: Core Doesn't Reject Cycles (Cyclic Routing)

### Rule
Core does NOT validate or reject cycles. Capability layer defines the graph via routing rules.
If capability defines cycles, they MUST be bounded by:
1. **Global iteration limit** (`MaxIterations`) - hard cap on pipeline loops
2. **Per-edge limits** (`EdgeLimits`) - capability-defined per-transition limits

### Rationale
```
Capability defines:  StageC -> [loop_back] -> StageA   (cycle exists)
       or:           StageC -> end                     (no cycle)
```

Core's job: Execute the graph. Enforce bounds. Not judge the graph structure.

### Configuration
```go
cfg := &PipelineConfig{
    MaxIterations: 5,  // Global cap
    EdgeLimits: []EdgeLimit{
        {From: "stageC", To: "stageA", MaxCount: 3},  // Max 3 loops on this edge
    },
}
```

> **Note:** Core uses generic terminology (loop_back, proceed, stageA, stageB).
> Domain-specific names (critic, planner) belong in capability layer only.

### Invariants

**C13.1: Edge limits enforced at runtime**
```go
if edgeCounts[edge] >= edgeLimit {
    route to "end" instead of cycle
}
```

**C13.2: Global limit is hard cap**
```go
if env.Iteration > p.MaxIterations {
    terminate
}
```

**C13.3: Checkpoints before cycles**
```go
if isCycleEdge(from, to) {
    checkpoint(from, env)
}
```

### Affected Components
- `coreengine/config/pipeline.go` - EdgeLimit type
- `coreengine/runtime/state_machine_executor.go` - Cycle handling
- Capability layer - Defines EdgeLimits for workflows

---

## Changelog

### 2026-01-25 - v1.3.1 (Test Alignment)
- Fixed test drift: aligned tests with current API signatures
- Added `UNKNOWN` to `HealthStatus` enum (used by L7 ToolHealthService)
- Made `sentence-transformers` a lazy/optional dependency
- Moved `test_sql_adapter.py` to integration/ (requires PostgreSQL)
- Fixed DI pattern in `test_trace_recorder.py` (use `is_enabled()` not patching)
- All unit tests passing (Go: 100%, Python: 325/325)
- Integration tests passing without external services (59/59)

### 2026-01-19 - v1.3.0 (Architecture Audit)
- Documented Memory Module → Avionics exception (L2 → L3 for database.factory)
- Clarified import rules with exceptions section

### 2026-01-06 - v1.2.0 (Hardening Review)
- Added Contract 11: Envelope Sync (Go <-> Python round-trip tests)
- Added Contract 12: Bounds Authority Split (Go in-loop, Python post-hoc)
- Added Contract 13: Intentional Cycles (Cyclic Routing)
- Added CyclePolicy and EdgeLimit to pipeline config
- Documented enforcement mechanisms and invariants

### 2026-01-06 - v1.1.0
- Added Contract 10: Capability-Owned ToolId
- ToolId/ToolCatalog removed from avionics and contracts_core exports

### 2025-12-16 - v1.0.0
- Initial contracts document created
- Documented layer architecture
- Defined 9 core contracts
- Captured state from codebase audit

---

*This document is maintained as part of the Jeeves-Core codebase audit process.*
