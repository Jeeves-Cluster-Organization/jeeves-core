# Jeeves-Core Architectural Contracts

**Last Updated:** 2025-12-16
**Version:** 1.0.0

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
    from jeeves_mission_system.orchestrator.agent_events import AgentEventEmitter

    # INCORRECT: Relative imports
    # from orchestrator.agent_events import AgentEventEmitter
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

## Changelog

### 2025-12-16 - v1.0.0
- Initial contracts document created
- Documented layer architecture
- Defined 9 core contracts
- Captured state from codebase audit

---

*This document is maintained as part of the Jeeves-Core codebase audit process.*
