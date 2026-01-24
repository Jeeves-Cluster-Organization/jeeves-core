# gRPC - gRPC Server Implementation

> **File Location**: `jeeves-core/coreengine/grpc/server.go`

The gRPC package provides the primary IPC mechanism between Python and Go, enabling envelope operations, bounds checking, and pipeline execution.

## Table of Contents

- [Overview](#overview)
- [EngineServer](#jeevesCoreServer)
- [Available RPCs](#available-rpcs)
- [Envelope Operations](#envelope-operations)
- [Pipeline Execution](#pipeline-execution)
- [Server Lifecycle](#server-lifecycle)
- [Usage Examples](#usage-examples)
- [Related Documentation](#related-documentation)

---

## Overview

The gRPC server enables:

- **Envelope Creation/Update**: Create and modify Envelopes
- **Bounds Checking**: Verify execution can continue
- **Pipeline Execution**: Run pipelines with streaming events
- **Agent Execution**: Execute individual agents
- **Graceful Shutdown**: Proper shutdown with timeout support

### Thread Safety

The server is thread-safe:
- `runtime` field protected by `RWMutex`
- All methods can be called concurrently

---

## EngineServer

### Structure

```go
type EngineServer struct {
    pb.UnimplementedEngineServiceServer
    
    logger    Logger
    runtime   *runtime.Runtime
    runtimeMu sync.RWMutex
}
```

### Constructor

```go
func NewEngineServer(logger Logger) *EngineServer
```

Creates a new gRPC server with the given logger.

### Logger Interface

```go
type Logger interface {
    Debug(msg string, keysAndValues ...any)
    Info(msg string, keysAndValues ...any)
    Warn(msg string, keysAndValues ...any)
    Error(msg string, keysAndValues ...any)
}
```

**Note**: `EngineServer` also implements `agents.Logger` (including `Bind` method) for use with the runtime.

### SetRuntime

```go
func (s *EngineServer) SetRuntime(r *runtime.Runtime)
```

Sets the runtime for pipeline execution. Thread-safe.

---

## Available RPCs

| RPC | Description | Type |
|-----|-------------|------|
| `CreateEnvelope` | Create new Envelope | Unary |
| `UpdateEnvelope` | Update envelope from proto | Unary |
| `CloneEnvelope` | Deep copy envelope | Unary |
| `CheckBounds` | Check if execution can continue | Unary |
| `ExecutePipeline` | Execute pipeline with streaming | Server Streaming |
| `ExecuteAgent` | Execute single agent | Unary |

---

## Envelope Operations

### CreateEnvelope

Creates a new Envelope from request:

```go
func (s *EngineServer) CreateEnvelope(
    ctx context.Context,
    req *pb.CreateEnvelopeRequest,
) (*pb.Envelope, error)
```

**Request Fields**:
- `RawInput`: User input text
- `UserId`: User identifier
- `SessionId`: Session identifier
- `RequestId`: Optional request ID
- `Metadata`: Key-value metadata
- `StageOrder`: Pipeline stage order

### UpdateEnvelope

Updates envelope from proto representation:

```go
func (s *EngineServer) UpdateEnvelope(
    ctx context.Context,
    req *pb.UpdateEnvelopeRequest,
) (*pb.Envelope, error)
```

### CloneEnvelope

Creates a deep copy:

```go
func (s *EngineServer) CloneEnvelope(
    ctx context.Context,
    req *pb.CloneRequest,
) (*pb.Envelope, error)
```

### CheckBounds

Verifies execution can continue:

```go
func (s *EngineServer) CheckBounds(
    ctx context.Context,
    protoEnv *pb.Envelope,
) (*pb.BoundsResult, error)
```

**BoundsResult Fields**:
- `CanContinue`: Whether execution can proceed
- `LlmCallsRemaining`: Remaining LLM call budget
- `AgentHopsRemaining`: Remaining agent hops
- `IterationsRemaining`: Remaining iterations
- `TerminalReason`: Why execution cannot continue (if applicable)

---

## Pipeline Execution

### ExecutePipeline

Executes pipeline with streaming execution events:

```go
func (s *EngineServer) ExecutePipeline(
    req *pb.ExecuteRequest,
    stream pb.EngineService_ExecutePipelineServer,
) error
```

**Request Fields**:
- `Envelope`: Initial envelope state
- `ThreadId`: Persistence thread ID
- `PipelineConfig`: JSON-encoded pipeline configuration

**Execution Events**:

| Event Type | Description |
|------------|-------------|
| `STAGE_STARTED` | Stage execution began |
| `STAGE_COMPLETED` | Stage completed successfully |
| `STAGE_FAILED` | Stage failed with error |
| `PIPELINE_COMPLETED` | Pipeline finished |
| `INTERRUPT_RAISED` | Interrupt pending |
| `BOUNDS_EXCEEDED` | Resource limits hit |

**Event Structure**:
```go
type ExecutionEvent struct {
    Type        ExecutionEventType
    Stage       string
    TimestampMs int64
    Payload     []byte  // JSON-encoded
    Envelope    *Envelope
}
```

### ExecuteAgent

Executes a single agent:

```go
func (s *EngineServer) ExecuteAgent(
    ctx context.Context,
    req *pb.ExecuteAgentRequest,
) (*pb.AgentResult, error)
```

**AgentResult Fields**:
- `Success`: Whether execution succeeded
- `Envelope`: Updated envelope
- `DurationMs`: Execution time
- `LlmCalls`: Number of LLM calls made

---

## Server Lifecycle

### Basic Start

```go
func Start(address string, server *EngineServer) error
```

Starts server and blocks until shutdown.

### Background Start

```go
func StartBackground(address string, server *EngineServer) (*grpc.Server, error)
```

Starts server in goroutine, returns server handle.

### GracefulServer

Wrapper with graceful shutdown support:

```go
type GracefulServer struct {
    // Internal fields
}
```

**Constructor**:
```go
func NewGracefulServer(
    coreServer *EngineServer,
    address string,
    opts ...grpc.ServerOption,
) (*GracefulServer, error)
```

If no `grpc.ServerOption` values are provided, default options are used via the internal `ServerOptions(logger)` function.

**Methods**:

| Method | Description |
|--------|-------------|
| `Start(ctx)` | Start and block until ctx cancelled |
| `StartBackground()` | Start in goroutine, return error channel |
| `GracefulStop()` | Graceful shutdown |
| `Stop()` | Immediate shutdown |
| `ShutdownWithTimeout(timeout)` | Graceful with timeout fallback |
| `GetGRPCServer()` | Get underlying grpc.Server |
| `Address()` | Get server address |

---

## Usage Examples

### Basic Server Setup

```go
// Create logger
logger := &MyLogger{}

// Create server
server := grpc.NewEngineServer(logger)

// Set runtime
cfg := config.NewPipelineConfig("analysis")
// ... configure pipeline ...

rt, err := runtime.NewRuntime(cfg, llmFactory, toolExecutor, logger)
if err != nil {
    log.Fatal(err)
}
server.SetRuntime(rt)

// Start server
if err := grpc.Start(":50051", server); err != nil {
    log.Fatal(err)
}
```

### Graceful Server with Context

```go
ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt)
defer cancel()

server := grpc.NewEngineServer(logger)

graceful, err := grpc.NewGracefulServer(server, ":50051")
if err != nil {
    log.Fatal(err)
}

// Blocks until SIGINT
if err := graceful.Start(ctx); err != nil && err != context.Canceled {
    log.Fatal(err)
}
```

### Background Server

```go
server := grpc.NewEngineServer(logger)

graceful, _ := grpc.NewGracefulServer(server, ":50051")
errCh, err := graceful.StartBackground()
if err != nil {
    log.Fatal(err)
}

// Do other work...

// Shutdown with timeout
graceful.ShutdownWithTimeout(30 * time.Second)

// Check for errors
if err := <-errCh; err != nil {
    log.Printf("Server error: %v", err)
}
```

### Python Client (Example)

```python
import grpc
from coreengine.proto import engine_pb2 as pb
from coreengine.proto import engine_pb2_grpc as pb_grpc

# Connect
channel = grpc.insecure_channel("localhost:50051")
stub = pb_grpc.EngineServiceStub(channel)

# Create envelope
request = pb.CreateEnvelopeRequest(
    raw_input="Analyze the auth module",
    user_id="user_123",
    session_id="sess_456",
)
envelope = stub.CreateEnvelope(request)

# Execute pipeline
execute_request = pb.ExecuteRequest(
    envelope=envelope,
    thread_id="thread_123",
)

for event in stub.ExecutePipeline(execute_request):
    print(f"Event: {event.type} at stage {event.stage}")
    if event.type == pb.ExecutionEventType.PIPELINE_COMPLETED:
        final_envelope = event.envelope
        break
```

### Streaming Pipeline Events

```go
// Client-side
stream, err := client.ExecutePipeline(ctx, &pb.ExecuteRequest{
    Envelope: envelope,
    ThreadId: "thread_123",
})
if err != nil {
    return err
}

for {
    event, err := stream.Recv()
    if err == io.EOF {
        break
    }
    if err != nil {
        return err
    }
    
    switch event.Type {
    case pb.ExecutionEventType_STAGE_STARTED:
        fmt.Printf("Started: %s\n", event.Stage)
    case pb.ExecutionEventType_STAGE_COMPLETED:
        fmt.Printf("Completed: %s\n", event.Stage)
    case pb.ExecutionEventType_INTERRUPT_RAISED:
        // Handle interrupt
        fmt.Printf("Interrupt at: %s\n", event.Stage)
    case pb.ExecutionEventType_PIPELINE_COMPLETED:
        fmt.Println("Pipeline complete")
    }
}
```

---

## Related Documentation

- [Core Engine Overview](README.md)
- [Runtime Execution](runtime.md) - Pipeline execution
- [Envelope System](envelope.md) - Envelope structure
- [Pipeline Configuration](config.md) - Pipeline config format
