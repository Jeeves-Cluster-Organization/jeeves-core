# Runtime - Pipeline Execution Engine

> **File Location**: `jeeves-core/coreengine/runtime/runtime.go`

The Runtime package provides the pipeline orchestration engine that executes agent sequences with support for sequential, parallel, and streaming modes.

## Table of Contents

- [Overview](#overview)
- [Key Types](#key-types)
- [Execution Modes](#execution-modes)
- [Run Options](#run-options)
- [Key Functions](#key-functions)
- [Bounds Enforcement](#bounds-enforcement)
- [Interrupt Handling](#interrupt-handling)
- [Usage Examples](#usage-examples)
- [Related Documentation](#related-documentation)

---

## Overview

The Runtime executes pipelines from configuration, managing:

- Agent instantiation from `AgentConfig`
- Sequential or parallel stage execution
- Bounds checking (iterations, LLM calls, agent hops)
- State persistence via `PersistenceAdapter`
- Interrupt handling for user interaction
- Edge limit enforcement for cyclic routing

---

## Key Types

### Runtime

The main orchestration engine:

```go
type Runtime struct {
    Config         *config.PipelineConfig
    LLMFactory     LLMProviderFactory
    ToolExecutor   agents.ToolExecutor
    Logger         agents.Logger
    Persistence    PersistenceAdapter
    PromptRegistry agents.PromptRegistry  // Prompt registry for agent prompt lookup
    UseMock        bool
    
    agents   map[string]*agents.Agent  // Internal: built agents
    eventCtx agents.EventContext              // Internal: event context
}
```

| Field | Type | Description |
|-------|------|-------------|
| `Config` | `*config.PipelineConfig` | Pipeline configuration |
| `LLMFactory` | `LLMProviderFactory` | Creates LLM providers by role |
| `ToolExecutor` | `agents.ToolExecutor` | Tool execution interface |
| `Logger` | `agents.Logger` | Structured logger |
| `Persistence` | `PersistenceAdapter` | State persistence adapter |
| `PromptRegistry` | `agents.PromptRegistry` | Prompt template registry |
| `UseMock` | `bool` | Enable mock handlers for testing |

### RunMode

Determines how pipeline stages execute:

```go
type RunMode string

const (
    RunModeSequential RunMode = "sequential"  // One stage at a time
    RunModeParallel   RunMode = "parallel"    // Independent stages concurrently
)
```

### RunOptions

Configures pipeline execution:

```go
type RunOptions struct {
    Mode     RunMode  // Sequential or parallel
    Stream   bool     // Send outputs to channel as they complete
    ThreadID string   // For state persistence (empty disables)
}
```

### StageOutput

Represents output from a completed stage:

```go
type StageOutput struct {
    Stage  string
    Output map[string]any
    Error  error
}
```

### LLMProviderFactory

Creates LLM providers by role:

```go
type LLMProviderFactory func(role string) agents.LLMProvider
```

### PersistenceAdapter

Handles state persistence:

```go
type PersistenceAdapter interface {
    SaveState(ctx context.Context, threadID string, state map[string]any) error
    LoadState(ctx context.Context, threadID string) (map[string]any, error)
}
```

---

## Execution Modes

### Sequential Mode

Stages execute one at a time following routing rules:

```
Stage A → Stage B → Stage C → end
              ↓
         [loop_back] → Stage A (with edge limits)
```

- Default execution mode
- Routing determined by agent output + routing rules
- Edge traversals tracked for cycle prevention
- Loop-back detection increments iteration counter

### Parallel Mode

Independent stages execute concurrently:

```
     ┌──── Stage A ────┐
     │                 │
Input ├──── Stage B ────┼──→ Output (merged)
     │                 │
     └──── Stage C ────┘
```

- Stages grouped by dependency satisfaction
- Results merged into main envelope
- `ParallelMode` flag set on envelope
- Stages identified as ready via `GetReadyStages()`

---

## Run Options

| Option | Type | Description |
|--------|------|-------------|
| `Mode` | `RunMode` | `sequential` or `parallel` |
| `Stream` | `bool` | Enable channel-based output streaming |
| `ThreadID` | `string` | Persistence thread ID (empty disables) |

---

## Key Functions

### Constructor

```go
func NewRuntime(
    cfg *config.PipelineConfig,
    llmFactory LLMProviderFactory,
    toolExecutor agents.ToolExecutor,
    logger agents.Logger,
) (*Runtime, error)
```

Creates a new Runtime, validating config and building agents.

### Execution Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `Execute` | `(ctx, env, opts) (*envelope, <-chan, error)` | Unified execution with options |
| `Run` | `(ctx, env, threadID) (*envelope, error)` | Sequential execution |
| `RunParallel` | `(ctx, env, threadID) (*envelope, error)` | Parallel execution |
| `RunWithStream` | `(ctx, env, threadID) (<-chan, error)` | Streaming sequential |
| `Resume` | `(ctx, env, response, threadID) (*envelope, error)` | Resume after interrupt |

### Execute

The primary execution method:

```go
func (r *Runtime) Execute(
    ctx context.Context,
    env *envelope.Envelope,
    opts RunOptions,
) (*envelope.Envelope, <-chan StageOutput, error)
```

**Behavior**:
1. Applies defaults from pipeline config
2. Initializes envelope with stage order and bounds
3. Executes in specified mode
4. Returns result envelope and optional output channel
5. Sends `__end__` marker to channel when complete

### Run

Convenience method for sequential execution:

```go
func (r *Runtime) Run(
    ctx context.Context,
    env *envelope.Envelope,
    threadID string,
) (*envelope.Envelope, error)
```

### RunParallel

Convenience method for parallel execution:

```go
func (r *Runtime) RunParallel(
    ctx context.Context,
    env *envelope.Envelope,
    threadID string,
) (*envelope.Envelope, error)
```

### RunWithStream

Streaming execution in goroutine:

```go
func (r *Runtime) RunWithStream(
    ctx context.Context,
    env *envelope.Envelope,
    threadID string,
) (<-chan StageOutput, error)
```

### Resume

Resume execution after interrupt:

```go
func (r *Runtime) Resume(
    ctx context.Context,
    env *envelope.Envelope,
    response envelope.InterruptResponse,
    threadID string,
) (*envelope.Envelope, error)
```

**Resume Stages** (configurable in PipelineConfig):
- `ClarificationResumeStage`: Where to resume after clarification
- `ConfirmationResumeStage`: Where to resume after approval
- `AgentReviewResumeStage`: Where to resume after agent review

### Other Methods

| Method | Description |
|--------|-------------|
| `SetEventContext(ctx)` | Set event context for all agents |
| `GetState(ctx, threadID)` | Get persisted state for thread |

---

## Bounds Enforcement

The Runtime enforces multiple bounds to prevent infinite loops:

### Global Bounds

| Bound | Default | Description |
|-------|---------|-------------|
| `MaxIterations` | 3 | Maximum pipeline loop iterations |
| `MaxLLMCalls` | 10 | Maximum total LLM calls |
| `MaxAgentHops` | 21 | Maximum agent transitions |

### Edge Limits

Per-edge transition limits (configured in `PipelineConfig.EdgeLimits`):

```go
EdgeLimits: []EdgeLimit{
    {From: "critic", To: "planner", MaxCount: 3},
}
```

### Iteration Tracking

Loop-back detection increments iteration when:
- Routing returns to an earlier stage in `StageOrder`
- Edge points from later stage to earlier stage

---

## Interrupt Handling

The Runtime supports interrupts for user interaction:

### Interrupt Types

| Type | Usage |
|------|-------|
| `clarification` | Need user clarification |
| `confirmation` | Need user confirmation |
| `agent_review` | Human-in-the-loop review |
| `checkpoint` | Checkpoint pause |
| `resource_exhausted` | Rate limit hit |
| `timeout` | Timeout occurred |
| `system_error` | System error |

### Interrupt Flow

1. Agent sets interrupt on envelope
2. Runtime detects `InterruptPending` flag
3. Execution pauses, returns current envelope
4. User provides response
5. Call `Resume()` with response
6. Execution continues from configured resume stage

---

## Usage Examples

### Basic Sequential Execution

```go
rt, err := runtime.NewRuntime(cfg, llmFactory, toolExecutor, logger)
if err != nil {
    return err
}

env := envelope.NewEnvelope()
env.RawInput = "Analyze the authentication system"

result, err := rt.Run(ctx, env, "thread_123")
if err != nil {
    return err
}

response := result.GetFinalResponse()
```

### Parallel Execution

```go
result, err := rt.RunParallel(ctx, env, "thread_123")
```

### Streaming with Progress

```go
outputChan, err := rt.RunWithStream(ctx, env, "")
if err != nil {
    return err
}

for output := range outputChan {
    if output.Stage == "__end__" {
        fmt.Println("Pipeline complete")
        break
    }
    
    fmt.Printf("Stage %s: %v\n", output.Stage, output.Output)
}
```

### Handling Interrupts

```go
result, err := rt.Run(ctx, env, "thread_123")
if err != nil {
    return err
}

if result.InterruptPending {
    // Get user input
    userResponse := getUserClarification(result.Interrupt.Question)
    
    // Resume with response
    response := envelope.InterruptResponse{
        Text: &userResponse,
    }
    
    result, err = rt.Resume(ctx, result, response, "thread_123")
}
```

### Context Cancellation

```go
ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
defer cancel()

result, err := rt.Run(ctx, env, "thread_123")
if err == context.DeadlineExceeded {
    fmt.Println("Pipeline timed out")
}
```

---

## Related Documentation

- [Core Engine Overview](README.md)
- [Agent System](agents.md)
- [Pipeline Configuration](config.md)
- [Envelope System](envelope.md)
