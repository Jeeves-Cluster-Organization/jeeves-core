# Failure Modes and Detection Analysis

**Version:** 1.0.0  
**Date:** 2026-01-19  
**Purpose:** Catalog top 10 failure modes with detection signals and current code coverage

---

## Overview

This document catalogs the critical failure modes in jeeves-core, their detection signals (logs/tests/metrics), and whether the current codebase surfaces them adequately.

The system follows the **"Fail Loud"** design principle from CONSTITUTION P2:
> "Fail loudly. No silent fallbacks. No guessing. If a write fails, rollback and report exactly what broke."

---

## Top 10 Failure Modes

### 1. Pipeline Bounds Exceeded (Crashes/Resource Exhaustion)

**Description:** Pipeline execution exceeds configured limits for LLM calls, agent hops, or iterations, leading to runaway execution or resource exhaustion.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Logs** | `coreengine/runtime/runtime.go:223-227` | `pipeline_bounds_exceeded` with `terminal_reason` |
| **Metrics** | `GenericEnvelope.LLMCallCount`, `AgentHopCount` | Tracked per-envelope |
| **Tests** | `coreengine/runtime/runtime_test.go` | Go unit tests |

**Current Code Surfaces It:** ✅ **YES**

```go
// coreengine/envelope/generic.go:379-402
func (e *GenericEnvelope) CanContinue() bool {
    if e.LLMCallCount >= e.MaxLLMCalls {
        reason := TerminalReasonMaxLLMCallsExceeded
        e.TerminalReason_ = &reason
        return false
    }
    if e.AgentHopCount >= e.MaxAgentHops {
        reason := TerminalReasonMaxAgentHopsExceeded
        e.TerminalReason_ = &reason
        return false
    }
    // ...
}
```

**Detection Gaps:**
- No alerting/metrics export to external monitoring systems
- No aggregated metrics across requests

---

### 2. Silent Event Emission Failures (Silent Corruption)

**Description:** Event emission to the event store fails but doesn't propagate to caller, causing event log gaps and potential audit trail corruption.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Logs** | `jeeves_memory_module/services/event_emitter.py:350-357` | `event_emission_failed` |
| **Tests** | `jeeves_memory_module/tests/unit/services/test_event_emitter.py:157` | Tests error handling |

**Current Code Surfaces It:** ⚠️ **PARTIAL**

```python
# jeeves_memory_module/services/event_emitter.py:350-357
except Exception as e:
    self._logger.error(
        "event_emission_failed",
        error=str(e),
        event_type=event_type
    )
    # Don't fail the operation if event emission fails
    return None  # ← SILENT FAILURE - returns None instead of raising
```

**Detection Gaps:**
- Returns `None` on failure (silent) rather than raising
- No metrics counter for failed emissions
- CommBus publishing failures are also logged but swallowed (line 400-406)

**Recommendation:** Add a failure counter metric; consider optional strict mode where event failures propagate.

---

### 3. Message Routing Failures (Wrong Routing)

**Description:** Messages sent via CommBus have no registered handler, causing messages to be silently dropped.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Logs** | `commbus/bus.go:78-79, 137-138` | "No subscribers for event", "No handler for command" |
| **Errors** | `commbus/errors.go:28-35` | `NoHandlerError` for queries |

**Current Code Surfaces It:** ⚠️ **PARTIAL (behavior differs by message type)**

| Message Type | Behavior | Surfaced? |
|--------------|----------|-----------|
| **Events (Publish)** | Logged, continues silently | ⚠️ Logged only |
| **Commands (Send)** | Logged, continues silently | ⚠️ Logged only |
| **Queries (QuerySync)** | Returns `NoHandlerError` | ✅ YES |

```go
// commbus/bus.go:137-138 - Commands silently continue
if !exists {
    log.Printf("No handler for command %s", messageType)
    return nil  // ← SILENT - returns nil error
}
```

**Detection Gaps:**
- Events and commands with no handler are logged but don't fail
- No metrics for unhandled message types

---

### 4. Database Connection Pool Exhaustion (Crashes)

**Description:** Connection pool runs out of connections, blocking all database operations.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Metrics** | `jeeves_avionics/database/postgres_client.py:676-690` | `health_check()` returns pool stats |
| **Logs** | Connection timeout errors | SQLAlchemy exceptions |
| **Tests** | `jeeves_avionics/tests/unit/database/test_connection_manager.py` | Pool testing |

**Current Code Surfaces It:** ✅ **YES**

```python
# jeeves_avionics/database/postgres_client.py:676-690
async def health_check(self) -> Dict[str, Any]:
    if self.engine:
        pool = self.engine.pool
        pool_stats = {
            "size": pool.size(),
            "checked_in": pool.checkedin(),
            "checked_out": pool.checkedout(),
            "overflow": pool.overflow(),
        }
```

**Detection Gaps:**
- No automatic alerting when pool approaches limits
- Health check is passive (must be called)

---

### 5. Rate Limiter Window Memory Growth (Memory Leak)

**Description:** Rate limiter sliding window buckets accumulate without cleanup, causing unbounded memory growth.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Logs** | `jeeves_control_tower/resources/rate_limiter.py:417-421` | `rate_limit_cleanup` debug log |
| **Metrics** | `SlidingWindow.buckets` dict size | Internal counter |

**Current Code Surfaces It:** ⚠️ **PARTIAL**

```python
# jeeves_control_tower/resources/rate_limiter.py:396-423
def cleanup_expired(self) -> int:
    """Clean up expired window data.
    
    Should be called periodically to prevent memory growth.  # ← MANUAL CALL REQUIRED
    """
    # ... cleanup logic
```

**Detection Gaps:**
- `cleanup_expired()` must be called manually - no automatic periodic cleanup
- No monitoring for window dict sizes
- Session dedup cache has same issue (`SessionDedupCache` line 136-144)

**Recommendation:** Add periodic cleanup via background task or TTL-based eviction.

---

### 6. Go Binary Not Available (Crashes)

**Description:** Go runtime binary not found or not executable, causing complete system failure in Go-only mode.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Errors** | `tests/unit/test_go_bridge.py:24-26` | `GoNotAvailableError` |
| **Logs** | Go bridge initialization | Error logs |
| **Tests** | `tests/unit/test_go_bridge.py` | Tests raise behavior |

**Current Code Surfaces It:** ✅ **YES**

```python
# Per HANDOFF.md - Go-Only Mode
# Go binary is REQUIRED - raises GoNotAvailableError if not found
bridge = GoEnvelopeBridge()  # Raises if binary missing
```

**Detection Gaps:**
- No pre-flight health check at startup
- No graceful degradation path (by design per Go-Only Mode)

---

### 7. Parallel Stage Race Conditions (Silent Corruption)

**Description:** Concurrent stage execution in parallel mode causes data races when merging outputs.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Tests** | `tests/integration/test_interrupt_flow.py:220-286` | Race condition tests |
| **Code** | `coreengine/runtime/runtime.go:386-389` | Mutex-protected merge |

**Current Code Surfaces It:** ✅ **YES (Protected)**

```go
// coreengine/runtime/runtime.go:386-389
mu.Lock()
env.SetOutput(result.Stage, result.Output)
completed[result.Stage] = true
mu.Unlock()
```

**Detection Gaps:**
- Go's race detector not run in CI (based on test files)
- No deadlock detection for complex stage dependencies

**Recommendation:** Add `-race` flag to Go CI tests.

---

### 8. Session Dedup Cache Unbounded Growth (Memory Leak)

**Description:** Session deduplication cache grows unbounded as sessions accumulate.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Logs** | `jeeves_memory_module/services/event_emitter.py:137-141` | `session_dedup_cache_full` warning |
| **Metrics** | `SessionDedupCache.session_count` property | Returns active session count |

**Current Code Surfaces It:** ⚠️ **PARTIAL**

```python
# jeeves_memory_module/services/event_emitter.py:136-144
if len(self._caches[session_id]) >= self._max_size:
    self._logger.warning(
        "session_dedup_cache_full",
        session_id=session_id,
        size=len(self._caches[session_id])
    )
    # Clear half the cache (simple eviction strategy)
    entries = list(self._caches[session_id])
    self._caches[session_id] = set(entries[len(entries) // 2:])
```

**Detection Gaps:**
- Per-session limit (10,000) but no cross-session limit
- `_caches` dict can grow indefinitely with many sessions
- No automatic session cleanup on session end

**Recommendation:** Add `clear_session()` calls to session lifecycle; add total entry limit.

---

### 9. Tool Execution Timeout (Crashes/Hangs)

**Description:** Tool execution hangs indefinitely without timeout, blocking pipeline progress.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Metrics** | `ResourceTracker.check_quota()` | Checks `elapsed_seconds` vs `timeout_seconds` |
| **Logs** | `jeeves_control_tower/resources/tracker.py:180-187` | `approaching_timeout` warning |
| **Code** | `coreengine/tools/executor.go:52-61` | Context-based timeout |

**Current Code Surfaces It:** ⚠️ **PARTIAL**

```go
// coreengine/tools/executor.go:52-61 - Uses context but no explicit timeout
func (e *ToolExecutor) Execute(ctx context.Context, toolName string, params map[string]any) (map[string]any, error) {
    // Relies on caller-provided context for timeout
    return def.Handler(ctx, params)
}
```

**Detection Gaps:**
- Tool executor relies on caller context timeout (not enforced internally)
- ResourceTracker warns at 80% but doesn't enforce hard cutoff
- No circuit breaker for repeatedly failing tools

---

### 10. Interrupt State Corruption (Wrong Routing/Silent Corruption)

**Description:** Interrupt response delivered to wrong request or interrupt state becomes inconsistent.

**Detection Signals:**
| Signal Type | Location | Implementation |
|-------------|----------|----------------|
| **Logs** | `jeeves_control_tower/kernel.py:356-360` | `handling_interrupt` with kind |
| **Tests** | `tests/integration/test_interrupt_flow.py:228` | Concurrent creation test |
| **Tests** | `jeeves_control_tower/tests/unit/test_interrupt_service.py` | Service unit tests |

**Current Code Surfaces It:** ✅ **YES**

```python
# jeeves_control_tower/kernel.py:451-460
resolved = await self._interrupts.respond(
    interrupt_id=interrupt_id,
    response=response,
    user_id=pcb.user_id,
)

if not resolved:
    self._logger.error(
        "interrupt_resolve_failed",
        interrupt_id=interrupt_id,
    )
```

**Detection Gaps:**
- No validation that interrupt_id belongs to the request being resumed
- Database persistence for interrupts is optional (`db` parameter)

---

## Summary Matrix

| # | Failure Mode | Type | Detection Signal | Surfaced? | Priority |
|---|--------------|------|------------------|-----------|----------|
| 1 | Pipeline Bounds Exceeded | Crash | Logs + Metrics | ✅ YES | P1 |
| 2 | Silent Event Emission | Silent Corruption | Logs only | ⚠️ PARTIAL | P1 |
| 3 | Message Routing Failures | Wrong Routing | Logs (varies) | ⚠️ PARTIAL | P2 |
| 4 | DB Pool Exhaustion | Crash | Health Check | ✅ YES | P1 |
| 5 | Rate Limiter Memory | Memory Leak | Manual cleanup | ⚠️ PARTIAL | P2 |
| 6 | Go Binary Missing | Crash | Error raised | ✅ YES | P1 |
| 7 | Parallel Stage Races | Silent Corruption | Mutex protected | ✅ YES | P1 |
| 8 | Session Cache Growth | Memory Leak | Warning log | ⚠️ PARTIAL | P2 |
| 9 | Tool Execution Timeout | Crash/Hang | Context timeout | ⚠️ PARTIAL | P1 |
| 10 | Interrupt Corruption | Wrong Routing | Logs + Tests | ✅ YES | P2 |

---

## Recommendations

### Immediate (P1 Gaps)

1. **Event Emission Failures (#2):** Add failure counter metric and optional strict mode
2. **Tool Execution Timeout (#9):** Add internal timeout enforcement in Go executor

### Short-term (P2 Gaps)

3. **Message Routing (#3):** Make command/event missing handler a configurable error
4. **Memory Cleanup (#5, #8):** Add automatic periodic cleanup tasks
5. **Go Race Detection (#7):** Add `-race` to CI test runs

### Long-term

6. **Unified Metrics Export:** Add Prometheus/OpenTelemetry metrics for all failure counters
7. **Pre-flight Health Checks:** Validate all dependencies at startup
8. **Circuit Breakers:** Add circuit breaker pattern for external calls (LLM, tools)

---

## Test Coverage

The following test files validate failure mode detection:

| Test File | Failure Modes Covered |
|-----------|----------------------|
| `tests/integration/test_interrupt_flow.py` | #7, #10 |
| `jeeves_mission_system/tests/contract/test_constitution_p2_reliability.py` | #2, #3 |
| `jeeves_memory_module/tests/unit/services/test_event_emitter.py` | #2 |
| `tests/unit/test_go_bridge.py` | #6 |
| `coreengine/runtime/runtime_test.go` | #1, #7 |
| `commbus/bus_test.go` | #3 |

---

*End of Failure Modes Analysis v1.0.0*
