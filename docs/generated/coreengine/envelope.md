# Envelope - Envelope State Container

> **File Location**: `jeeves-core/coreengine/envelope/`

The Envelope package provides the `Envelope` - a dynamic state container for pipeline execution that replaces hardcoded per-agent output fields.

## Table of Contents

- [Overview](#overview)
- [Envelope](#genericenvelope)
- [State Tracking](#state-tracking)
- [Bounds Checking](#bounds-checking)
- [Interrupt System](#interrupt-system)
- [Parallel Execution State](#parallel-execution-state)
- [Enums](#enums)
- [Serialization](#serialization)
- [Usage Examples](#usage-examples)
- [Related Documentation](#related-documentation)

---

## Overview

The `Envelope` is the central state container that flows through the pipeline:

- **Dynamic Outputs**: Any agent can write to `Outputs[key]`
- **String-based Stages**: No enum, stages defined in configuration
- **Unified Interrupts**: Single interrupt system for all interrupt types
- **Bounds Tracking**: LLM calls, agent hops, iterations
- **Parallel Support**: Track active, completed, and failed stages

### Design Principles

- **Configuration-Driven**: Stage names are strings, not enums
- **Extensible**: Any agent can add outputs without schema changes
- **Cloneable**: Deep copy support for checkpointing
- **Serializable**: Full state persistence support

---

## Envelope

**File**: `envelope.go`

### Structure

```go
type Envelope struct {
    // Identification
    EnvelopeID string
    RequestID  string
    UserID     string
    SessionID  string
    
    // Original Input
    RawInput   string
    ReceivedAt time.Time
    
    // Dynamic Agent Outputs
    Outputs map[string]map[string]any
    
    // Pipeline State
    CurrentStage  string
    StageOrder    []string
    Iteration     int
    MaxIterations int
    
    // Parallel Execution State
    ActiveStages      map[string]bool
    CompletedStageSet map[string]bool
    FailedStages      map[string]string
    ParallelMode      bool
    
    // Bounds Tracking
    LLMCallCount    int
    MaxLLMCalls     int
    AgentHopCount   int
    MaxAgentHops    int
    TerminalReason_ *TerminalReason
    
    // Control Flow
    Terminated        bool
    TerminationReason *string
    
    // Interrupt Handling
    InterruptPending bool
    Interrupt        *FlowInterrupt
    
    // Multi-Stage Execution
    CompletedStages      []map[string]any
    CurrentStageNumber   int
    MaxStages            int
    AllGoals             []string
    RemainingGoals       []string
    GoalCompletionStatus map[string]string
    
    // Retry Context
    PriorPlans   []map[string]any
    LoopFeedback []string
    
    // Audit Trail
    ProcessingHistory []ProcessingRecord
    Errors            []map[string]any
    
    // Timing
    CreatedAt   time.Time
    CompletedAt *time.Time
    
    // Metadata
    Metadata map[string]any
}
```

### Constructor

```go
func NewEnvelope() *Envelope
```

Creates envelope with:
- Generated `EnvelopeID`, `RequestID`, `SessionID`
- Default bounds (`MaxIterations=3`, `MaxLLMCalls=10`, `MaxAgentHops=21`)
- Initialized maps and slices

### Factory Function

```go
func CreateEnvelope(
    userMessage, userID, sessionID string,
    requestID *string,
    metadata map[string]any,
    stageOrder []string,
) *Envelope
```

---

## State Tracking

### Output Access

```go
// Get output by key
output := env.GetOutput("plan")

// Set output by key
env.SetOutput("plan", map[string]any{
    "steps": steps,
    "reasoning": "...",
})

// Check if output exists
if env.HasOutput("plan") {
    // ...
}
```

### Processing History

```go
type ProcessingRecord struct {
    Agent       string
    StageOrder  int
    StartedAt   time.Time
    CompletedAt *time.Time
    DurationMS  int
    Status      string  // "running", "success", "error", "skipped"
    Error       *string
    LLMCalls    int
}
```

**Methods**:

```go
// Record agent start (increments AgentHopCount)
env.RecordAgentStart(agentName, stageOrder)

// Record agent completion
env.RecordAgentComplete(agentName, status, errorMsg, llmCalls, durationMS)
```

### Goal Tracking

```go
// Initialize goals from intent
env.InitializeGoals([]string{"goal1", "goal2", "goal3"})

// Advance stage after critic assessment
hasMore := env.AdvanceStage(satisfiedGoals, stageSummary)

// Get accumulated context
context := env.GetStageContext()
```

### Retry Management

```go
// Increment iteration for retry
env.IncrementIteration(feedback)
```

This:
- Increments `Iteration`
- Adds feedback to `LoopFeedback`
- Archives current plan to `PriorPlans`
- Clears downstream outputs

---

## Bounds Checking

### CanContinue

```go
func (e *Envelope) CanContinue() bool
```

Returns `false` if:
- `Terminated` is true
- `InterruptPending` is true
- `Iteration > MaxIterations` (sets `TerminalReasonMaxIterationsExceeded`)
- `LLMCallCount >= MaxLLMCalls` (sets `TerminalReasonMaxLLMCallsExceeded`)
- `AgentHopCount >= MaxAgentHops` (sets `TerminalReasonMaxAgentHopsExceeded`)

### Termination

```go
// Terminate with reason
env.Terminate("User cancelled", &envelope.TerminalReasonUserCancelled)
```

Sets:
- `Terminated = true`
- `TerminationReason` to message
- `TerminalReason_` to typed reason
- `CompletedAt` to current time

---

## Interrupt System

**File**: `envelope.go`

### InterruptKind

```go
type InterruptKind string

const (
    InterruptKindClarification     InterruptKind = "clarification"
    InterruptKindConfirmation      InterruptKind = "confirmation"
    InterruptKindAgentReview       InterruptKind = "agent_review"
    InterruptKindCheckpoint        InterruptKind = "checkpoint"
    InterruptKindResourceExhausted InterruptKind = "resource_exhausted"
    InterruptKindTimeout           InterruptKind = "timeout"
    InterruptKindSystemError       InterruptKind = "system_error"
)
```

### FlowInterrupt

```go
type FlowInterrupt struct {
    Kind       InterruptKind
    ID         string
    Question   string            // For clarification
    Message    string            // For confirmation
    Data       map[string]any    // Extensible payload
    Response   *InterruptResponse
    CreatedAt  time.Time
    ExpiresAt  *time.Time
}
```

### InterruptResponse

```go
type InterruptResponse struct {
    Text       *string           // For clarification
    Approved   *bool             // For confirmation
    Decision   *string           // For critic (approve/reject/modify)
    Data       map[string]any    // Extensible
    ReceivedAt time.Time
}
```

### Interrupt Methods

```go
// Set interrupt with options
env.SetInterrupt(
    envelope.InterruptKindClarification,
    "int_123",
    envelope.WithQuestion("What database?"),
    envelope.WithExpiry(5 * time.Minute),
)

// Resolve interrupt
env.ResolveInterrupt(envelope.InterruptResponse{
    Text: ptr("PostgreSQL"),
})

// Clear interrupt without response
env.ClearInterrupt()

// Check for pending interrupt
if env.HasPendingInterrupt() {
    kind := env.GetInterruptKind()
}
```

### Interrupt Options

```go
type InterruptOption func(*FlowInterrupt)

WithQuestion(q string)           // Set question
WithMessage(m string)            // Set message
WithExpiry(d time.Duration)      // Set expiry
WithInterruptData(d map)         // Set data
```

---

## Parallel Execution State

### Stage Tracking

```go
// Mark stage as active
env.StartStage("analyzer")

// Mark stage as completed
env.CompleteStage("analyzer")

// Mark stage as failed
env.FailStage("analyzer", "timeout error")
```

### Stage Queries

```go
env.IsStageCompleted("analyzer")  // bool
env.IsStageActive("analyzer")     // bool
env.IsStageFailed("analyzer")     // bool
env.GetActiveStageCount()         // int
env.GetCompletedStageCount()      // int
env.AllStagesComplete()           // bool
env.HasFailures()                 // bool
```

---

## Enums

**File**: `enums.go`

### TerminalReason

```go
type TerminalReason string

const (
    TerminalReasonCompletedSuccessfully TerminalReason = "completed_successfully"
    TerminalReasonCompleted             TerminalReason = "completed"
    TerminalReasonClarificationRequired TerminalReason = "clarification_required"
    TerminalReasonConfirmationRequired  TerminalReason = "confirmation_required"
    TerminalReasonDeniedByPolicy        TerminalReason = "denied_by_policy"
    TerminalReasonToolFailedRecoverably TerminalReason = "tool_failed_recoverably"
    TerminalReasonToolFailedFatally     TerminalReason = "tool_failed_fatally"
    TerminalReasonLLMFailedFatally      TerminalReason = "llm_failed_fatally"
    TerminalReasonMaxIterationsExceeded TerminalReason = "max_iterations_exceeded"
    TerminalReasonMaxLLMCallsExceeded   TerminalReason = "max_llm_calls_exceeded"
    TerminalReasonMaxAgentHopsExceeded  TerminalReason = "max_agent_hops_exceeded"
    TerminalReasonMaxLoopExceeded       TerminalReason = "max_loop_exceeded"
    TerminalReasonUserCancelled         TerminalReason = "user_cancelled"
)
```

### RiskApproval

```go
type RiskApproval string

const (
    RiskApprovalApproved             RiskApproval = "approved"
    RiskApprovalDenied               RiskApproval = "denied"
    RiskApprovalRequiresConfirmation RiskApproval = "requires_confirmation"
)
```

### LoopVerdict

```go
type LoopVerdict string

const (
    LoopVerdictProceed  LoopVerdict = "proceed"    // Continue to next stage
    LoopVerdictLoopBack LoopVerdict = "loop_back"  // Return to earlier stage
    LoopVerdictAdvance  LoopVerdict = "advance"    // Skip to later stage
)
```

---

## Serialization

### ToStateDict

```go
state := env.ToStateDict()
```

Returns complete envelope state as `map[string]any` for persistence.

### FromStateDict

```go
env := envelope.FromStateDict(state)
```

Reconstructs envelope from persisted state.

### ToResultDict

```go
result := env.ToResultDict()
```

Returns result dictionary for API responses with:
- Identification fields
- Current stage and status
- Response and interrupt info
- Timing and error info

### TotalProcessingTimeMS

```go
totalMS := env.TotalProcessingTimeMS()
```

Calculates total processing time from processing history records.

### Clone

```go
clone := env.Clone()
```

Creates deep copy for checkpointing. All nested maps and slices are copied.

**FlowInterrupt also supports cloning**:

```go
interruptClone := interrupt.Clone()
```

---

## Usage Examples

### Creating and Populating

```go
env := envelope.NewEnvelope()
env.RawInput = "Analyze the authentication module"
env.UserID = "user_123"
env.SessionID = "sess_456"

// Set metadata
env.Metadata["source"] = "api"
env.Metadata["priority"] = "high"
```

### Working with Outputs

```go
// Planner sets output
env.SetOutput("plan", map[string]any{
    "steps": []map[string]any{
        {"tool": "read_file", "parameters": map[string]any{"path": "auth.py"}},
        {"tool": "analyze_code", "parameters": map[string]any{"target": "auth.py"}},
    },
    "reasoning": "Need to understand auth flow",
})

// Executor reads plan
plan := env.GetOutput("plan")
steps := plan["steps"].([]any)
```

### Handling Interrupts

```go
// Set clarification interrupt
env.SetInterrupt(
    envelope.InterruptKindClarification,
    "clarify_123",
    envelope.WithQuestion("Which authentication method: OAuth or JWT?"),
    envelope.WithExpiry(10 * time.Minute),
)

// Check in handler
if env.HasPendingInterrupt() {
    return env, nil  // Pause execution
}

// Later, resolve interrupt
env.ResolveInterrupt(envelope.InterruptResponse{
    Text: ptr("OAuth 2.0"),
})
```

### Checking Bounds

```go
if !env.CanContinue() {
    if env.TerminalReason_ != nil {
        switch *env.TerminalReason_ {
        case envelope.TerminalReasonMaxLLMCallsExceeded:
            log.Println("LLM budget exhausted")
        case envelope.TerminalReasonMaxIterationsExceeded:
            log.Println("Too many iterations")
        }
    }
    return env, nil
}
```

### Persistence

```go
// Save state
state := env.ToStateDict()
data, _ := json.Marshal(state)
db.Save(env.EnvelopeID, data)

// Load state
var state map[string]any
data, _ := db.Load(envelopeID)
json.Unmarshal(data, &state)
env := envelope.FromStateDict(state)
```

### Checkpointing

```go
// Create checkpoint before risky operation
checkpoint := env.Clone()

// If operation fails, restore
if err != nil {
    env = checkpoint
}
```

---

## Related Documentation

- [Core Engine Overview](README.md)
- [Runtime Execution](runtime.md)
- [Agent System](agents.md)
- [Pipeline Configuration](config.md)
