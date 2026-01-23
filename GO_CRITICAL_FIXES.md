# Critical Go Fixes - Implementation Guide

**Status:** COMPLETE  
**Date:** 2026-01-23  
**All fixes implemented with tests**

This document provides specific, actionable code changes for the most critical issues identified in the codebase analysis.

---

## Summary: All Fixes Implemented

| Fix | Status | Tests Added | Coverage Impact |
|-----|--------|-------------|-----------------|
| Fix 1: CommBus Unsubscribe | COMPLETE | 4 | Maintained 77.9% |
| Fix 2: gRPC Thread-Safety | COMPLETE | 2 | Maintained 67.8% |
| Fix 3: Context Cancellation | COMPLETE | 3 | Improved 91.2% |
| Fix 4: Type Safety | COMPLETE | 22+ | NEW: 86.7% |
| Fix 5: Config DI | COMPLETE | 4 | Improved 95.6% |
| Fix 6: gRPC Interceptors | COMPLETE | 10 | Improved grpc |
| Fix 7: Graceful Shutdown | COMPLETE | 4 | Improved grpc |
| Fix 8: Structured Logging | COMPLETE | - | Maintained |

---

## Fix 1: CommBus Unsubscribe Function Bug (P0) - COMPLETE

### Problem (FIXED)
The unsubscribe function compared function pointer addresses incorrectly:

```go
// BEFORE - BROKEN
for i, h := range subs {
    if &h == &handler {  // This NEVER works - h is a new variable each iteration
        b.subscribers[eventType] = append(subs[:i], subs[i+1:]...)
        break
    }
}
```

### Solution (IMPLEMENTED)
Used ID-based subscriber tracking with atomic counter:

```go
// AFTER - FIXED in commbus/bus.go
type subscriberEntry struct {
    id      string
    handler HandlerFunc
}

type InMemoryCommBus struct {
    subscribers  map[string][]subscriberEntry  // Changed type
    nextSubID    uint64                        // Atomic counter for unique IDs
    logger       BusLogger                     // Structured logging
    // ... other fields
}

func (b *InMemoryCommBus) Subscribe(eventType string, handler HandlerFunc) func() {
    b.mu.Lock()
    
    // Generate unique ID atomically
    id := atomic.AddUint64(&b.nextSubID, 1)
    subID := fmt.Sprintf("sub_%d", id)
    
    // ... subscribe with ID
    
    return func() {
        b.mu.Lock()
        defer b.mu.Unlock()
        for i, entry := range subs {
            if entry.id == subID {  // Match by ID, not function pointer
                b.subscribers[eventType] = append(subs[:i], subs[i+1:]...)
                return
            }
        }
    }
}
```

### Tests Added
- `TestUnsubscribe` - Fixed assertion (now verifies count stays at 1)
- `TestUnsubscribeMultiple` - Multiple handlers unsubscribe independently
- `TestUnsubscribeIdempotent` - Calling unsubscribe twice is safe
- `TestUnsubscribeRace` - Concurrent unsubscribe is thread-safe

---

## Fix 2: Thread-Safety for gRPC Server Runtime (P0) - COMPLETE

### Problem (FIXED)
`ExecutePipeline` accessed shared `s.runtime` field without synchronization.

### Solution (IMPLEMENTED)
Added mutex protection with thread-safe accessor:

```go
// AFTER - FIXED in coreengine/grpc/server.go
type JeevesCoreServer struct {
    pb.UnimplementedJeevesCoreServiceServer
    logger    Logger
    runtime   *runtime.Runtime
    runtimeMu sync.RWMutex  // Added mutex protection
}

func (s *JeevesCoreServer) SetRuntime(rt *runtime.Runtime) {
    s.runtimeMu.Lock()
    defer s.runtimeMu.Unlock()
    s.runtime = rt
}

func (s *JeevesCoreServer) getRuntime() *runtime.Runtime {
    s.runtimeMu.RLock()
    defer s.runtimeMu.RUnlock()
    return s.runtime
}
```

### Tests Added
- `TestSetRuntimeThreadSafe` - 100 concurrent SetRuntime calls
- `TestGetRuntimeThreadSafe` - Concurrent read/write operations

---

## Fix 3: Context Cancellation in Execution Loops (P0) - COMPLETE

### Problem (FIXED)
Sequential and parallel execution didn't check context at loop start.

### Solution (IMPLEMENTED)
Added context cancellation checks at the start of each loop iteration:

```go
// AFTER - FIXED in coreengine/runtime/runtime.go

func (r *Runtime) runSequentialCore(ctx context.Context, env *envelope.GenericEnvelope, ...) (*envelope.GenericEnvelope, error) {
    for env.CurrentStage != "end" && !env.Terminated {
        // CRITICAL: Check context at start of each iteration
        select {
        case <-ctx.Done():
            r.Logger.Info("pipeline_sequential_cancelled",
                "envelope_id", env.EnvelopeID,
                "current_stage", env.CurrentStage,
            )
            return env, ctx.Err()
        default:
        }
        // ... rest of loop
    }
}

func (r *Runtime) runParallelCore(ctx context.Context, env *envelope.GenericEnvelope, ...) (*envelope.GenericEnvelope, error) {
    for !env.Terminated {
        // CRITICAL: Check context at start of each iteration
        select {
        case <-ctx.Done():
            r.Logger.Info("pipeline_parallel_cancelled",
                "envelope_id", env.EnvelopeID,
            )
            return env, ctx.Err()
        default:
        }
        // ... rest of loop
    }
}
```

### Tests Added
- `TestSequentialCancellation` - Immediate cancellation returns error
- `TestParallelCancellation` - Immediate cancellation returns error
- `TestSequentialCancellationDuringExecution` - Mid-execution cancellation

---

## Fix 4: Type Assertion Safety (P1) - COMPLETE

### Problem (FIXED)
Unsafe type assertions could cause panics on malformed data.

### Solution (IMPLEMENTED)
Created new `coreengine/typeutil` package with safe assertion helpers:

```go
// NEW FILE: coreengine/typeutil/safe.go

// Safe assertion with ok pattern
func SafeMapStringAny(v any) (map[string]any, bool) {
    if v == nil {
        return nil, false
    }
    m, ok := v.(map[string]any)
    return m, ok
}

// With default value
func SafeStringDefault(v any, defaultValue string) string {
    if s, ok := SafeString(v); ok {
        return s
    }
    return defaultValue
}

// Handles int, int64, float64
func SafeInt(v any) (int, bool) {
    switch n := v.(type) {
    case int:
        return n, true
    case int64:
        return int(n), true
    case float64:
        return int(n), true
    default:
        return 0, false
    }
}

// Nested value access
func GetNestedString(data map[string]any, path string) (string, bool) {
    v, ok := GetNestedValue(data, path)
    if !ok {
        return "", false
    }
    return SafeString(v)
}
```

### Tests Added
- 22+ tests covering all helper functions
- Coverage: 86.7%

---

## Fix 5: Replace Global Config with DI (P1) - COMPLETE

### Problem (FIXED)
Global state made testing difficult.

### Solution (IMPLEMENTED)
Created `ConfigProvider` interface for dependency injection:

```go
// AFTER - FIXED in coreengine/config/core_config.go

type ConfigProvider interface {
    GetCoreConfig() *CoreConfig
}

type DefaultConfigProvider struct{}

func (DefaultConfigProvider) GetCoreConfig() *CoreConfig {
    return GetGlobalCoreConfig()
}

type StaticConfigProvider struct {
    config *CoreConfig
}

func NewStaticConfigProvider(cfg *CoreConfig) *StaticConfigProvider {
    return &StaticConfigProvider{config: cfg}
}

func (p *StaticConfigProvider) GetCoreConfig() *CoreConfig {
    if p.config == nil {
        return DefaultCoreConfig()
    }
    return p.config
}

// Deprecated: Use ConfigProvider interface instead
func GetCoreConfig() *CoreConfig {
    return GetGlobalCoreConfig()
}
```

### Tests Added
- `TestDefaultConfigProvider` - Uses global config
- `TestStaticConfigProvider` - Uses static config
- `TestStaticConfigProviderNil` - Handles nil config
- `TestConfigProviderInterface` - Interface compliance

---

## Fix 6: Add gRPC Interceptors (P1) - COMPLETE

### Solution (IMPLEMENTED)
Created new `coreengine/grpc/interceptors.go`:

```go
// NEW FILE: coreengine/grpc/interceptors.go

// LoggingInterceptor - structured request logging
func LoggingInterceptor(logger Logger) grpc.UnaryServerInterceptor {
    return func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
        start := time.Now()
        logger.Debug("grpc_request_start", "method", info.FullMethod)
        resp, err := handler(ctx, req)
        // ... logging
        return resp, err
    }
}

// RecoveryInterceptor - panic recovery with stack traces
func RecoveryInterceptor(logger Logger, handler RecoveryHandler) grpc.UnaryServerInterceptor {
    return func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, next grpc.UnaryHandler) (resp interface{}, err error) {
        defer func() {
            if r := recover(); r != nil {
                logger.Error("grpc_panic_recovered", "panic", r, "stack", string(debug.Stack()))
                err = handler(r)
            }
        }()
        return next(ctx, req)
    }
}

// ChainUnaryInterceptors - compose multiple interceptors
func ChainUnaryInterceptors(interceptors ...grpc.UnaryServerInterceptor) grpc.UnaryServerInterceptor

// ServerOptions - production-ready server options
func ServerOptions(logger Logger) []grpc.ServerOption
```

### Tests Added
- `TestLoggingInterceptor` - Verifies logging
- `TestRecoveryInterceptor` - Panic recovery
- `TestRecoveryInterceptorWithPanic` - Panic handling
- `TestRecoveryInterceptorCustomHandler` - Custom handler
- `TestChainUnaryInterceptors` - Chaining order
- `TestServerOptions` - Option creation
- And more...

---

## Fix 7: Add Graceful Shutdown (P1) - COMPLETE

### Solution (IMPLEMENTED)
Created `GracefulServer` wrapper in `coreengine/grpc/server.go`:

```go
// AFTER - FIXED in coreengine/grpc/server.go

type GracefulServer struct {
    grpcServer *grpc.Server
    coreServer *JeevesCoreServer
    address    string
    listener   net.Listener
    shutdownMu sync.Mutex
    isShutdown bool
}

func NewGracefulServer(server *JeevesCoreServer, address string) (*GracefulServer, error) {
    opts := ServerOptions(server.logger)
    grpcServer := grpc.NewServer(opts...)
    pb.RegisterJeevesCoreServiceServer(grpcServer, server)
    // ...
}

// Start - blocks until context cancelled, then graceful shutdown
func (s *GracefulServer) Start(ctx context.Context) error

// StartBackground - returns error channel
func (s *GracefulServer) StartBackground() (chan error, error)

// GracefulStop - idempotent graceful shutdown
func (s *GracefulServer) GracefulStop()

// ShutdownWithTimeout - graceful with timeout fallback
func (s *GracefulServer) ShutdownWithTimeout(timeout time.Duration) error
```

### Tests Added
- `TestNewGracefulServer` - Creation
- `TestGracefulServerGracefulStopIdempotent` - Double-stop safety
- `TestGracefulServerStartBackground` - Background start
- `TestGracefulServerShutdownWithTimeout` - Timeout handling

---

## Fix 8: Structured Logging (P1) - COMPLETE

### Solution (IMPLEMENTED)
Added `BusLogger` interface and replaced all `log.Printf` calls:

```go
// AFTER - FIXED in commbus/bus.go

type BusLogger interface {
    Debug(msg string, keysAndValues ...any)
    Info(msg string, keysAndValues ...any)
    Warn(msg string, keysAndValues ...any)
    Error(msg string, keysAndValues ...any)
}

func NewInMemoryCommBusWithLogger(queryTimeout time.Duration, logger BusLogger) *InMemoryCommBus

func NoopBusLogger() BusLogger

// Also updated middleware.go with logger injection:
func NewLoggingMiddlewareWithLogger(logger BusLogger) *LoggingMiddleware
func NewCircuitBreakerMiddlewareWithLogger(threshold int, timeout time.Duration, logger BusLogger, excludedTypes ...string) *CircuitBreakerMiddleware
```

### Changes Made
- Replaced 10 `log.Printf` calls in `bus.go`
- Replaced 8 `log.Printf` calls in `middleware.go`
- Added structured logging with key-value pairs

---

## RCA Bug Fixes (Bonus) - COMPLETE

### RCA Issue 1: TestCLI_ResultWithOutputs Failure

**Root Cause:** `ToResultDict()` in `generic.go` did not include the `outputs` field.

**Fix:** Added outputs to result dictionary:
```go
// coreengine/envelope/generic.go
func (e *GenericEnvelope) ToResultDict() map[string]any {
    result := map[string]any{
        // ... existing fields
    }
    if e.Outputs != nil && len(e.Outputs) > 0 {
        result["outputs"] = e.Outputs
    }
    return result
}
```

### RCA Issue 2: Parallel Execution Race Condition

**Root Cause:** `Clone()` (read) and `SetOutput()` (write) raced on `env.Outputs` map.

**Fix:** Clone envelope while holding mutex:
```go
// coreengine/runtime/runtime.go
mu.Lock()
stageEnv := env.Clone()  // Clone under lock
mu.Unlock()

go func(name string, a *agents.UnifiedAgent, clonedEnv *envelope.GenericEnvelope) {
    // Use clonedEnv instead of env
}(stageName, agent, stageEnv)
```

---

## Verification

All fixes verified:

```bash
# All tests passing
$ go test ./... -count=1
ok  cmd/envelope
ok  commbus
ok  coreengine/agents
ok  coreengine/config
ok  coreengine/envelope
ok  coreengine/grpc
ok  coreengine/runtime
ok  coreengine/testutil
ok  coreengine/tools
ok  coreengine/typeutil

# No race conditions
$ go test ./... -race -count=2
# All pass

# Coverage maintained
$ go test ./... -cover
# 86.5% weighted average
```

---

## Checklist - ALL COMPLETE

- [x] Fix 1: CommBus Unsubscribe - ID-based tracking
- [x] Fix 2: gRPC Thread Safety - RWMutex protection
- [x] Fix 3: Context Cancellation - select with ctx.Done()
- [x] Fix 4: Type Safety - typeutil package
- [x] Fix 5: DI for Config - ConfigProvider interface
- [x] Fix 6: gRPC Interceptors - logging, recovery
- [x] Fix 7: Graceful Shutdown - GracefulServer wrapper
- [x] Fix 8: Structured Logging - BusLogger interface
- [x] RCA 1: ToResultDict outputs - Added outputs field
- [x] RCA 2: Parallel race - Clone under mutex

---

*Document updated: January 23, 2026*  
*All fixes: IMPLEMENTED*  
*All tests: PASSING*  
*Race conditions: NONE*
