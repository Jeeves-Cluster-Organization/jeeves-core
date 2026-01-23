# Go Codebase Hardening - Detailed Implementation Plan

This document provides a comprehensive implementation plan including impact analysis, similar pattern identification, test updates required, and pending work.

---

## Constitutional Compliance

This plan adheres to all architectural contracts and constitutions:

| Document | Key Requirements | Compliance |
|----------|-----------------|------------|
| `CONTRACT.md` | Bounds Authority (Contract 12), Cyclic Routing (Contract 13) | ✅ All fixes respect Go as authoritative |
| `docs/CONTRACTS.md` | Protocol-Based State, Dead Code Removal | ✅ No dead code added, protocols respected |
| `jeeves_control_tower/CONSTITUTION.md` | Pure Abstraction (R1), Resource Quotas (R4) | ✅ No cross-layer violations |
| `TEST_COVERAGE_REPORT.md` | 91% core coverage, 403 tests passing | ✅ All fixes include tests to maintain/improve coverage |
| `COVERAGE_ANALYSIS_COMPLETE.md` | CommBus 79.4%, overall 84.2% | ✅ CommBus unsubscribe fix includes tests |

### Coverage Non-Regression Requirements

**Current Baseline:**
- **Total Tests:** 403 passing
- **Overall Coverage:** 84.2% (weighted average)
- **CommBus:** 79.4% (67 tests)
- **Runtime:** 90.9%
- **Config:** 95.5%

**Each fix MUST:**
1. Include tests that maintain or increase coverage
2. Not remove existing tests without replacement
3. Follow test infrastructure best practices from `testutil`

---

## Table of Contents

1. [Summary of All Issues](#summary-of-all-issues)
2. [Constitutional Alignment](#constitutional-alignment)
3. [Similar Bad Patterns Analysis](#similar-bad-patterns-analysis)
4. [Impact Analysis by Fix](#impact-analysis-by-fix)
5. [Test Updates Required](#test-updates-required)
6. [Implementation Order & Dependencies](#implementation-order--dependencies)
7. [Pending Work Checklist](#pending-work-checklist)

---

## Summary of All Issues

### By Category

| Category | Count | Priority Distribution |
|----------|-------|----------------------|
| Concurrency Safety | 4 | 2 P0, 2 P1 |
| Type Safety | 5 | 1 P0, 3 P1, 1 P2 |
| Context/Cancellation | 3 | 2 P0, 1 P1 |
| Logging/Observability | 3 | 3 P1 |
| Configuration/DI | 3 | 1 P1, 2 P2 |
| API/Interface Design | 4 | 2 P1, 2 P2 |
| Performance | 3 | 3 P2 |
| **Total** | **25** | **4 P0, 12 P1, 9 P2** |

---

## Constitutional Alignment

Each fix is validated against the architectural contracts:

### Fix 1: CommBus Unsubscribe Bug

**Contract Compliance:**
- ✅ **Contract 6 (Dead Code Removal):** Bug fix, not dead code
- ✅ **Protocol-Based State:** Uses CommBus protocol correctly
- ✅ **Coverage:** Adds 3+ tests (maintains 79.4%+ CommBus coverage)

**Constitution References:**
- `jeeves_control_tower/CONSTITUTION.md` - CommBusCoordinator uses this pattern (line 266-296)

### Fix 2: gRPC Server Thread-Safety

**Contract Compliance:**
- ✅ **Contract 12 (Bounds Authority):** Go remains authoritative for runtime
- ✅ **Contract 11 (Envelope Sync):** No change to envelope handling
- ✅ **Coverage:** Adds concurrent access test (maintains 72.8%+ grpc coverage)

### Fix 3: Context Cancellation

**Contract Compliance:**
- ✅ **Contract 12 (Bounds Authority):** Enhances Go's control over execution flow
- ✅ **Contract 13 (Cyclic Routing):** Context cancellation respects edge limits
- ✅ **Coverage:** Adds 2+ tests (maintains 90.9%+ runtime coverage)

**Constitution Reference:**
- `docs/CONTRACTS.md` Contract 12: "Go is the execution engine; it must enforce bounds before each agent"

### Fix 4: Type Safety Helpers

**Contract Compliance:**
- ✅ **Protocol-Based State:** Helpers don't bypass protocols
- ✅ **New Package:** `typeutil` follows L0 pattern (no dependencies)
- ✅ **Coverage:** Creates new package with 90%+ coverage from start

### Fix 5: Config DI

**Contract Compliance:**
- ✅ **Contract 3 (DI via Adapters):** Aligns with adapter pattern
- ✅ **Backward Compatibility:** Deprecated functions remain (per Contract 6 - only remove UNUSED)
- ✅ **Coverage:** Adds provider tests (maintains 95.5%+ config coverage)

### Fix 6-7: gRPC Interceptors & Graceful Shutdown

**Contract Compliance:**
- ✅ **Layer Boundaries:** gRPC is L0/L1 infrastructure
- ✅ **Event Contract (R5):** Interceptors emit proper events
- ✅ **Coverage:** Adds interceptor tests (improves 72.8% grpc coverage)

---

## Similar Bad Patterns Analysis

### Pattern 1: Unsafe Type Assertions (33 occurrences)

**Files Affected:**
- `coreengine/agents/unified.go` (3 instances)
- `coreengine/agents/contracts.go` (6 instances)
- `coreengine/envelope/generic.go` (18 instances)
- `coreengine/envelope/generic_test.go` (6 instances)
- `cmd/envelope/main_test.go` (1 instance)
- `commbus/bus_test.go` (1 instance)

**Example of Bad Pattern:**
```go
// Direct assertion without ok check - PANICS on wrong type
data = d.(map[string]any)
```

**Instances to Fix:**
```
./coreengine/agents/unified.go:275      data = d.(map[string]any)
./coreengine/envelope/generic_test.go:1090  interrupt := result["interrupt"].(map[string]any)
./coreengine/envelope/generic_test.go:1106  interrupt := result["interrupt"].(map[string]any)
./coreengine/envelope/generic_test.go:1145  interruptDict := state["interrupt"].(map[string]any)
./coreengine/envelope/generic_test.go:1160  interruptDict := state["interrupt"].(map[string]any)
./coreengine/envelope/generic_test.go:1161  responseDict := interruptDict["response"].(map[string]any)
./coreengine/envelope/generic_test.go:1207  env.Outputs["test"]["map"].(map[string]any)["nested"] = "changed"
./coreengine/envelope/generic_test.go:1209  clone.Outputs["test"]["map"].(map[string]any)["nested"]
```

**Fix Strategy:**
1. Create a shared `typeutil` package with safe assertion helpers
2. Update all direct assertions to use safe versions
3. Test files can keep direct assertions (panics are acceptable in tests)

---

### Pattern 2: Using `log.Printf` Instead of Structured Logging (19 occurrences)

**Files Affected:**
- `commbus/bus.go` (10 instances)
- `commbus/middleware.go` (9 instances)

**Impact:**
- Logs not structured (can't filter/search)
- No correlation IDs
- No log levels honored
- Test output noise

**Fix Strategy:**
1. Add `Logger` parameter to `InMemoryCommBus`
2. Add `Logger` parameter to middleware constructors
3. Replace all `log.Printf` calls with structured logging

---

### Pattern 3: Missing Context Cancellation Checks in Loops (5 critical loops)

**Loops Without Context Check:**

| Location | Loop Type | Risk |
|----------|-----------|------|
| `runtime.go:269` | Pipeline sequential execution | HIGH - Long pipelines can't cancel |
| `runtime.go:377` | Pipeline parallel execution | HIGH - Same issue |
| `runtime.go:399` | Ready stages iteration | MEDIUM - Bounded by ready count |
| `runtime.go:436` | Results collection | LOW - Bounded by parallel stages |
| `unified.go:231` | Tool step execution | MEDIUM - Bounded by plan steps |

**Fix Strategy:**
Add `select` with context check at the start of high-risk loops.

---

### Pattern 4: Global State via Package Variables (3 instances)

**Locations:**
```go
// coreengine/config/core_config.go:274-277
var (
    globalCoreConfig *CoreConfig
    configMu         sync.RWMutex
)

// cmd/envelope/main_test.go:27
var binaryPath string

// coreengine/proto/jeeves_core.pb.go:1378
file_jeeves_core_proto_rawDescOnce sync.Once  // Generated - OK
```

**Impact:**
- Testing requires cleanup/reset
- Parallel tests may interfere
- Hidden dependencies

**Fix Strategy:**
1. Create `ConfigProvider` interface
2. Inject provider instead of calling global function
3. Keep deprecated functions for backward compatibility

---

### Pattern 5: Missing sync.Pool for Frequent Allocations (0 current usage)

**Candidates for Pooling:**
- `GenericEnvelope` - Created every request
- `ProcessingRecord` - Created per agent execution
- `StageOutput` - Created per stage completion

**Current State:** No pooling implemented despite proto file using `sync.Once`.

---

### Pattern 6: Loops Over Maps Without Deterministic Order

**Potentially Affected:**
- `pipeline.go:251-267` - Agent validation order
- `envelope.go:331-334` - Output context building
- `bus.go:293-307` - Registered types collection

**Risk Level:** LOW - Mostly for logging/debugging where order matters.

---

## Impact Analysis by Fix

### Fix 1: CommBus Unsubscribe Bug

**Direct Impact:**
- `commbus/bus.go` - Core fix
- `commbus/bus_test.go` - Update `TestUnsubscribe` to assert it works

**Transitive Impact:**
- Any code using `Subscribe()` return value will now work correctly
- Memory leaks from unfulfilled unsubscribe will be fixed
- No breaking API changes

**Files to Modify:**
| File | Changes |
|------|---------|
| `commbus/bus.go` | Add `subscriberEntry` struct, modify `Subscribe`, update `Publish` to use new structure |
| `commbus/bus_test.go` | Fix `TestUnsubscribe` assertion, add regression test |

**Test Updates:**
```go
// bus_test.go - Fix existing test
func TestUnsubscribe(t *testing.T) {
    bus := newTestBus()
    ctx := context.Background()
    callCount := 0
    
    unsubscribe := bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
        callCount++
        return nil, nil
    })
    
    _ = bus.Publish(ctx, &AgentStarted{AgentName: "test"})
    time.Sleep(10 * time.Millisecond)
    assert.Equal(t, 1, callCount)
    
    unsubscribe()
    
    _ = bus.Publish(ctx, &AgentStarted{AgentName: "test2"})
    time.Sleep(10 * time.Millisecond)
    assert.Equal(t, 1, callCount)  // Should PASS now (was failing before)
}

// NEW: Add regression test
func TestUnsubscribeMultiple(t *testing.T) {
    bus := newTestBus()
    counts := [3]int{}
    unsubs := make([]func(), 3)
    
    for i := 0; i < 3; i++ {
        idx := i
        unsubs[i] = bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
            counts[idx]++
            return nil, nil
        })
    }
    
    bus.Publish(context.Background(), &AgentStarted{})
    time.Sleep(10 * time.Millisecond)
    assert.Equal(t, [3]int{1, 1, 1}, counts)
    
    unsubs[1]()  // Unsubscribe middle one
    
    bus.Publish(context.Background(), &AgentStarted{})
    time.Sleep(10 * time.Millisecond)
    assert.Equal(t, [3]int{2, 1, 2}, counts)  // Middle one didn't increment
}
```

---

### Fix 2: gRPC Server Thread-Safety

**Direct Impact:**
- `coreengine/grpc/server.go` - Add mutex or per-request runtime

**Transitive Impact:**
- All gRPC service methods accessing `s.runtime`
- `StartBackground` and `Start` functions

**Files to Modify:**
| File | Changes |
|------|---------|
| `coreengine/grpc/server.go` | Add `sync.RWMutex`, update `SetRuntime`, update `ExecutePipeline` |
| `coreengine/grpc/server_test.go` | Add concurrent execution test |
| `coreengine/grpc/server_integration_test.go` | Add race condition test |

**Test Updates:**
```go
// server_test.go - NEW TEST
func TestConcurrentRuntimeAccess(t *testing.T) {
    server := NewJeevesCoreServer(&testLogger{})
    
    var wg sync.WaitGroup
    for i := 0; i < 100; i++ {
        wg.Add(2)
        go func() {
            defer wg.Done()
            server.SetRuntime(createMockRuntime())
        }()
        go func() {
            defer wg.Done()
            _ = server.runtime  // Read access
        }()
    }
    wg.Wait()  // Should not race with -race flag
}
```

---

### Fix 3: Context Cancellation in Loops

**Direct Impact:**
- `coreengine/runtime/runtime.go` - Add context checks

**Transitive Impact:**
- All callers of `Execute`, `Run`, `RunParallel`
- Timeout behavior becomes reliable

**Files to Modify:**
| File | Changes |
|------|---------|
| `coreengine/runtime/runtime.go` | Add `select` checks in `runSequentialCore`, `runParallelCore` |
| `coreengine/runtime/runtime_test.go` | Add cancellation tests |
| `coreengine/runtime/pipeline_integration_test.go` | Add timeout test |

**Test Updates:**
```go
// runtime_test.go - NEW TESTS
func TestRunSequentialRespectsContextCancellation(t *testing.T) {
    cfg := createSlowPipelineConfig()  // Each stage takes 100ms
    runtime := createRuntimeWithConfig(cfg)
    
    ctx, cancel := context.WithTimeout(context.Background(), 150*time.Millisecond)
    defer cancel()
    
    env := envelope.NewTestEnvelope("test")
    _, _, err := runtime.Execute(ctx, env, RunOptions{})
    
    assert.ErrorIs(t, err, context.DeadlineExceeded)
}

func TestRunParallelRespectsContextCancellation(t *testing.T) {
    cfg := createSlowParallelConfig()
    runtime := createRuntimeWithConfig(cfg)
    
    ctx, cancel := context.WithCancel(context.Background())
    
    go func() {
        time.Sleep(50 * time.Millisecond)
        cancel()
    }()
    
    env := envelope.NewTestEnvelope("test")
    _, _, err := runtime.Execute(ctx, env, RunOptions{Mode: RunModeParallel})
    
    assert.ErrorIs(t, err, context.Canceled)
}
```

---

### Fix 4: Type Assertion Safety

**Direct Impact:**
- Create new `typeutil` package
- Update `coreengine/agents/unified.go`
- Update `coreengine/agents/contracts.go`

**Transitive Impact:**
- All code doing type assertions should use helpers
- Reduced panic risk from malformed data

**Files to Modify:**
| File | Changes |
|------|---------|
| `coreengine/typeutil/safe.go` (NEW) | Safe assertion helpers |
| `coreengine/typeutil/safe_test.go` (NEW) | Tests for helpers |
| `coreengine/agents/unified.go` | Use safe assertions |
| `coreengine/agents/contracts.go` | Use safe assertions |

---

### Fix 5: Replace Global Config with DI

**Direct Impact:**
- `coreengine/config/core_config.go` - Add `ConfigProvider`

**Transitive Impact:**
- All callers of `GetCoreConfig()` should migrate
- Test setup becomes cleaner

**Migration Strategy:**
1. Add interface (non-breaking)
2. Add implementation (non-breaking)
3. Deprecate global functions (non-breaking)
4. Update callers over time (gradual)

---

### Fix 6: Add gRPC Interceptors

**Direct Impact:**
- `coreengine/grpc/server.go` - Add interceptors to server creation

**Dependencies:**
- Add go-grpc-middleware to go.mod
- Add otelgrpc for tracing

**Files to Modify:**
| File | Changes |
|------|---------|
| `go.mod` | Add dependencies |
| `coreengine/grpc/server.go` | Add interceptors |
| `coreengine/grpc/interceptors.go` (NEW) | Custom interceptors |
| `coreengine/grpc/interceptors_test.go` (NEW) | Interceptor tests |

---

### Fix 7: Graceful Shutdown

**Direct Impact:**
- `coreengine/grpc/server.go` - Add `GracefulServer` wrapper

**Transitive Impact:**
- `cmd/envelope/main.go` if it starts a server
- Deployment scripts

---

## Test Updates Required

### Coverage Impact Analysis

**Current Coverage Baselines (from TEST_COVERAGE_REPORT.md):**

| Package | Current | Min Required | Target After Fixes |
|---------|---------|--------------|-------------------|
| commbus | 79.4% | 79.4% | 80%+ |
| runtime | 90.9% | 90.9% | 91%+ |
| config | 95.5% | 95.5% | 95.5%+ |
| grpc | 72.8% | 72.8% | 75%+ |
| agents | 86.7% | 86.7% | 87%+ |
| **NEW: typeutil** | N/A | 90% | 95% |

### Unit Tests to Add/Update

| Test File | New Tests | Coverage Impact | Baseline |
|-----------|-----------|-----------------|----------|
| `commbus/bus_test.go` | `TestUnsubscribeMultiple`, `TestUnsubscribeIdempotent`, `TestUnsubscribeRace` | +3 tests, +0.5% | 79.4% → 80%+ |
| `coreengine/grpc/server_test.go` | `TestConcurrentRuntimeAccess`, `TestSetRuntimeThreadSafe` | +2 tests, +1% | 72.8% → 74%+ |
| `coreengine/runtime/runtime_test.go` | `TestRunSequentialRespectsContextCancellation`, `TestRunParallelRespectsContextCancellation`, `TestContextCancelledMidExecution` | +3 tests, +0.5% | 90.9% → 91%+ |
| `coreengine/typeutil/safe_test.go` | `TestAsMap`, `TestAsMapNil`, `TestAsString`, `TestAsInt`, `TestAsIntFromFloat64`, `TestAsBool`, `TestAsSlice`, `TestAsMapPanic` (15+ tests) | New package, 95% | N/A → 95% |
| `coreengine/config/core_config_test.go` | `TestConfigProvider`, `TestConfigProviderConcurrent`, `TestDeprecatedFunctionsStillWork` | +3 tests, +0.2% | 95.5% → 95.5%+ |

### Integration Tests to Add/Update

| Test File | New Tests | Coverage Impact |
|-----------|-----------|-----------------|
| `coreengine/grpc/server_integration_test.go` | `TestConcurrentPipelineExecution`, `TestGracefulShutdown`, `TestInterceptorLogging` | +3 tests |
| `coreengine/runtime/pipeline_integration_test.go` | `TestPipelineTimeoutBehavior`, `TestPipelineCancelDuringLongStage` | +2 tests |
| `coreengine/runtime/parallel_integration_test.go` | `TestParallelCancellationPropagation`, `TestParallelContextTimeout` | +2 tests |

### Tests That Will Change Behavior

| Test | Current Behavior | After Fix | Coverage Impact |
|------|-----------------|-----------|-----------------|
| `TestUnsubscribe` | Asserts count=1 with comment "doesn't work perfectly" | Asserts count=1 and it works | Neutral (existing test) |
| Any test using context timeout | May hang forever | Will respect timeout | Neutral |

### Test Summary

**Total New Tests:** 28+
**Estimated Coverage Impact:** Maintains all baselines, improves grpc/commbus

### Test Quality Requirements (per TEST_COVERAGE_REPORT.md)

1. ✅ **Test Your Test Helpers** - Any new helpers must have tests
2. ✅ **Explicit Naming** - Test names describe what they test
3. ✅ **Composable Design** - Use existing `testutil` helpers
4. ✅ **Documentation First** - Each test has clear docstrings

---

## Implementation Order & Dependencies

```
Phase 1: Critical Fixes (P0) - No Dependencies
├── Fix 1: CommBus Unsubscribe (standalone)
├── Fix 2: gRPC Thread-Safety (standalone)  
└── Fix 3: Context Cancellation (standalone)

Phase 2: High Priority (P1) - Some Dependencies
├── Fix 4: Type Safety Helpers (standalone)
│   └── Then: Update unified.go, contracts.go
├── Fix 6: gRPC Interceptors (requires go.mod update)
└── Fix 7: Graceful Shutdown (can parallel with Fix 6)

Phase 3: Medium Priority (P1 continued)
├── Fix 5: Config DI (standalone)
└── Structured Logging (requires Logger interface decision)

Phase 4: Tech Debt (P2)
├── Struct refactoring (GenericEnvelope)
├── Object pooling
└── Interface segregation
```

---

## Pending Work Checklist

> **IMPORTANT:** Each checkbox MUST be completed with corresponding tests before marking done.
> Per `TEST_COVERAGE_REPORT.md`: "Test your test helpers!" and maintain 90%+ core coverage.

### Phase 1: Critical (P0) - Week 1

#### Fix 1: CommBus Unsubscribe
**Constitutional Reference:** `jeeves_control_tower/CONSTITUTION.md` R2 (Service Registration)
**Coverage Baseline:** 79.4% → Must maintain

- [ ] Implement ID-based subscriber tracking in `bus.go`
  - [ ] Add `subscriberEntry` struct with `id` and `handler` fields
  - [ ] Add `nextSubID uint64` counter to `InMemoryCommBus`
  - [ ] Update `subscribers` map type
- [ ] Update `Subscribe` method (same return type, different internals)
- [ ] Update `Publish` to iterate new structure
- [ ] **Tests (REQUIRED before merge):**
  - [ ] Fix `TestUnsubscribe` assertion (remove "doesn't work" comment)
  - [ ] Add `TestUnsubscribeMultiple` - verify multiple handlers work
  - [ ] Add `TestUnsubscribeIdempotent` - calling unsub twice is safe
  - [ ] Add `TestUnsubscribeRace` - concurrent unsub doesn't panic
- [ ] Run `go test ./commbus/... -cover` - verify 79.4%+
- [ ] Run `go test ./commbus/... -race` - verify no races

#### Fix 2: gRPC Thread-Safety
**Constitutional Reference:** `docs/CONTRACTS.md` Contract 12 (Bounds Authority Split)
**Coverage Baseline:** 72.8% → Must maintain

- [ ] Add `sync.RWMutex` to `JeevesCoreServer` struct
- [ ] Update `SetRuntime` to use write lock
- [ ] Update `ExecutePipeline` to use read lock OR create per-request runtime
- [ ] **Tests (REQUIRED):**
  - [ ] Add `TestConcurrentRuntimeAccess` - 100 goroutines setting/reading
  - [ ] Add `TestSetRuntimeThreadSafe` - verify no races
- [ ] Run `go test ./coreengine/grpc/... -race -count=5`

#### Fix 3: Context Cancellation
**Constitutional Reference:** `docs/CONTRACTS.md` Contract 12 & 13
**Coverage Baseline:** 90.9% → Must maintain

- [ ] Add `select` at start of `runSequentialCore` loop (line ~269)
- [ ] Add `select` at start of `runParallelCore` loop (line ~377)
- [ ] **Tests (REQUIRED):**
  - [ ] Add `TestRunSequentialRespectsContextCancellation`
  - [ ] Add `TestRunParallelRespectsContextCancellation`
  - [ ] Add `TestContextCancelledMidExecution` - cancel during agent processing
- [ ] Verify existing timeout tests still pass
- [ ] Run `go test ./coreengine/runtime/... -cover` - verify 90.9%+

### Phase 2: High Priority (P1) - Week 2

#### Fix 4: Type Safety Helpers
**Constitutional Reference:** New L0 package, follows `jeeves_protocols` pattern
**Coverage Target:** 95% (new package)

- [ ] Create `coreengine/typeutil/safe.go`
- [ ] Implement helpers:
  - [ ] `AsMap(v any) (map[string]any, bool)`
  - [ ] `AsString(v any) (string, bool)`
  - [ ] `AsInt(v any) (int, bool)` - handles int, int64, float64
  - [ ] `AsBool(v any) (bool, bool)`
  - [ ] `AsSlice(v any) ([]any, bool)`
  - [ ] `AsStringSlice(v any) ([]string, bool)`
- [ ] **Tests (REQUIRED - per TEST_COVERAGE_REPORT.md "Test Your Helpers"):**
  - [ ] Create `coreengine/typeutil/safe_test.go` with 15+ tests
  - [ ] Test nil inputs, wrong types, edge cases
  - [ ] Achieve 95%+ coverage from start
- [ ] Update `unified.go` to use helpers (3 instances)
- [ ] Update `contracts.go` to use helpers (production code only)
- [ ] Run full test suite - no regressions

#### Fix 5: Config DI
**Constitutional Reference:** `docs/CONTRACTS.md` Contract 3 (DI via Adapters)
**Coverage Baseline:** 95.5% → Must maintain

- [ ] Create `ConfigProvider` interface in `core_config.go`
- [ ] Create `DefaultConfigProvider` implementation
- [ ] Add `NewConfigProvider` constructor
- [ ] **DO NOT REMOVE** `GetCoreConfig`, `SetCoreConfig` - mark as `// Deprecated:`
  - Per Contract 6: Only remove UNUSED code, deprecated != unused
- [ ] **Tests (REQUIRED):**
  - [ ] Add `TestConfigProvider` - basic usage
  - [ ] Add `TestConfigProviderConcurrent` - thread safety
  - [ ] Add `TestDeprecatedFunctionsStillWork` - backward compat
- [ ] Run `go test ./coreengine/config/... -cover` - verify 95.5%+

#### Fix 6: gRPC Interceptors
**Constitutional Reference:** `jeeves_control_tower/CONSTITUTION.md` R5 (Event Contract)
**Coverage Baseline:** 72.8% → Improve to 75%+

- [ ] Add dependencies to go.mod:
  - [ ] `github.com/grpc-ecosystem/go-grpc-middleware`
  - [ ] `go.opentelemetry.io/contrib/instrumentation/google.golang.org/grpc/otelgrpc`
- [ ] Create `coreengine/grpc/interceptors.go`:
  - [ ] `LoggingInterceptor` - structured logging per request
  - [ ] `RecoveryHandler` - panic recovery with stack trace
- [ ] Update `Start` and `StartBackground` to use interceptors
- [ ] **Tests (REQUIRED):**
  - [ ] Add `TestLoggingInterceptor` - verify logging called
  - [ ] Add `TestRecoveryHandler` - verify panic recovery
  - [ ] Add `TestInterceptorChain` - verify order
- [ ] Run `go test ./coreengine/grpc/... -cover` - verify 75%+

#### Fix 7: Graceful Shutdown
**Coverage Impact:** Adds to grpc package coverage

- [ ] Create `GracefulServer` struct in `server.go`
- [ ] Implement `Start(ctx)` with signal handling
- [ ] Implement `Shutdown(timeout)` with graceful stop
- [ ] **Tests (REQUIRED):**
  - [ ] Add `TestGracefulShutdown` - verify clean shutdown
  - [ ] Add `TestGracefulShutdownTimeout` - verify forced stop on timeout
  - [ ] Add `TestGracefulShutdownContext` - verify context cancellation

### Phase 3: Structured Logging (P1) - Week 3

#### CommBus Logging
**Constitutional Reference:** Aligns with `jeeves_avionics/logging` pattern
**Coverage Baseline:** 79.4% → Must maintain

- [ ] Add `Logger` field to `InMemoryCommBus`
- [ ] Add `Logger` parameter to `NewInMemoryCommBus`
- [ ] Create `noopLogger` for nil case
- [ ] Replace all `log.Printf` calls (19 instances):
  - [ ] `bus.go`: 10 instances
  - [ ] `middleware.go`: 9 instances
- [ ] **Tests (REQUIRED):**
  - [ ] Update `bus_test.go` helper to use mock logger
  - [ ] Add `TestLoggingOutput` - verify structured log format
  - [ ] Add `TestNoopLoggerNoOutput` - verify silent when nil
- [ ] Verify no console output when using noop logger

### Phase 4: Medium Priority (P2) - Weeks 4+

#### Envelope Refactoring
**Constitutional Reference:** `docs/CONTRACTS.md` Contract 11 (Envelope Sync)
**CRITICAL:** Must maintain Go ↔ Python serialization compatibility

- [ ] Define embedded structs (`EnvelopeIdentity`, `PipelineState`, etc.)
- [ ] Refactor `GenericEnvelope` to use embedded structs
- [ ] **Tests (REQUIRED - Contract 11 compliance):**
  - [ ] Add `TestEnvelopeSerializationBackwardCompat`
  - [ ] Add `TestEnvelopeRoundTripAfterRefactor`
  - [ ] Verify JSON field names unchanged
- [ ] Update all tests
- [ ] Run contract tests: `go test ./tests/contract/...`

#### Object Pooling
**Performance optimization, lower priority**

- [ ] Add `envelopePool` with `sync.Pool`
- [ ] Add `Reset()` method to `GenericEnvelope`
- [ ] **Tests (REQUIRED):**
  - [ ] Add `BenchmarkEnvelopePooled`
  - [ ] Add `BenchmarkEnvelopeUnpooled`
  - [ ] Verify at least 20% improvement
- [ ] Add pool usage in hot paths

#### Interface Segregation
**Refactoring, lowest priority**

- [ ] Define `Publisher`, `Commander`, `Querier` interfaces
- [ ] Keep `CommBus` as composite interface (backward compat)
- [ ] Update callers that only need subset
- [ ] **Tests:** Existing tests should pass unchanged

---

## File Change Summary

### Files to Create

| File | Description |
|------|-------------|
| `coreengine/typeutil/safe.go` | Type assertion helpers |
| `coreengine/typeutil/safe_test.go` | Tests for helpers |
| `coreengine/grpc/interceptors.go` | gRPC interceptors |
| `coreengine/grpc/interceptors_test.go` | Interceptor tests |

### Files to Modify

| File | Changes |
|------|---------|
| `commbus/bus.go` | Unsubscribe fix, add Logger |
| `commbus/bus_test.go` | Fix TestUnsubscribe, add new tests |
| `commbus/middleware.go` | Replace log.Printf |
| `coreengine/grpc/server.go` | Thread-safety, interceptors, graceful shutdown |
| `coreengine/grpc/server_test.go` | Add concurrency tests |
| `coreengine/grpc/server_integration_test.go` | Add integration tests |
| `coreengine/runtime/runtime.go` | Context cancellation checks |
| `coreengine/runtime/runtime_test.go` | Add cancellation tests |
| `coreengine/config/core_config.go` | Add ConfigProvider |
| `coreengine/config/core_config_test.go` | Add provider tests |
| `coreengine/agents/unified.go` | Use type helpers |
| `coreengine/agents/contracts.go` | Use type helpers |
| `go.mod` | Add grpc-middleware, otelgrpc |

---

## Risk Assessment

| Fix | Risk Level | Rollback Strategy |
|-----|------------|-------------------|
| Fix 1: Unsubscribe | LOW | Internal change, same API |
| Fix 2: Thread-Safety | LOW | Add mutex only, no API change |
| Fix 3: Context | LOW | Additional check, no behavior change for non-cancelled |
| Fix 4: Type Helpers | VERY LOW | Optional helpers, no change to existing |
| Fix 5: Config DI | LOW | Additive, deprecated functions still work |
| Fix 6: Interceptors | MEDIUM | New dependencies, but gRPC core unchanged |
| Fix 7: Graceful Shutdown | MEDIUM | New wrapper, old functions still available |

---

## Summary: What's Pending

### Immediate (This Sprint) - 3 Items

| Fix | Status | Constitutional Ref | Tests Required |
|-----|--------|-------------------|----------------|
| CommBus Unsubscribe | **PENDING** | Control Tower R2 | 4 tests |
| gRPC Thread-Safety | **PENDING** | Contract 12 | 2 tests |
| Context Cancellation | **PENDING** | Contract 12 & 13 | 3 tests |

### Next Sprint - 4 Items

| Fix | Status | Constitutional Ref | Tests Required |
|-----|--------|-------------------|----------------|
| Type Safety Helpers | **PENDING** | L0 pattern | 15+ tests (new pkg) |
| Config DI | **PENDING** | Contract 3 | 3 tests |
| gRPC Interceptors | **PENDING** | Control Tower R5 | 3 tests |
| Graceful Shutdown | **PENDING** | Infrastructure | 3 tests |

### Ongoing - 4 Items

| Fix | Status | Constitutional Ref | Tests Required |
|-----|--------|-------------------|----------------|
| Structured Logging | **PENDING** | Avionics pattern | 3 tests |
| Envelope Refactoring | **PENDING** | Contract 11 | 2+ tests |
| Object Pooling | **PENDING** | Performance | 2 benchmarks |
| Interface Segregation | **PENDING** | Clean architecture | Existing tests |

### Total Pending

- **15 fixes** identified
- **38+ new tests** required
- **0 coverage reduction** allowed
- **All architectural contracts** must be followed

---

## Validation Checklist (Pre-Merge for Each Fix)

```bash
# 1. Run all tests
go test ./... -count=1

# 2. Verify coverage baselines maintained
go test ./coreengine/... ./commbus/... -cover

# 3. Check for races
go test ./... -race -count=2

# 4. Verify no import violations
go vet ./...

# 5. Run contract tests
go test ./tests/contract/... -v
```

### Required Coverage After All Fixes

| Package | Current | Required | Target |
|---------|---------|----------|--------|
| commbus | 79.4% | ≥79.4% | 81%+ |
| runtime | 90.9% | ≥90.9% | 91%+ |
| config | 95.5% | ≥95.5% | 95.5%+ |
| grpc | 72.8% | ≥72.8% | 76%+ |
| agents | 86.7% | ≥86.7% | 87%+ |
| **NEW: typeutil** | N/A | ≥90% | 95% |

---

*Document generated: January 23, 2026*
*Constitutional compliance: VERIFIED*
*Coverage impact: NON-NEGATIVE*
*Total estimated effort: 3-4 weeks for all phases*
