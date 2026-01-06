# StateMachineExecutor Design Document

**Created:** 2026-01-06
**Status:** Architectural Design (Pre-Implementation)
**Decision:** D6-D9 from FUTURE_PLAN.md

---

## Executive Summary

This document specifies the `StateMachineExecutor` - the **only** Go executor. It replaces `DAGExecutor` (deleted, not deprecated).

**Key Principles:**
1. **Go is authoritative** - Python is a thin gRPC client, nothing else
2. **Core doesn't judge graphs** - Capability defines structure, core executes it
3. **No backward compatibility** - Delete old code, don't wrap it
4. **gRPC only** - Subprocess client deleted, not deprecated
5. **Fail loudly** - No fallbacks to Python implementations

---

## 1. What Gets Deleted

### Files to DELETE (not deprecate)

| File | Reason |
|------|--------|
| `coreengine/runtime/dag_executor.go` | Replaced by StateMachineExecutor |
| `jeeves_protocols/client.py` | Replaced by gRPC client |
| Sequential execution in `runtime.go` | Merged into StateMachineExecutor |

### Code Patterns to DELETE

| Pattern | Replacement |
|---------|-------------|
| `CyclePolicyReject` | Core doesn't validate graph structure |
| `EnableDAGExecution` flag | Always state machine |
| `EnableArbiter` flag | Arbiter is capability concern |
| Subprocess calls | gRPC only |
| `if go_client else python_fallback` | Go only. Fail if unavailable. |
| `topologicalOrder` | Not needed - cycles are expected |

---

## 2. StateMachineExecutor Design

### 2.1 Core Structure

```go
// coreengine/runtime/state_machine_executor.go
package runtime

import (
    "context"
    "sync"
    "time"
    
    "github.com/jeeves-cluster-organization/codeanalysis/coreengine/agents"
    "github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
    "github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// TransitionReason explains why a transition occurred.
type TransitionReason string

const (
    TransitionDefault     TransitionReason = "default"      // DefaultNext
    TransitionRouting     TransitionReason = "routing"      // RoutingRule matched
    TransitionReintent    TransitionReason = "reintent"     // Critic requested reintent
    TransitionNextStage   TransitionReason = "next_stage"   // Multi-stage advance
    TransitionError       TransitionReason = "error"        // ErrorNext routing
    TransitionComplete    TransitionReason = "complete"     // Pipeline complete
)

// Transition records a state transition for audit/debugging.
type Transition struct {
    From       string           `json:"from"`
    To         string           `json:"to"`
    Reason     TransitionReason `json:"reason"`
    Iteration  int              `json:"iteration"`
    Timestamp  time.Time        `json:"timestamp"`
    Checkpoint string           `json:"checkpoint,omitempty"` // Checkpoint ID if created
}

// EdgeLimit configures per-edge cycle limits.
// Capability layer defines these for their workflows.
type EdgeLimit struct {
    From     string `json:"from"`
    To       string `json:"to"`
    MaxCount int    `json:"max_count"` // 0 = use global MaxIterations
}

// Checkpoint stores envelope state for rollback.
type Checkpoint struct {
    ID        string                       `json:"id"`
    Stage     string                       `json:"stage"`
    Envelope  *envelope.GenericEnvelope    `json:"envelope"`
    CreatedAt time.Time                    `json:"created_at"`
}

// StateMachineExecutor unifies sequential routing with parallel execution.
type StateMachineExecutor struct {
    Config      *config.PipelineConfig
    Agents      map[string]*agents.UnifiedAgent
    Logger      agents.Logger
    Persistence PersistenceAdapter

    // Cycle management
    globalMaxIterations int                    // From config.MaxIterations
    edgeLimits          map[string]*EdgeLimit  // Key: "from->to"
    edgeCounts          map[string]int         // Current counts per edge
    
    // State management
    checkpoints   map[string]*Checkpoint  // Stage -> latest checkpoint
    transitions   []Transition            // Audit trail
    
    // Parallel execution (from DAGExecutor)
    completedChan chan StageResult
    errorChan     chan StageResult
    mu            sync.RWMutex
    wg            sync.WaitGroup
    activeCount   int
    maxParallel   int
}
```

### 2.2 Unified Execution Flow

```go
// Execute runs the pipeline with cycle support.
func (e *StateMachineExecutor) Execute(
    ctx context.Context,
    env *envelope.GenericEnvelope,
    threadID string,
) (*envelope.GenericEnvelope, error) {
    
    // Initialize
    e.initializeExecution(env)
    
    for !e.isTerminal(env) {
        // 1. Check bounds (authoritative - Go enforces)
        if !env.CanContinue() {
            e.Logger.Warn("bounds_exceeded", "reason", env.TerminalReason_)
            break
        }
        
        // 2. Check for pending interrupt
        if env.InterruptPending {
            e.Logger.Info("interrupt_pending", "kind", env.GetInterruptKind())
            break
        }
        
        // 3. Determine executable stages
        stages := e.getExecutableStages(env)
        if len(stages) == 0 {
            // Deadlock or completion
            if e.isComplete(env) {
                env.CurrentStage = "end"
                break
            }
            e.Logger.Error("execution_deadlock", "active", env.ActiveStages)
            break
        }
        
        // 4. Execute stages (parallel if independent, sequential if dependent)
        if e.canParallelize(stages) {
            e.executeParallel(ctx, env, stages, threadID)
        } else {
            e.executeSequential(ctx, env, stages[0], threadID)
        }
        
        // 5. Handle routing (cycles are expected here)
        e.handleRouting(env)
        
        // 6. Persist state
        e.persistState(ctx, env, threadID)
    }
    
    return env, nil
}
```

### 2.3 Cycle-Aware Routing

```go
// handleRouting processes agent output routing rules.
func (e *StateMachineExecutor) handleRouting(env *envelope.GenericEnvelope) {
    currentStage := env.CurrentStage
    agent := e.Agents[currentStage]
    if agent == nil {
        return
    }
    
    output := env.GetOutput(currentStage)
    nextStage := e.evaluateRouting(agent.Config, output)
    
    // Check if this creates a cycle
    edgeKey := fmt.Sprintf("%s->%s", currentStage, nextStage)
    
    // Check per-edge limit (capability-defined)
    if limit, exists := e.edgeLimits[edgeKey]; exists {
        limit.current++
        if limit.current > limit.MaxCount {
            e.Logger.Warn("edge_limit_exceeded",
                "edge", edgeKey,
                "limit", limit.MaxCount,
            )
            // Route to completion instead of cycle
            nextStage = "end"
        }
    }
    
    // Check global iteration limit
    if e.isCycleEdge(currentStage, nextStage) {
        env.Iteration++
        if env.Iteration > e.globalMaxIterations {
            e.Logger.Warn("global_iteration_limit",
                "iteration", env.Iteration,
                "max", e.globalMaxIterations,
            )
            nextStage = "end"
        }
        
        // Create checkpoint before cycle
        e.checkpoint(currentStage, env)
    }
    
    // Record transition
    e.transitions = append(e.transitions, Transition{
        From:      currentStage,
        To:        nextStage,
        Reason:    e.classifyTransition(currentStage, nextStage, output),
        Iteration: env.Iteration,
        Timestamp: time.Now().UTC(),
    })
    
    env.CurrentStage = nextStage
}

// isCycleEdge returns true if this edge creates a backward jump.
func (e *StateMachineExecutor) isCycleEdge(from, to string) bool {
    fromOrder := e.getStageOrder(from)
    toOrder := e.getStageOrder(to)
    return toOrder < fromOrder // Backward jump = cycle
}
```

### 2.4 State Checkpoints and Rollback

```go
// checkpoint saves envelope state before a potential cycle.
func (e *StateMachineExecutor) checkpoint(stage string, env *envelope.GenericEnvelope) {
    e.mu.Lock()
    defer e.mu.Unlock()
    
    cp := &Checkpoint{
        ID:        fmt.Sprintf("cp_%s_%d", stage, env.Iteration),
        Stage:     stage,
        Envelope:  env.Clone(), // REQUIRES Clone() method
        CreatedAt: time.Now().UTC(),
    }
    
    e.checkpoints[stage] = cp
    
    e.Logger.Debug("checkpoint_created",
        "id", cp.ID,
        "stage", stage,
        "iteration", env.Iteration,
    )
}

// rollbackTo restores envelope to a previous checkpoint.
func (e *StateMachineExecutor) rollbackTo(stage string) (*envelope.GenericEnvelope, error) {
    e.mu.RLock()
    defer e.mu.RUnlock()
    
    cp, exists := e.checkpoints[stage]
    if !exists {
        return nil, fmt.Errorf("no checkpoint for stage: %s", stage)
    }
    
    e.Logger.Info("rollback_to_checkpoint",
        "id", cp.ID,
        "stage", stage,
    )
    
    return cp.Envelope.Clone(), nil
}
```

---

## 3. Envelope Clone() Implementation

Required for checkpoints. Add to `coreengine/envelope/generic.go`:

```go
// Clone creates a deep copy of the envelope.
func (e *GenericEnvelope) Clone() *GenericEnvelope {
    clone := &GenericEnvelope{
        // Identification (copy by value)
        EnvelopeID: e.EnvelopeID,
        RequestID:  e.RequestID,
        UserID:     e.UserID,
        SessionID:  e.SessionID,
        
        // Input
        RawInput:   e.RawInput,
        ReceivedAt: e.ReceivedAt,
        
        // Pipeline state
        CurrentStage:  e.CurrentStage,
        Iteration:     e.Iteration,
        MaxIterations: e.MaxIterations,
        
        // Bounds
        LLMCallCount:  e.LLMCallCount,
        MaxLLMCalls:   e.MaxLLMCalls,
        AgentHopCount: e.AgentHopCount,
        MaxAgentHops:  e.MaxAgentHops,
        
        // Control flow
        Terminated:       e.Terminated,
        InterruptPending: e.InterruptPending,
        
        // Multi-stage
        CurrentStageNumber: e.CurrentStageNumber,
        MaxStages:          e.MaxStages,
        
        // Timing
        CreatedAt: e.CreatedAt,
    }
    
    // Deep copy maps
    clone.Outputs = deepCopyOutputs(e.Outputs)
    clone.ActiveStages = copyStringBoolMap(e.ActiveStages)
    clone.CompletedStageSet = copyStringBoolMap(e.CompletedStageSet)
    clone.FailedStages = copyStringStringMap(e.FailedStages)
    clone.GoalCompletionStatus = copyStringStringMap(e.GoalCompletionStatus)
    clone.Metadata = deepCopyAnyMap(e.Metadata)
    
    // Deep copy slices
    clone.StageOrder = copyStringSlice(e.StageOrder)
    clone.AllGoals = copyStringSlice(e.AllGoals)
    clone.RemainingGoals = copyStringSlice(e.RemainingGoals)
    clone.CriticFeedback = copyStringSlice(e.CriticFeedback)
    clone.CompletedStages = deepCopyMapSlice(e.CompletedStages)
    clone.PriorPlans = deepCopyMapSlice(e.PriorPlans)
    clone.ProcessingHistory = copyProcessingHistory(e.ProcessingHistory)
    clone.Errors = deepCopyMapSlice(e.Errors)
    
    // Deep copy pointers
    if e.TerminalReason_ != nil {
        reason := *e.TerminalReason_
        clone.TerminalReason_ = &reason
    }
    if e.TerminationReason != nil {
        reason := *e.TerminationReason
        clone.TerminationReason = &reason
    }
    if e.Interrupt != nil {
        clone.Interrupt = e.Interrupt.Clone()
    }
    if e.CompletedAt != nil {
        t := *e.CompletedAt
        clone.CompletedAt = &t
    }
    
    return clone
}

// Helper functions
func copyStringBoolMap(m map[string]bool) map[string]bool {
    if m == nil {
        return nil
    }
    result := make(map[string]bool, len(m))
    for k, v := range m {
        result[k] = v
    }
    return result
}

func copyStringStringMap(m map[string]string) map[string]string {
    if m == nil {
        return nil
    }
    result := make(map[string]string, len(m))
    for k, v := range m {
        result[k] = v
    }
    return result
}

func copyStringSlice(s []string) []string {
    if s == nil {
        return nil
    }
    result := make([]string, len(s))
    copy(result, s)
    return result
}

func deepCopyOutputs(m map[string]map[string]any) map[string]map[string]any {
    if m == nil {
        return nil
    }
    result := make(map[string]map[string]any, len(m))
    for k, v := range m {
        result[k] = deepCopyAnyMap(v)
    }
    return result
}

func deepCopyAnyMap(m map[string]any) map[string]any {
    if m == nil {
        return nil
    }
    // Use JSON round-trip for deep copy (handles nested structures)
    data, _ := json.Marshal(m)
    var result map[string]any
    json.Unmarshal(data, &result)
    return result
}

func deepCopyMapSlice(s []map[string]any) []map[string]any {
    if s == nil {
        return nil
    }
    result := make([]map[string]any, len(s))
    for i, m := range s {
        result[i] = deepCopyAnyMap(m)
    }
    return result
}

func copyProcessingHistory(h []ProcessingRecord) []ProcessingRecord {
    if h == nil {
        return nil
    }
    result := make([]ProcessingRecord, len(h))
    copy(result, h)
    return result
}
```

---

## 4. gRPC Migration

### 4.1 Why gRPC > Subprocess

| Aspect | Subprocess | gRPC |
|--------|------------|------|
| Latency per call | 10-100ms | 0.1-1ms |
| Connection | New process each call | Persistent |
| Serialization | JSON text | Protobuf binary |
| Streaming | ❌ No | ✅ Native |
| Error handling | Exit codes + stderr | Typed errors |
| Bidirectional | ❌ No | ✅ Yes |

### 4.2 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Python Process                          │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐    │
│  │ FastAPI      │   │ Orchestrator │   │ LLM/Tools    │    │
│  │ Gateway      │   │              │   │ (stays Py)   │    │
│  └──────┬───────┘   └──────┬───────┘   └──────────────┘    │
│         │                  │                                 │
│         └──────────────────┼─────────────────────────────────┤
│                            │ gRPC Client                     │
│                            ▼                                 │
└────────────────────────────┬────────────────────────────────┘
                             │ TCP :50051
┌────────────────────────────┴────────────────────────────────┐
│                      Go Process (Long-running)               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │                   gRPC Server                         │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐   │   │
│  │  │ Envelope    │  │ StateMachine│  │ Bounds      │   │   │
│  │  │ Service     │  │ Executor    │  │ Checker     │   │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘   │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 4.3 Proto Definition

```protobuf
// coreengine/proto/jeeves_core.proto
syntax = "proto3";

package jeeves.core.v1;

option go_package = "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto";

// JeevesCoreService - Go runtime operations
service JeevesCoreService {
    // Envelope operations
    rpc CreateEnvelope(CreateEnvelopeRequest) returns (Envelope);
    rpc CheckBounds(Envelope) returns (BoundsResult);
    rpc UpdateEnvelope(UpdateEnvelopeRequest) returns (Envelope);
    
    // Execution
    rpc ExecutePipeline(ExecuteRequest) returns (stream ExecutionEvent);
    rpc ExecuteAgent(ExecuteAgentRequest) returns (AgentResult);
    
    // State management
    rpc CreateCheckpoint(CheckpointRequest) returns (Checkpoint);
    rpc RollbackToCheckpoint(RollbackRequest) returns (Envelope);
}

message CreateEnvelopeRequest {
    string raw_input = 1;
    string user_id = 2;
    string session_id = 3;
    optional string request_id = 4;
    map<string, string> metadata = 5;
    repeated string stage_order = 6;
}

message Envelope {
    string envelope_id = 1;
    string request_id = 2;
    string user_id = 3;
    string session_id = 4;
    string raw_input = 5;
    string current_stage = 6;
    int32 iteration = 7;
    int32 llm_call_count = 8;
    int32 agent_hop_count = 9;
    bool terminated = 10;
    optional string termination_reason = 11;
    optional string terminal_reason = 12;
    bool interrupt_pending = 13;
    map<string, bytes> outputs = 14;  // JSON-encoded per-agent outputs
    // ... other fields
}

message BoundsResult {
    bool can_continue = 1;
    optional string terminal_reason = 2;
    int32 llm_calls_remaining = 3;
    int32 agent_hops_remaining = 4;
    int32 iterations_remaining = 5;
}

message ExecuteRequest {
    Envelope envelope = 1;
    string thread_id = 2;
    PipelineConfig config = 3;
}

message ExecutionEvent {
    enum EventType {
        STAGE_STARTED = 0;
        STAGE_COMPLETED = 1;
        STAGE_FAILED = 2;
        PIPELINE_COMPLETED = 3;
        INTERRUPT_RAISED = 4;
        CHECKPOINT_CREATED = 5;
    }
    EventType type = 1;
    string stage = 2;
    int64 timestamp_ms = 3;
    bytes payload = 4;  // JSON-encoded event data
}

message AgentResult {
    bool success = 1;
    bytes output = 2;  // JSON-encoded output
    optional string error = 3;
    int32 duration_ms = 4;
    int32 llm_calls = 5;
}
```

### 4.4 Python Client

```python
# jeeves_protocols/grpc_client.py
"""gRPC client for Go runtime.

Replaces subprocess-based GoClient with persistent gRPC connection.
"""

import grpc
from typing import Optional, Dict, Any, Iterator
from dataclasses import dataclass

from jeeves_protocols.envelope import GenericEnvelope
from jeeves_protocols.proto import jeeves_core_pb2, jeeves_core_pb2_grpc


class GrpcGoClient:
    """gRPC client for Go runtime operations.
    
    Usage:
        client = GrpcGoClient()
        
        # Create envelope (via Go)
        envelope = client.create_envelope("Hello", "user1", "session1")
        
        # Check bounds (authoritative - Go decides)
        result = client.check_bounds(envelope)
        if not result.can_continue:
            raise BoundsExceededError(result.terminal_reason)
        
        # Execute pipeline with streaming
        for event in client.execute_pipeline(envelope, thread_id):
            handle_event(event)
    """
    
    DEFAULT_ADDRESS = "localhost:50051"
    
    def __init__(self, address: Optional[str] = None):
        self._address = address or self.DEFAULT_ADDRESS
        self._channel: Optional[grpc.Channel] = None
        self._stub: Optional[jeeves_core_pb2_grpc.JeevesCoreServiceStub] = None
    
    def _ensure_connected(self) -> None:
        """Ensure gRPC channel is connected."""
        if self._channel is None:
            self._channel = grpc.insecure_channel(self._address)
            self._stub = jeeves_core_pb2_grpc.JeevesCoreServiceStub(self._channel)
    
    def create_envelope(
        self,
        raw_input: str,
        user_id: str,
        session_id: str,
        request_id: Optional[str] = None,
        metadata: Optional[Dict[str, str]] = None,
        stage_order: Optional[list] = None,
    ) -> GenericEnvelope:
        """Create envelope via Go runtime."""
        self._ensure_connected()
        
        request = jeeves_core_pb2.CreateEnvelopeRequest(
            raw_input=raw_input,
            user_id=user_id,
            session_id=session_id,
            request_id=request_id or "",
            metadata=metadata or {},
            stage_order=stage_order or [],
        )
        
        response = self._stub.CreateEnvelope(request)
        return self._proto_to_envelope(response)
    
    def check_bounds(self, envelope: GenericEnvelope) -> "BoundsResult":
        """Check bounds - Go is authoritative."""
        self._ensure_connected()
        
        proto_env = self._envelope_to_proto(envelope)
        result = self._stub.CheckBounds(proto_env)
        
        return BoundsResult(
            can_continue=result.can_continue,
            terminal_reason=result.terminal_reason if result.HasField("terminal_reason") else None,
            llm_calls_remaining=result.llm_calls_remaining,
            agent_hops_remaining=result.agent_hops_remaining,
            iterations_remaining=result.iterations_remaining,
        )
    
    def execute_pipeline(
        self,
        envelope: GenericEnvelope,
        thread_id: str,
    ) -> Iterator["ExecutionEvent"]:
        """Execute pipeline with streaming events."""
        self._ensure_connected()
        
        request = jeeves_core_pb2.ExecuteRequest(
            envelope=self._envelope_to_proto(envelope),
            thread_id=thread_id,
        )
        
        for event in self._stub.ExecutePipeline(request):
            yield ExecutionEvent(
                type=event.type,
                stage=event.stage,
                timestamp_ms=event.timestamp_ms,
                payload=event.payload,
            )
    
    def close(self) -> None:
        """Close the gRPC channel."""
        if self._channel:
            self._channel.close()
            self._channel = None
            self._stub = None
    
    def __enter__(self):
        return self
    
    def __exit__(self, *args):
        self.close()
    
    # Conversion helpers
    def _envelope_to_proto(self, env: GenericEnvelope) -> jeeves_core_pb2.Envelope:
        """Convert Python envelope to protobuf."""
        # Implementation details...
        pass
    
    def _proto_to_envelope(self, proto: jeeves_core_pb2.Envelope) -> GenericEnvelope:
        """Convert protobuf to Python envelope."""
        # Implementation details...
        pass


@dataclass
class BoundsResult:
    can_continue: bool
    terminal_reason: Optional[str]
    llm_calls_remaining: int
    agent_hops_remaining: int
    iterations_remaining: int


@dataclass
class ExecutionEvent:
    type: int
    stage: str
    timestamp_ms: int
    payload: bytes
```

### 4.5 Go gRPC Server

```go
// coreengine/grpc/server.go
package grpc

import (
    "context"
    "net"
    
    "google.golang.org/grpc"
    
    pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
    "github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
    "github.com/jeeves-cluster-organization/codeanalysis/coreengine/runtime"
)

type JeevesCoreServer struct {
    pb.UnimplementedJeevesCoreServiceServer
    
    executor *runtime.StateMachineExecutor
    logger   Logger
}

func NewJeevesCoreServer(executor *runtime.StateMachineExecutor, logger Logger) *JeevesCoreServer {
    return &JeevesCoreServer{
        executor: executor,
        logger:   logger,
    }
}

func (s *JeevesCoreServer) CreateEnvelope(
    ctx context.Context,
    req *pb.CreateEnvelopeRequest,
) (*pb.Envelope, error) {
    env := envelope.CreateGenericEnvelope(
        req.RawInput,
        req.UserId,
        req.SessionId,
        req.RequestId,
        convertMetadata(req.Metadata),
        req.StageOrder,
    )
    
    return envelopeToProto(env), nil
}

func (s *JeevesCoreServer) CheckBounds(
    ctx context.Context,
    env *pb.Envelope,
) (*pb.BoundsResult, error) {
    goEnv := protoToEnvelope(env)
    canContinue := goEnv.CanContinue()
    
    result := &pb.BoundsResult{
        CanContinue:         canContinue,
        LlmCallsRemaining:   int32(goEnv.MaxLLMCalls - goEnv.LLMCallCount),
        AgentHopsRemaining:  int32(goEnv.MaxAgentHops - goEnv.AgentHopCount),
        IterationsRemaining: int32(goEnv.MaxIterations - goEnv.Iteration),
    }
    
    if !canContinue && goEnv.TerminalReason_ != nil {
        reason := string(*goEnv.TerminalReason_)
        result.TerminalReason = &reason
    }
    
    return result, nil
}

func (s *JeevesCoreServer) ExecutePipeline(
    req *pb.ExecuteRequest,
    stream pb.JeevesCoreService_ExecutePipelineServer,
) error {
    ctx := stream.Context()
    env := protoToEnvelope(req.Envelope)
    
    // Execute with event streaming
    eventChan := make(chan *pb.ExecutionEvent, 100)
    
    go func() {
        defer close(eventChan)
        
        // Hook into executor events
        s.executor.OnStageStarted = func(stage string) {
            eventChan <- &pb.ExecutionEvent{
                Type:        pb.ExecutionEvent_STAGE_STARTED,
                Stage:       stage,
                TimestampMs: time.Now().UnixMilli(),
            }
        }
        
        s.executor.OnStageCompleted = func(stage string, output map[string]any) {
            payload, _ := json.Marshal(output)
            eventChan <- &pb.ExecutionEvent{
                Type:        pb.ExecutionEvent_STAGE_COMPLETED,
                Stage:       stage,
                TimestampMs: time.Now().UnixMilli(),
                Payload:     payload,
            }
        }
        
        _, err := s.executor.Execute(ctx, env, req.ThreadId)
        
        eventChan <- &pb.ExecutionEvent{
            Type:        pb.ExecutionEvent_PIPELINE_COMPLETED,
            TimestampMs: time.Now().UnixMilli(),
            Payload:     []byte(fmt.Sprintf(`{"error": %v}`, err != nil)),
        }
    }()
    
    for event := range eventChan {
        if err := stream.Send(event); err != nil {
            return err
        }
    }
    
    return nil
}

// Start starts the gRPC server.
func Start(address string, server *JeevesCoreServer) error {
    lis, err := net.Listen("tcp", address)
    if err != nil {
        return err
    }
    
    grpcServer := grpc.NewServer()
    pb.RegisterJeevesCoreServiceServer(grpcServer, server)
    
    return grpcServer.Serve(lis)
}
```

---

## 5. Pipeline Config (Simplified)

```go
// coreengine/config/pipeline.go

// EdgeLimit for per-edge transition limits.
// Capability defines graph structure. Core enforces bounds.
type EdgeLimit struct {
    From     string `json:"from"`
    To       string `json:"to"`
    MaxCount int    `json:"max_count"` // 0 = use global MaxIterations
}

type PipelineConfig struct {
    Name   string         `json:"name"`
    Agents []*AgentConfig `json:"agents"`

    // Bounds - Go enforces these authoritatively
    MaxIterations         int `json:"max_iterations"`
    MaxLLMCalls           int `json:"max_llm_calls"`
    MaxAgentHops          int `json:"max_agent_hops"`
    DefaultTimeoutSeconds int `json:"default_timeout_seconds"`

    // Per-edge cycle limits
    EdgeLimits []EdgeLimit `json:"edge_limits,omitempty"`

    // Internal
    adjacencyList map[string][]string
    edgeLimitMap  map[string]int
}

// buildGraph validates and builds graph.
// Core doesn't judge graph structure - capability defines it.
func (p *PipelineConfig) buildGraph(validNames map[string]bool) error {
    // Validate deps exist, build adjacency list, build edge limit map
    // NO cycle rejection - that's not core's concern
}
```

---

## 6. Implementation Plan (Aggressive)

### Phase 1: DELETE (Do First)
- [ ] Delete `dag_executor.go`
- [ ] Delete `CyclePolicyReject` (already done)
- [ ] Delete `EnableDAGExecution` flag (already done)
- [ ] Delete `topologicalOrder` computation (already done)
- [ ] Delete `EnableArbiter`, `SkipArbiterForReadOnly` (already done)
- [ ] Delete `RunsWith`, `JoinStrategy` from AgentConfig

### Phase 2: Implement Go
- [ ] Implement `state_machine_executor.go`
- [ ] Implement `envelope.Clone()`
- [ ] Implement gRPC server (`grpc/server.go`)
- [ ] Generate proto code

### Phase 3: Implement Python Client
- [ ] Implement `grpc_client.py`
- [ ] Delete `client.py` (subprocess)
- [ ] Update all callers to use gRPC
- [ ] Remove all `if go_client else python` patterns

### Phase 4: Verify & Clean
- [ ] Run tests (no fallbacks = failures are real)
- [ ] Delete any remaining backward compat code

---

## 7. Testing Strategy

### 7.1 Unit Tests

```go
func TestStateMachineExecutor_HandlesCycles(t *testing.T) {
    // Configure pipeline with REINTENT cycle
    cfg := &config.PipelineConfig{
        CyclePolicy: config.CyclePolicyAllow,
        EdgeLimits: []config.EdgeLimit{
            {From: "critic", To: "intent", MaxCount: 3},
        },
    }
    
    executor := NewStateMachineExecutor(cfg, agents, logger)
    
    // Execute with critic returning "reintent" verdict
    env := envelope.NewGenericEnvelope()
    env.Outputs["critic"] = map[string]any{"verdict": "reintent"}
    
    result, err := executor.Execute(ctx, env, "test-thread")
    
    // Should cycle back to intent
    assert.NoError(t, err)
    assert.Equal(t, 1, result.Iteration)
}

func TestStateMachineExecutor_EnforcesEdgeLimits(t *testing.T) {
    cfg := &config.PipelineConfig{
        CyclePolicy: config.CyclePolicyAllow,
        EdgeLimits: []config.EdgeLimit{
            {From: "critic", To: "intent", MaxCount: 2},
        },
    }
    
    // After 2 cycles, should route to end instead
    // ...
}

func TestEnvelope_Clone_IsDeepCopy(t *testing.T) {
    original := envelope.NewGenericEnvelope()
    original.Outputs["test"] = map[string]any{"key": "value"}
    
    clone := original.Clone()
    
    // Modify clone
    clone.Outputs["test"]["key"] = "modified"
    
    // Original should be unchanged
    assert.Equal(t, "value", original.Outputs["test"]["key"])
}
```

### 7.2 Integration Tests

```python
def test_grpc_envelope_roundtrip():
    """Verify gRPC envelope serialization."""
    with GrpcGoClient() as client:
        # Create via Go
        env = client.create_envelope("test", "u1", "s1")
        
        # Modify in Python
        env.outputs["test"] = {"key": "value"}
        
        # Check bounds via Go
        result = client.check_bounds(env)
        
        assert result.can_continue
        assert result.llm_calls_remaining == 10
```

---

## 8. File Changes Summary

| File | Action | Description |
|------|--------|-------------|
| `coreengine/runtime/dag_executor.go` | **DELETE** | Gone, not deprecated |
| `coreengine/runtime/state_machine_executor.go` | CREATE | The only executor |
| `coreengine/envelope/generic.go` | MODIFY | Add Clone() method |
| `coreengine/config/pipeline.go` | MODIFY | Remove bloat, add EdgeLimit |
| `coreengine/proto/jeeves_core.proto` | CREATE | gRPC service definition |
| `coreengine/grpc/server.go` | CREATE | gRPC server |
| `jeeves_protocols/grpc_client.py` | CREATE | The only Python client |
| `jeeves_protocols/client.py` | **DELETE** | Gone, not deprecated |

### Deleted Code Patterns

```python
# BEFORE (bloat)
if self._go_client:
    result = self._go_client.can_continue(envelope)
else:
    result = self._python_fallback(envelope)  # DELETE THIS

# AFTER (fail loudly)
result = self._grpc_client.can_continue(envelope)
# If Go is down, we crash. That's correct.
```

```go
// BEFORE (bloat)
if p.CyclePolicy == CyclePolicyReject {
    return fmt.Errorf("cycle detected")
}
// CyclePolicyAllow case...

// AFTER (core doesn't judge)
// No policy check. Capability defines structure. Edge limits enforce bounds.
```

---

*End of Design Document*

