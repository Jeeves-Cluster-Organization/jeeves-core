# Critical Go Fixes - Implementation Guide

This document provides specific, actionable code changes for the most critical issues identified in the codebase analysis.

---

## Fix 1: CommBus Unsubscribe Function Bug (P0)

### Problem
The unsubscribe function in `bus.go` compares function pointer addresses incorrectly:

```go
// bus.go:229-237 - BROKEN
for i, h := range subs {
    if &h == &handler {  // This NEVER works - h is a new variable each iteration
        b.subscribers[eventType] = append(subs[:i], subs[i+1:]...)
        break
    }
}
```

### Solution
Use a handler ID-based approach:

```go
// In bus.go - replace Subscribe method

type subscriberEntry struct {
    id      string
    handler HandlerFunc
}

type InMemoryCommBus struct {
    handlers     map[string]HandlerFunc
    subscribers  map[string][]subscriberEntry  // Changed type
    middleware   []Middleware
    queryTimeout time.Duration
    mu           sync.RWMutex
    nextSubID    uint64  // Add counter for unique IDs
}

func (b *InMemoryCommBus) Subscribe(eventType string, handler HandlerFunc) func() {
    b.mu.Lock()
    
    // Generate unique ID
    b.nextSubID++
    subID := fmt.Sprintf("sub_%d", b.nextSubID)
    
    if _, exists := b.subscribers[eventType]; !exists {
        b.subscribers[eventType] = make([]subscriberEntry, 0)
    }
    b.subscribers[eventType] = append(b.subscribers[eventType], subscriberEntry{
        id:      subID,
        handler: handler,
    })
    b.mu.Unlock()

    log.Printf("Subscribed to %s (id: %s)", eventType, subID)

    // Return unsubscribe function that captures the ID
    return func() {
        b.mu.Lock()
        defer b.mu.Unlock()

        subs := b.subscribers[eventType]
        for i, entry := range subs {
            if entry.id == subID {
                b.subscribers[eventType] = append(subs[:i], subs[i+1:]...)
                log.Printf("Unsubscribed from %s (id: %s)", eventType, subID)
                return
            }
        }
    }
}
```

---

## Fix 2: Thread-Safety for gRPC Server Runtime (P0)

### Problem
`ExecutePipeline` mutates shared `s.runtime` field without synchronization:

```go
// server.go:164-176 - NOT THREAD-SAFE
s.runtime = rt  // Race condition if multiple requests arrive
```

### Solution
Either use mutex protection or create runtime per request:

```go
// Option A: Per-request runtime (RECOMMENDED)
func (s *JeevesCoreServer) ExecutePipeline(
    req *pb.ExecuteRequest,
    stream pb.JeevesCoreService_ExecutePipelineServer,
) error {
    ctx := stream.Context()
    env := protoToEnvelope(req.Envelope)

    // Create runtime for this request
    var rt *runtime.Runtime
    var err error
    
    if len(req.PipelineConfig) > 0 {
        var cfg config.PipelineConfig
        if err := json.Unmarshal(req.PipelineConfig, &cfg); err != nil {
            return fmt.Errorf("failed to parse pipeline config: %w", err)
        }
        rt, err = runtime.NewRuntime(&cfg, nil, nil, s)
        if err != nil {
            return fmt.Errorf("failed to create runtime: %w", err)
        }
    } else {
        // Use default runtime (must be set at server creation time)
        s.mu.RLock()
        rt = s.runtime
        s.mu.RUnlock()
    }

    if rt == nil {
        return fmt.Errorf("no runtime configured")
    }

    // ... rest of method using local rt
}

// Add mutex to struct
type JeevesCoreServer struct {
    pb.UnimplementedJeevesCoreServiceServer
    logger  Logger
    runtime *runtime.Runtime
    mu      sync.RWMutex  // Add this
}
```

---

## Fix 3: Context Cancellation in Execution Loops (P0)

### Problem
Sequential execution doesn't check context at loop start:

```go
// runtime.go:269 - Missing context check
for env.CurrentStage != "end" && !env.Terminated {
    // Context cancellation not checked here!
}
```

### Solution

```go
// runtime.go - Update runSequentialCore
func (r *Runtime) runSequentialCore(ctx context.Context, env *envelope.GenericEnvelope, opts RunOptions, outputChan chan StageOutput) (*envelope.GenericEnvelope, error) {
    var err error
    edgeTraversals := make(map[string]int)

    for env.CurrentStage != "end" && !env.Terminated {
        // CRITICAL: Check context at start of each iteration
        select {
        case <-ctx.Done():
            r.Logger.Info("pipeline_cancelled",
                "envelope_id", env.EnvelopeID,
                "current_stage", env.CurrentStage,
                "reason", ctx.Err().Error(),
            )
            return env, ctx.Err()
        default:
        }

        // Check if we should continue
        if cont, _ := r.shouldContinue(env); !cont {
            break
        }
        
        // ... rest of loop
    }
    
    return env, nil
}

// Also update runParallelCore
func (r *Runtime) runParallelCore(ctx context.Context, env *envelope.GenericEnvelope, opts RunOptions, outputChan chan StageOutput) (*envelope.GenericEnvelope, error) {
    completed := make(map[string]bool)
    var mu sync.Mutex

    for !env.Terminated {
        // CRITICAL: Check context at start of each iteration
        select {
        case <-ctx.Done():
            r.Logger.Info("pipeline_parallel_cancelled",
                "envelope_id", env.EnvelopeID,
                "reason", ctx.Err().Error(),
            )
            return env, ctx.Err()
        default:
        }

        if cont, _ := r.shouldContinue(env); !cont {
            break
        }
        
        // ... rest of loop
    }
    
    return env, nil
}
```

---

## Fix 4: Type Assertion Safety (P1)

### Problem
Unsafe type assertions can cause panics:

```go
// unified.go:274-275
if d, ok := result["data"]; ok {
    data = d.(map[string]any)  // PANIC if d is wrong type
}
```

### Solution

```go
// Create a safe type assertion helper
func asMap(v any) (map[string]any, bool) {
    if v == nil {
        return nil, false
    }
    m, ok := v.(map[string]any)
    return m, ok
}

func asString(v any) (string, bool) {
    if v == nil {
        return "", false
    }
    s, ok := v.(string)
    return s, ok
}

func asInt(v any) (int, bool) {
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

// Update toolProcess to use safe assertions
func (a *UnifiedAgent) toolProcess(ctx context.Context, env *envelope.GenericEnvelope) (map[string]any, error) {
    plan := env.GetOutput("plan")
    if plan == nil {
        return map[string]any{
            "results":       []any{},
            "total_time_ms": 0,
            "all_succeeded": true,
        }, nil
    }

    // Safe step extraction
    stepsRaw, _ := plan["steps"]
    steps, ok := stepsRaw.([]any)
    if !ok {
        steps = []any{}
    }
    
    results := make([]map[string]any, 0, len(steps))
    totalTimeMS := 0

    for i, stepAny := range steps {
        step, ok := asMap(stepAny)
        if !ok {
            continue
        }

        toolName, _ := asString(step["tool"])
        params, _ := asMap(step["parameters"])
        if params == nil {
            params = make(map[string]any)
        }
        
        stepID, ok := asString(step["step_id"])
        if !ok {
            stepID = fmt.Sprintf("step_%d", i)
        }

        // ... rest of execution

        // Safe data extraction from result
        if resultData, ok := asMap(result["data"]); ok {
            data = resultData
        }
    }
    
    // ... rest of method
}
```

---

## Fix 5: Replace Global Config with DI (P1)

### Problem
Global state makes testing difficult and creates hidden dependencies:

```go
// core_config.go:274-307
var globalCoreConfig *CoreConfig
```

### Solution

```go
// 1. Create a ConfigProvider interface
type ConfigProvider interface {
    GetCoreConfig() *CoreConfig
    GetPipelineConfig(name string) (*PipelineConfig, error)
}

// 2. Create implementation
type DefaultConfigProvider struct {
    coreConfig     *CoreConfig
    pipelineConfigs map[string]*PipelineConfig
    mu             sync.RWMutex
}

func NewConfigProvider(core *CoreConfig) *DefaultConfigProvider {
    if core == nil {
        core = DefaultCoreConfig()
    }
    return &DefaultConfigProvider{
        coreConfig:      core,
        pipelineConfigs: make(map[string]*PipelineConfig),
    }
}

func (p *DefaultConfigProvider) GetCoreConfig() *CoreConfig {
    p.mu.RLock()
    defer p.mu.RUnlock()
    return p.coreConfig
}

// 3. Update Runtime to accept ConfigProvider
type Runtime struct {
    Config         *PipelineConfig
    ConfigProvider ConfigProvider  // Add this
    LLMFactory     LLMProviderFactory
    // ...
}

// 4. Remove global variables (deprecate, don't delete yet)
// Keep for backward compatibility but mark as deprecated

// Deprecated: Use ConfigProvider instead
func GetCoreConfig() *CoreConfig {
    configMu.RLock()
    defer configMu.RUnlock()
    if globalCoreConfig == nil {
        return DefaultCoreConfig()
    }
    return globalCoreConfig
}
```

---

## Fix 6: Add gRPC Interceptors (P1)

### Solution

```go
// server.go - Add new file or update existing

import (
    grpc_middleware "github.com/grpc-ecosystem/go-grpc-middleware"
    grpc_recovery "github.com/grpc-ecosystem/go-grpc-middleware/recovery"
    "go.opentelemetry.io/contrib/instrumentation/google.golang.org/grpc/otelgrpc"
)

// LoggingInterceptor creates a logging interceptor
func LoggingInterceptor(logger Logger) grpc.UnaryServerInterceptor {
    return func(
        ctx context.Context,
        req interface{},
        info *grpc.UnaryServerInfo,
        handler grpc.UnaryHandler,
    ) (interface{}, error) {
        start := time.Now()
        
        logger.Debug("grpc_request_start",
            "method", info.FullMethod,
        )
        
        resp, err := handler(ctx, req)
        
        duration := time.Since(start)
        if err != nil {
            logger.Error("grpc_request_error",
                "method", info.FullMethod,
                "duration_ms", duration.Milliseconds(),
                "error", err.Error(),
            )
        } else {
            logger.Info("grpc_request_complete",
                "method", info.FullMethod,
                "duration_ms", duration.Milliseconds(),
            )
        }
        
        return resp, err
    }
}

// RecoveryHandler handles panics
func RecoveryHandler(logger Logger) grpc_recovery.RecoveryHandlerFunc {
    return func(p interface{}) error {
        logger.Error("grpc_panic_recovered",
            "panic", fmt.Sprintf("%v", p),
            "stack", string(debug.Stack()),
        )
        return status.Errorf(codes.Internal, "internal error")
    }
}

// Start starts the gRPC server with interceptors
func Start(address string, server *JeevesCoreServer) error {
    lis, err := net.Listen("tcp", address)
    if err != nil {
        return fmt.Errorf("failed to listen: %w", err)
    }

    grpcServer := grpc.NewServer(
        grpc.ChainUnaryInterceptor(
            otelgrpc.UnaryServerInterceptor(),
            LoggingInterceptor(server.logger),
            grpc_recovery.UnaryServerInterceptor(
                grpc_recovery.WithRecoveryHandler(RecoveryHandler(server.logger)),
            ),
        ),
        grpc.ChainStreamInterceptor(
            otelgrpc.StreamServerInterceptor(),
            grpc_recovery.StreamServerInterceptor(
                grpc_recovery.WithRecoveryHandler(RecoveryHandler(server.logger)),
            ),
        ),
    )
    
    pb.RegisterJeevesCoreServiceServer(grpcServer, server)

    server.logger.Info("grpc_server_started", "address", address)
    return grpcServer.Serve(lis)
}
```

---

## Fix 7: Add Graceful Shutdown (P1)

### Solution

```go
// server.go - Update lifecycle functions

// GracefulServer wraps grpc.Server with lifecycle management
type GracefulServer struct {
    grpcServer *grpc.Server
    listener   net.Listener
    logger     Logger
    done       chan struct{}
}

// NewGracefulServer creates a new GracefulServer
func NewGracefulServer(address string, server *JeevesCoreServer) (*GracefulServer, error) {
    lis, err := net.Listen("tcp", address)
    if err != nil {
        return nil, fmt.Errorf("failed to listen: %w", err)
    }

    grpcServer := grpc.NewServer(
        // ... interceptors from Fix 6
    )
    pb.RegisterJeevesCoreServiceServer(grpcServer, server)

    return &GracefulServer{
        grpcServer: grpcServer,
        listener:   lis,
        logger:     server.logger,
        done:       make(chan struct{}),
    }, nil
}

// Start starts the server and blocks until shutdown
func (s *GracefulServer) Start(ctx context.Context) error {
    // Start serving
    errCh := make(chan error, 1)
    go func() {
        s.logger.Info("grpc_server_starting", "address", s.listener.Addr().String())
        errCh <- s.grpcServer.Serve(s.listener)
    }()

    // Wait for shutdown signal or error
    select {
    case <-ctx.Done():
        s.logger.Info("grpc_server_shutting_down", "reason", "context cancelled")
        return s.Shutdown(5 * time.Second)
    case err := <-errCh:
        return err
    }
}

// Shutdown performs graceful shutdown
func (s *GracefulServer) Shutdown(timeout time.Duration) error {
    // Create a deadline for graceful shutdown
    done := make(chan struct{})
    go func() {
        s.grpcServer.GracefulStop()
        close(done)
    }()

    select {
    case <-done:
        s.logger.Info("grpc_server_stopped_gracefully")
        return nil
    case <-time.After(timeout):
        s.logger.Warn("grpc_server_force_stopping", "timeout", timeout)
        s.grpcServer.Stop()
        return fmt.Errorf("graceful shutdown timed out after %v", timeout)
    }
}

// Usage example in main.go:
func main() {
    ctx, cancel := signal.NotifyContext(context.Background(), 
        syscall.SIGINT, syscall.SIGTERM)
    defer cancel()

    server, err := NewGracefulServer(":50051", jeevesCoreServer)
    if err != nil {
        log.Fatal(err)
    }

    if err := server.Start(ctx); err != nil {
        log.Fatal(err)
    }
}
```

---

## Testing the Fixes

### Test for Fix 1 (Unsubscribe)

```go
func TestUnsubscribeActuallyWorks(t *testing.T) {
    bus := NewInMemoryCommBus(time.Second)
    callCount := 0
    
    unsubscribe := bus.Subscribe("TestEvent", func(ctx context.Context, msg Message) (any, error) {
        callCount++
        return nil, nil
    })
    
    // First publish should trigger handler
    bus.Publish(context.Background(), &TestEvent{})
    assert.Equal(t, 1, callCount)
    
    // Unsubscribe
    unsubscribe()
    
    // Second publish should NOT trigger handler
    bus.Publish(context.Background(), &TestEvent{})
    assert.Equal(t, 1, callCount)  // Still 1, not 2
}
```

### Test for Fix 3 (Context Cancellation)

```go
func TestRunSequentialRespectsContextCancellation(t *testing.T) {
    runtime := createTestRuntimeWithSlowAgent()
    
    ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
    defer cancel()
    
    env := envelope.NewTestEnvelope("test")
    
    _, _, err := runtime.Execute(ctx, env, RunOptions{})
    
    assert.Error(t, err)
    assert.ErrorIs(t, err, context.DeadlineExceeded)
}
```

---

## Checklist

- [ ] Fix 1: CommBus Unsubscribe - Replace function pointer comparison with ID-based tracking
- [ ] Fix 2: gRPC Thread Safety - Add mutex or per-request runtime
- [ ] Fix 3: Context Cancellation - Add `select` at start of execution loops
- [ ] Fix 4: Type Safety - Replace direct type assertions with helper functions
- [ ] Fix 5: DI for Config - Create ConfigProvider interface
- [ ] Fix 6: gRPC Interceptors - Add logging, recovery, and tracing interceptors
- [ ] Fix 7: Graceful Shutdown - Implement GracefulServer wrapper

---

*Document generated: January 23, 2026*
