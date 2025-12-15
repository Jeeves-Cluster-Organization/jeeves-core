# Jeeves-Core Codebase Audit Report

**Date:** 2025-12-15
**Scope:** Full codebase audit for unused paths, bad patterns, dead code, duplicates, and centralization opportunities
**Modules Analyzed:** jeeves_mission_system, jeeves_avionics, jeeves_control_tower, jeeves_memory_module, jeeves_protocols, jeeves_shared

---

## Executive Summary

| Category | Critical | High | Medium | Low | Total |
|----------|----------|------|--------|-----|-------|
| Bugs | 1 | 0 | 1 | 0 | 2 |
| Dead Code | 0 | 1 | 4 | 8 | 13 |
| Anti-Patterns | 0 | 2 | 6 | 4 | 12 |
| Duplicates | 0 | 2 | 3 | 2 | 7 |
| Cross-Module Issues | 0 | 2 | 4 | 2 | 8 |
| **Total** | **1** | **7** | **18** | **16** | **42** |

---

## Architecture Overview

The codebase follows a well-defined layered architecture:

```
L0: jeeves_shared, jeeves_protocols  (Zero dependencies - types & utilities)
    ↓
L1: jeeves_control_tower             (OS-like kernel - process lifecycle)
    ↓
L2: jeeves_memory_module             (Event sourcing & semantic memory)
    ↓
L3: jeeves_avionics                  (Infrastructure - LLM, DB, Gateway)
    ↓
L4: jeeves_mission_system            (API layer - orchestration & services)
```

---

## CRITICAL Issues (Must Fix Immediately)

### 1. Runtime Crash Bug in EmbeddingService
**File:** `jeeves_memory_module/services/embedding_service.py:253`
**Type:** AttributeError at runtime

```python
# CURRENT (BROKEN):
self.logger.info("cache_cleared")

# FIX:
self._logger.info("cache_cleared")
```

**Impact:** Application crashes when `clear_cache()` is called.

---

## HIGH Priority Issues

### 2. Broken Relative Import in Chat API
**File:** `jeeves_mission_system/api/chat.py:22`

```python
# CURRENT (BROKEN):
from services.chat_service import ChatService

# FIX:
from jeeves_mission_system.services.chat_service import ChatService
```

**Impact:** Module fails to import unless cwd is in sys.path.

---

### 3. Protocol Duplication Across Files
**Files:** `jeeves_protocols/agents.py` vs `jeeves_protocols/protocols.py`

| Protocol | agents.py | protocols.py | Status |
|----------|-----------|--------------|--------|
| Logger/LoggerProtocol | Lines 32-37 | Lines 38-45 | DUPLICATE |
| LLMProvider/LLMProviderProtocol | Lines 20-23 | Lines 139-154 | DUPLICATE |
| Persistence/PersistenceProtocol | Lines 40-43 | Lines 53-58 | DUPLICATE |
| ToolExecutor | Lines 25-30 | Lines 162-174 | DUPLICATE |
| PromptRegistry | Lines 46-49 | Lines 202-210 | DUPLICATE |
| EventContext | Lines 52-55 | Lines 265-280 | DUPLICATE |

**Fix:** Consolidate all protocols to `protocols.py`, import from there in `agents.py`.

---

### 4. Type System Mismatch in Memory Module
**Files:** `jeeves_memory_module/intent_classifier.py` vs `jeeves_memory_module/manager.py`

- IntentClassifier classifies to: `task`, `journal`, `fact`, `message`
- MemoryManager accepts only: `message`, `fact`

**Impact:** 50% of classification results are discarded.

**Fix Options:**
1. Update IntentClassifier to only classify `message`/`fact`
2. Extend MemoryManager to handle all 4 types
3. Add explicit filtering with deprecation warning

---

### 5. Missing Import in Test Configuration
**File:** `jeeves_control_tower/tests/conftest.py:155`

```python
# MISSING:
from unittest.mock import MagicMock

# MagicMock is used on line 155 but never imported
```

**Impact:** NameError at test runtime.

---

## MEDIUM Priority Issues

### 6. Global Singleton Anti-Pattern (8 instances)
The codebase heavily uses lazy-initialized global singletons, violating ADR-001 (DI architecture):

| Location | Global Variable | Getter Function |
|----------|----------------|-----------------|
| `jeeves_avionics/settings.py:296` | `_settings` | `get_settings()` |
| `jeeves_avionics/feature_flags.py:433` | `_feature_flags` | `get_feature_flags()` |
| `jeeves_avionics/capability_registry.py:231` | `_registry` | `get_capability_registry()` |
| `jeeves_avionics/gateway/grpc_client.py:155` | `_client` | `get_grpc_client()` |
| `jeeves_avionics/webhooks/service.py:626` | `_global_webhook_service` | `get_webhook_service()` |
| `jeeves_avionics/database/redis_client.py:368` | `_redis_client` | `get_redis_client()` |
| `jeeves_avionics/database/connection_manager.py:311` | `_connection_manager` | `get_connection_manager()` |
| `jeeves_avionics/observability/otel_adapter.py:531` | `_global_adapter` | `get_otel_adapter()` |

**Issues:**
- Hard to test (can't easily mock)
- Not thread-safe in some cases
- Order-dependent initialization

**Fix:** Use dependency injection via AppContext or function parameters.

---

### 7. Bloated Classes Violating Single Responsibility

| Class | File | Lines | Concerns Mixed |
|-------|------|-------|----------------|
| LLMGateway | `jeeves_avionics/llm/gateway.py` | 803 | Cost tracking, streaming, fallback, stats |
| PostgreSQLClient | `jeeves_avionics/database/postgres_client.py` | 766 | Pool mgmt, schema init, queries, transactions |
| OpenTelemetryAdapter | `jeeves_avionics/observability/otel_adapter.py` | 576 | Tracing, logging, span propagation, events |
| WebhookService | `jeeves_avionics/webhooks/service.py` | 645 | Webhook mgmt, retry logic, event emission |
| ChatRouter | `jeeves_avionics/gateway/routers/chat.py` | 696 | HTTP handling, event categorization, SSE |

**Fix:** Split into smaller, focused classes (e.g., `LLMCostCalculator`, `LLMStreamHandler`, `LLMFallbackStrategy`).

---

### 8. LLM Provider Factory Proliferation
**Files:** `jeeves_avionics/llm/factory.py`, `jeeves_avionics/wiring.py`

4 overlapping factory functions:
1. `create_llm_provider()` - Basic provider creation
2. `create_agent_provider()` - Agent-specific with settings
3. `create_agent_provider_with_node_awareness()` - Adds node profiles
4. `create_llm_provider_factory()` - Returns factory function

**Fix:** Consolidate to single `LLMProviderFactory` class with builder pattern.

---

### 9. Duplicate Interrupt Handling
**Files:** `jeeves_control_tower/lifecycle/manager.py` vs `jeeves_control_tower/events/aggregator.py`

| Method | LifecycleManager | EventAggregator | Used By Kernel |
|--------|------------------|-----------------|----------------|
| `set_interrupt()` | Lines 370-391 | - | NO |
| `raise_interrupt()` | - | Lines 95-120 | YES |
| `clear_interrupt()` | Lines 393-403 | Lines 136-151 | EventAggregator only |

**Fix:** Remove duplicate methods from LifecycleManager, consolidate in EventAggregator.

---

### 10. Direct State Manipulation Bypassing Protocol
**File:** `jeeves_control_tower/kernel.py:472`

```python
# CURRENT (Anti-pattern):
pcb.state = ProcessState.WAITING  # Direct assignment
self._lifecycle.transition_state(pid, ProcessState.READY)  # Contradicts above

# FIX:
self._lifecycle.transition_state(pid, ProcessState.READY)  # Use proper protocol
```

---

### 11. Dual Span Tracking Mechanisms
**Files:** `jeeves_avionics/logging/__init__.py` vs `jeeves_avionics/observability/otel_adapter.py`

Both maintain separate span registries:
- `logging/__init__.py`: `_ACTIVE_SPANS` dict
- `observability/otel_adapter.py`: `_otel_spans` dict

**Fix:** Consolidate to single span tracking in `otel_adapter.py`.

---

### 12. TYPE_CHECKING Import Safety Issue
**File:** `jeeves_mission_system/orchestrator/event_context.py:37`

```python
if TYPE_CHECKING:
    from orchestrator.agent_events import AgentEventEmitter  # Relative import!
```

**Fix:** Use absolute import or add `from __future__ import annotations`.

---

## LOW Priority Issues

### 13. Dead Code - Unused Methods

| File | Method | Lines | Reason |
|------|--------|-------|--------|
| `jeeves_control_tower/lifecycle/manager.py` | `update_current_stage()` | 361-368 | Never called |
| `jeeves_control_tower/lifecycle/manager.py` | `set_interrupt()` | 370-391 | EventAggregator version used |
| `jeeves_control_tower/lifecycle/manager.py` | `clear_interrupt()` | 393-403 | EventAggregator version used |
| `jeeves_control_tower/ipc/coordinator.py` | `mark_service_healthy()` | 384-395 | Never called |
| `jeeves_control_tower/ipc/coordinator.py` | `mark_service_unhealthy()` | 397-408 | Never called |
| `jeeves_control_tower/ipc/coordinator.py` | `get_service_load()` | 410-418 | Never called |
| `jeeves_control_tower/ipc/coordinator.py` | `get_least_loaded_service()` | 420-427 | Never called |
| `jeeves_control_tower/ipc/coordinator.py` | `subscribe()` | 356-367 | Never called |
| `jeeves_control_tower/ipc/coordinator.py` | `unsubscribe()` | 369-378 | Never called |
| `jeeves_memory_module/intent_classifier.py` | `get_thresholds()` | 279-290 | Orphaned from v3.0 |
| `jeeves_memory_module/intent_classifier.py` | `update_thresholds()` | 292-318 | Orphaned from v3.0 |
| `jeeves_avionics/database/factory.py` | `reset_factory()` | 118-121 | No-op function |
| `jeeves_avionics/gateway/grpc_client.py` | `set_grpc_client()` | 155-157 | Global state setter unused |

---

### 14. Unused Class - HttpCommBusAdapter
**File:** `jeeves_control_tower/ipc/adapters/http_adapter.py:85-293`

Complete implementation (208 lines) but never instantiated. Also includes unused protocol `CommBusAdapterProtocol` (lines 25-82).

**Fix:** Either integrate or remove (or document as future stub).

---

### 15. Unused Import
**File:** `jeeves_memory_module/services/embedding_service.py:15`

```python
from functools import lru_cache  # Never used
```

---

### 16. Hardcoded Event Mappings
**File:** `jeeves_avionics/gateway/routers/chat.py:73-86`

```python
# Anti-pattern: Long if-elif chains for event categorization
if "perception" in event_type or "intent" in event_type or "planner" in event_type or \
   "executor" in event_type or "synthesizer" in event_type or "integration" in event_type or \
   "agent.started" in event_type or "agent.completed" in event_type:
    category = EventCategory.AGENT_LIFECYCLE
elif "critic" in event_type:
    category = EventCategory.CRITIC_DECISION
# ... more elif chains
```

**Fix:** Use configuration-driven mapping dict or event metadata.

---

### 17. Mixed Initialization Concerns in Database Factory
**File:** `jeeves_avionics/database/factory.py`

```python
async def create_database_client(settings, logger=None, auto_init_schema=True):
    client = await _create_client(settings, logger=_logger)
    await client.connect()              # Creation concern
    if auto_init_schema:                # Initialization concern
        await _maybe_init_schema(client, _logger)  # Schema concern
    return client
```

**Fix:** Separate creation from initialization.

---

### 18. Logger Injection Inconsistency
Many functions accept `logger: Optional[LoggerProtocol]` but also call `get_current_logger()` as fallback, mixing DI with global access:

```python
# Pattern found in 43+ locations
def some_function(logger: Optional[LoggerProtocol] = None):
    logger = logger or get_current_logger()  # Mixed patterns
```

**Fix:** Choose one pattern consistently (prefer DI).

---

## Centralization Opportunities

### 1. Consolidate Protocol Definitions
Move all protocols from `agents.py` to `protocols.py` for single source of truth.

### 2. Create Unified Factory Pattern
Replace 4 LLM factory functions with single `LLMProviderFactory` class.

### 3. Centralize Event Categorization
Extract hardcoded event mappings to configuration in `jeeves_protocols/events.py`.

### 4. Consolidate JSON Utilities
Merge `JSONRepairKit` (protocols) with `JSONEncoderWithUUID` (shared) into single module.

### 5. Unify Span Tracking
Consolidate dual span registries into single mechanism.

---

## Layer Responsibility Summary

| Layer | Responsibilities | Violations Found |
|-------|-----------------|------------------|
| **jeeves_shared** | UUID utils, datetime, logging base | None |
| **jeeves_protocols** | Type contracts, protocols, events | Protocol duplication |
| **jeeves_control_tower** | Process lifecycle, resources, interrupts | Dead code, duplicate interrupt handling |
| **jeeves_memory_module** | Event sourcing, semantic search | Bug, type mismatch |
| **jeeves_avionics** | LLM, DB, Gateway infrastructure | 8 global singletons, bloated classes |
| **jeeves_mission_system** | API orchestration, services | Import issues |

---

## Recommended Fix Priority

### Immediate (Before Next Deploy)
1. Fix `embedding_service.py:253` logger bug
2. Fix `api/chat.py:22` relative import
3. Add missing MagicMock import in control_tower tests

### Short-Term (Within 1 Week)
4. Consolidate protocol definitions
5. Fix IntentClassifier/MemoryManager type mismatch
6. Remove duplicate interrupt handling methods

### Medium-Term (Within 1 Month)
7. Replace global singletons with proper DI
8. Split bloated classes (LLMGateway, PostgreSQLClient)
9. Consolidate LLM factory functions
10. Extract event categorization to config

### Long-Term (Backlog)
11. Remove all dead code identified
12. Consolidate JSON utilities
13. Unify span tracking
14. Standardize logger injection pattern

---

## Test Coverage Gaps

| Module | Well Tested | Untested |
|--------|-------------|----------|
| jeeves_control_tower | ResourceTracker, InterruptService | HttpCommBusAdapter, unused lifecycle methods |
| jeeves_memory_module | Core services | Some edge cases in intent classifier |
| jeeves_avionics | Gateway routers | Some factory functions |
| jeeves_mission_system | API endpoints | Some orchestrator paths |

---

## Appendix: Files Changed Summary

When implementing fixes, the following files will need modification:

```
# Critical Fixes
jeeves_memory_module/services/embedding_service.py
jeeves_mission_system/api/chat.py
jeeves_control_tower/tests/conftest.py

# Protocol Consolidation
jeeves_protocols/agents.py
jeeves_protocols/protocols.py
jeeves_protocols/__init__.py

# Dead Code Removal
jeeves_control_tower/lifecycle/manager.py
jeeves_control_tower/ipc/coordinator.py
jeeves_control_tower/ipc/adapters/http_adapter.py
jeeves_memory_module/intent_classifier.py
jeeves_avionics/database/factory.py
jeeves_avionics/gateway/grpc_client.py

# Refactoring
jeeves_avionics/llm/factory.py
jeeves_avionics/llm/gateway.py
jeeves_avionics/database/postgres_client.py
jeeves_avionics/gateway/routers/chat.py
jeeves_control_tower/kernel.py
jeeves_control_tower/events/aggregator.py
```

---

*Report generated by automated codebase audit*
