# Go Codebase Hardening - Detailed Implementation Plan

This document provides a comprehensive implementation plan including impact analysis, similar pattern identification, test updates required, and pending work.

---

## Table of Contents

1. [Summary of All Issues](#summary-of-all-issues)
2. [Similar Bad Patterns Analysis](#similar-bad-patterns-analysis)
3. [Impact Analysis by Fix](#impact-analysis-by-fix)
4. [Test Updates Required](#test-updates-required)
5. [Implementation Order & Dependencies](#implementation-order--dependencies)
6. [Pending Work Checklist](#pending-work-checklist)

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

### Unit Tests to Add/Update

| Test File | New Tests | Updated Tests |
|-----------|-----------|---------------|
| `commbus/bus_test.go` | `TestUnsubscribeMultiple`, `TestUnsubscribeIdempotent` | `TestUnsubscribe` (fix assertion) |
| `coreengine/grpc/server_test.go` | `TestConcurrentRuntimeAccess` | - |
| `coreengine/runtime/runtime_test.go` | `TestRunSequentialRespectsContextCancellation`, `TestRunParallelRespectsContextCancellation` | - |
| `coreengine/typeutil/safe_test.go` | All new (15+ tests) | - |
| `coreengine/config/core_config_test.go` | `TestConfigProvider` | - |

### Integration Tests to Add/Update

| Test File | New Tests | Updated Tests |
|-----------|-----------|---------------|
| `coreengine/grpc/server_integration_test.go` | `TestConcurrentPipelineExecution`, `TestGracefulShutdown` | - |
| `coreengine/runtime/pipeline_integration_test.go` | `TestPipelineTimeoutBehavior` | - |
| `coreengine/runtime/parallel_integration_test.go` | `TestParallelCancellationPropagation` | - |

### Tests That Will Change Behavior

| Test | Current Behavior | After Fix |
|------|-----------------|-----------|
| `TestUnsubscribe` | Asserts count=1 with comment "doesn't work perfectly" | Asserts count=1 and it works |
| Any test using context timeout | May hang forever | Will respect timeout |

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

### Phase 1: Critical (P0) - Week 1

- [ ] **Fix 1: CommBus Unsubscribe**
  - [ ] Implement ID-based subscriber tracking in `bus.go`
  - [ ] Update `Subscribe` method signature (same return type)
  - [ ] Update `Publish` to iterate new structure
  - [ ] Fix `TestUnsubscribe` assertion
  - [ ] Add `TestUnsubscribeMultiple` test
  - [ ] Add `TestUnsubscribeIdempotent` test
  - [ ] Run existing tests to ensure no regression

- [ ] **Fix 2: gRPC Thread-Safety**
  - [ ] Add `sync.RWMutex` to `JeevesCoreServer` struct
  - [ ] Update `SetRuntime` to use write lock
  - [ ] Update `ExecutePipeline` to use read lock OR create per-request runtime
  - [ ] Add `TestConcurrentRuntimeAccess` test
  - [ ] Run with `-race` flag

- [ ] **Fix 3: Context Cancellation**
  - [ ] Add `select` at start of `runSequentialCore` loop
  - [ ] Add `select` at start of `runParallelCore` loop
  - [ ] Add `TestRunSequentialRespectsContextCancellation`
  - [ ] Add `TestRunParallelRespectsContextCancellation`
  - [ ] Verify existing timeout tests still pass

### Phase 2: High Priority (P1) - Week 2

- [ ] **Fix 4: Type Safety Helpers**
  - [ ] Create `coreengine/typeutil/safe.go`
  - [ ] Implement `AsMap`, `AsString`, `AsInt`, `AsBool`, `AsSlice` helpers
  - [ ] Add comprehensive tests in `safe_test.go`
  - [ ] Update `unified.go` to use helpers (3 instances)
  - [ ] Update `contracts.go` to use helpers (safe instances only, keep test file as-is)

- [ ] **Fix 5: Config DI**
  - [ ] Create `ConfigProvider` interface in `core_config.go`
  - [ ] Create `DefaultConfigProvider` implementation
  - [ ] Add `NewConfigProvider` constructor
  - [ ] Deprecate (don't remove) `GetCoreConfig`, `SetCoreConfig`
  - [ ] Add `TestConfigProvider` tests
  - [ ] Update one caller as example

- [ ] **Fix 6: gRPC Interceptors**
  - [ ] Add `github.com/grpc-ecosystem/go-grpc-middleware` to go.mod
  - [ ] Add `go.opentelemetry.io/contrib/instrumentation/google.golang.org/grpc/otelgrpc` to go.mod
  - [ ] Create `interceptors.go` with `LoggingInterceptor`, `RecoveryHandler`
  - [ ] Update `Start` and `StartBackground` to use interceptors
  - [ ] Add interceptor tests

- [ ] **Fix 7: Graceful Shutdown**
  - [ ] Create `GracefulServer` struct
  - [ ] Implement `Start(ctx)` with signal handling
  - [ ] Implement `Shutdown(timeout)` with graceful stop
  - [ ] Add shutdown tests
  - [ ] Update any main.go files that start servers

### Phase 3: Structured Logging (P1) - Week 3

- [ ] **CommBus Logging**
  - [ ] Add `Logger` field to `InMemoryCommBus`
  - [ ] Add `Logger` parameter to `NewInMemoryCommBus`
  - [ ] Create `noopLogger` for nil case
  - [ ] Replace all `log.Printf` calls (19 instances)
  - [ ] Update `bus_test.go` to use mock logger
  - [ ] Verify no output when using noop logger

### Phase 4: Medium Priority (P2) - Weeks 4+

- [ ] **Envelope Refactoring**
  - [ ] Define embedded structs (`EnvelopeIdentity`, `PipelineState`, etc.)
  - [ ] Refactor `GenericEnvelope` to use embedded structs
  - [ ] Update all tests
  - [ ] Verify JSON serialization compatibility

- [ ] **Object Pooling**
  - [ ] Add `envelopePool` with `sync.Pool`
  - [ ] Add `Reset()` method to `GenericEnvelope`
  - [ ] Benchmark to verify improvement
  - [ ] Add pool usage in hot paths

- [ ] **Interface Segregation**
  - [ ] Define `Publisher`, `Commander`, `Querier` interfaces
  - [ ] Keep `CommBus` as composite interface
  - [ ] Update callers that only need subset

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

*Document generated: January 23, 2026*
*Total estimated effort: 3-4 weeks for all phases*
