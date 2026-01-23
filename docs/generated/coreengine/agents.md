# Agents - Unified Agent System

> **File Location**: `jeeves-core/coreengine/agents/`

The Agents package provides the `Agent` - a single, configuration-driven agent class that replaces individual agent implementations.

## Table of Contents

- [Overview](#overview)
- [Agent](#unifiedagent)
- [Contracts and Outcomes](#contracts-and-outcomes)
- [Agent Lifecycle](#agent-lifecycle)
- [Interfaces](#interfaces)
- [Tool Results](#tool-results)
- [Usage Examples](#usage-examples)
- [Related Documentation](#related-documentation)

---

## Overview

The centralized architecture (v4.0) uses a single `Agent` class that:

- Is driven entirely by configuration (`AgentConfig`)
- Supports LLM, tool execution, or service processing modes
- Uses hooks for capability layer customization
- Handles routing via configurable rules

### Design Principles

- **Configuration over Code**: Agent behavior defined declaratively
- **Capability Flags**: `HasLLM`, `HasTools`, `HasPolicies` determine processing mode
- **Hook System**: Pre/post-processing hooks for extensibility
- **Standardized Results**: Uniform tool result structure

---

## Agent

**File**: `agent.go`

### Structure

```go
type Agent struct {
    Config         *config.AgentConfig
    Name           string
    Logger         Logger
    LLM            LLMProvider
    Tools          ToolExecutor
    EventCtx       EventContext
    PromptRegistry PromptRegistry
    UseMock        bool
    
    // Hooks (set by capability layer)
    PreProcess  ProcessHook
    PostProcess OutputHook
    MockHandler MockHandler
}
```

### Constructor

```go
func NewAgent(
    cfg *config.AgentConfig,
    logger Logger,
    llm LLMProvider,
    tools ToolExecutor,
) (*Agent, error)
```

Validates configuration and creates the agent. Returns error if:
- Configuration is invalid
- `HasLLM=true` but no LLM provider given
- `HasTools=true` but no tool executor given

### Key Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `Process` | `(ctx, env) (*env, error)` | Process envelope through agent |
| `SetEventContext` | `(ctx EventContext)` | Set event context for emissions |

### Processing Modes

The agent selects processing mode based on configuration:

| Priority | Condition | Handler |
|----------|-----------|---------|
| 1 | `UseMock && MockHandler != nil` | MockHandler |
| 2 | `Config.HasLLM` | llmProcess |
| 3 | `Config.HasTools` | toolProcess |
| 4 | Default | serviceProcess |

---

## Contracts and Outcomes

**File**: `contracts.go`

### AgentOutcome

Explicit outcome types for agent execution:

```go
type AgentOutcome string

const (
    AgentOutcomeSuccess   AgentOutcome = "success"
    AgentOutcomeError     AgentOutcome = "error"
    AgentOutcomeClarify   AgentOutcome = "clarify"
    AgentOutcomeConfirm   AgentOutcome = "confirm"
    AgentOutcomeReplan    AgentOutcome = "replan"
    AgentOutcomeLoopBack  AgentOutcome = "loop_back"
    AgentOutcomeTerminate AgentOutcome = "terminate"
    AgentOutcomeSkip      AgentOutcome = "skip"
    AgentOutcomePartial   AgentOutcome = "partial"
)
```

### Outcome Methods

```go
// IsTerminal checks if outcome terminates the pipeline
func (o AgentOutcome) IsTerminal() bool

// IsSuccess checks if outcome indicates success
func (o AgentOutcome) IsSuccess() bool

// RequiresLoop checks if outcome requires looping back
func (o AgentOutcome) RequiresLoop() bool
```

### ToolStatus

Status of tool execution:

```go
type ToolStatus string

const (
    ToolStatusSuccess ToolStatus = "success"
    ToolStatusError   ToolStatus = "error"
)

// Parse status from string
func ToolStatusFromString(value string) (ToolStatus, error)
```

### IdempotencyClass

Classification for tool idempotency:

```go
type IdempotencyClass string

const (
    IdempotencyClassSafe          IdempotencyClass = "safe"
    IdempotencyClassIdempotent    IdempotencyClass = "idempotent"
    IdempotencyClassNonIdempotent IdempotencyClass = "non_idempotent"
)

// Infer idempotency class from risk level
func IdempotencyClassFromRiskLevel(riskLevel commbus.RiskLevel) IdempotencyClass
```

### ValidationSeverity

Severity levels for validation issues:

```go
type ValidationSeverity string

const (
    ValidationSeverityCritical ValidationSeverity = "critical"
    ValidationSeverityError    ValidationSeverity = "error"
    ValidationSeverityWarning  ValidationSeverity = "warning"
)
```

---

## Agent Lifecycle

### Processing Flow

```
┌─────────────────┐
│  RecordStart    │  Record agent start, increment hop count
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   PreProcess    │  Hook: PreProcess(env) → env
│     Hook        │  (Optional, set by capability layer)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Main Process   │  LLM / Tool / Service processing
│                 │  based on Config capabilities
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ ValidateOutput  │  Check RequiredOutputFields
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   SetOutput     │  Store output in envelope
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  PostProcess    │  Hook: PostProcess(env, output) → env
│     Hook        │  (Optional, set by capability layer)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ EvaluateRouting │  Determine next stage from rules
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ RecordComplete  │  Record completion, emit events
└─────────────────┘
```

### LLM Processing

When `HasLLM=true`:

1. Build prompt (from registry or fallback)
2. Configure LLM options (temperature, max tokens)
3. Call LLM provider
4. Parse JSON response
5. Return parsed output

### Tool Processing

When `HasTools=true`:

1. Get plan from envelope (`outputs["plan"]`)
2. Iterate over plan steps
3. For each step:
   - Check tool access permission
   - Execute tool with parameters
   - Record result (success/error with timing)
4. Return aggregated results

### Service Processing

Default mode returns empty output. Capability layer hooks add logic.

---

## Interfaces

### LLMProvider

```go
type LLMProvider interface {
    Generate(ctx context.Context, model string, prompt string, options map[string]any) (string, error)
}
```

### ToolExecutor

```go
type ToolExecutor interface {
    Execute(ctx context.Context, toolName string, params map[string]any) (map[string]any, error)
}
```

### Logger

```go
type Logger interface {
    Info(msg string, fields ...any)
    Debug(msg string, fields ...any)
    Warn(msg string, fields ...any)
    Error(msg string, fields ...any)
    Bind(fields ...any) Logger
}
```

### EventContext

```go
type EventContext interface {
    EmitAgentStarted(agentName string) error
    EmitAgentCompleted(agentName string, status string, durationMS int, err error) error
}
```

### PromptRegistry

```go
type PromptRegistry interface {
    Get(key string, context map[string]any) (string, error)
}
```

### Hook Types

```go
// Called during processing
type ProcessHook func(env *envelope.Envelope) (*envelope.Envelope, error)

// Called with output
type OutputHook func(env *envelope.Envelope, output map[string]any) (*envelope.Envelope, error)

// Generates mock output for testing
type MockHandler func(env *envelope.Envelope) (map[string]any, error)
```

---

## Tool Results

### StandardToolResult

Standardized tool result structure:

```go
type StandardToolResult struct {
    Status  ToolStatus
    Data    map[string]any
    Error   *ToolErrorDetails
    Message *string
}
```

**Constructors**:

```go
func NewStandardToolResultSuccess(data map[string]any, message *string) *StandardToolResult
func NewStandardToolResultFailure(err *ToolErrorDetails, message *string) *StandardToolResult
```

**Methods**:

```go
// Validate validates cross-field constraints
func (r *StandardToolResult) Validate() error
```

### ToolErrorDetails

Standardized error structure:

```go
type ToolErrorDetails struct {
    ErrorType   string
    Message     string
    Details     map[string]any
    Recoverable bool
}
```

**Factory Functions**:

```go
func NewToolErrorDetails(errorType, message string, recoverable bool) *ToolErrorDetails
func ToolErrorDetailsFromError(err error, recoverable bool) *ToolErrorDetails
func ToolErrorDetailsNotFound(entity, identifier string) *ToolErrorDetails
```

### ToolResultData

Standardized tool output structure:

```go
type ToolResultData struct {
    ItemsExplored []string
    ItemsPending  []string
    Discoveries   []DiscoveredItem
    Evidence      []Evidence
    Raw           map[string]any
}
```

**Constructor**:

```go
// Creates ToolResultData from a map, extracting known fields
func NewToolResultData(data map[string]any) *ToolResultData
```

### NormalizeToolResult

Converts legacy tool results to `StandardToolResult`:

```go
func NormalizeToolResult(result any) (*StandardToolResult, error)
```

---

## Usage Examples

### Creating an Agent

```go
cfg := &config.AgentConfig{
    Name:       "planner",
    StageOrder: 1,
    HasLLM:     true,
    ModelRole:  "planner",
    PromptKey:  "planner_prompt",
    OutputKey:  "plan",
    RequiredOutputFields: []string{"steps", "reasoning"},
    DefaultNext: "executor",
    RoutingRules: []config.RoutingRule{
        {Condition: "needs_clarification", Value: true, Target: "clarification"},
    },
}

agent, err := agents.NewAgent(cfg, logger, llmProvider, nil)
```

### Adding Hooks

```go
// Pre-process hook
agent.PreProcess = func(env *envelope.Envelope) (*envelope.Envelope, error) {
    // Add context to envelope
    env.Metadata["enhanced"] = true
    return env, nil
}

// Post-process hook
agent.PostProcess = func(env *envelope.Envelope, output map[string]any) (*envelope.Envelope, error) {
    // Validate plan structure
    if steps, ok := output["steps"].([]any); ok && len(steps) == 0 {
        return nil, fmt.Errorf("empty plan")
    }
    return env, nil
}
```

### Mock Testing

```go
agent.UseMock = true
agent.MockHandler = func(env *envelope.Envelope) (map[string]any, error) {
    return map[string]any{
        "steps": []map[string]any{
            {"tool": "read_file", "parameters": map[string]any{"path": "main.go"}},
        },
        "reasoning": "Test plan",
    }, nil
}

result, err := agent.Process(ctx, env)
```

### Processing Envelope

```go
env := envelope.NewEnvelope()
env.RawInput = "Analyze authentication flow"

result, err := agent.Process(ctx, env)
if err != nil {
    log.Printf("Agent failed: %v", err)
    return
}

plan := result.GetOutput("plan")
fmt.Printf("Generated %d steps\n", len(plan["steps"].([]any)))
```

---

## Related Documentation

- [Core Engine Overview](README.md)
- [Runtime Execution](runtime.md)
- [Pipeline Configuration](config.md)
- [Envelope System](envelope.md)
- [Tool Executor](tools.md)
