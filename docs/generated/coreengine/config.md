# Configuration - Pipeline and Agent Configuration

> **File Location**: `jeeves-core/coreengine/config/`

The Config package provides declarative configuration for pipelines and agents, including core orchestration settings, routing rules, and bounds.

## Table of Contents

- [Overview](#overview)
- [ExecutionConfig](#coreconfig)
- [PipelineConfig](#pipelineconfig)
- [AgentConfig](#agentconfig)
- [Routing Rules](#routing-rules)
- [Edge Limits](#edge-limits)
- [Config Providers](#config-providers)
- [Usage Examples](#usage-examples)
- [Related Documentation](#related-documentation)

---

## Overview

Configuration is split between:

| Config Type | Purpose | Location |
|-------------|---------|----------|
| `ExecutionConfig` | Orchestration settings (timeouts, limits, flags) | `execution_config.go` |
| `PipelineConfig` | Pipeline structure (agents, bounds, routing) | `pipeline.go` |
| `AgentConfig` | Individual agent settings | `pipeline.go` |

### Design Principles

- **Infrastructure-Agnostic**: No database URLs or LLM endpoints
- **Cyclic Routing Support**: Pipelines can have cycles with bounds
- **Declarative Agents**: Agents defined by configuration, not code
- **Dependency Injection**: ConfigProvider interface for testability

---

## ExecutionConfig

**File**: `execution_config.go`

Core orchestration configuration:

```go
type ExecutionConfig struct {
    // Execution Limits
    MaxPlanSteps      int
    MaxToolRetries    int
    MaxLLMRetries     int
    MaxLoopIterations int
    
    // Timeouts (seconds)
    LLMTimeout      int
    ExecutorTimeout int
    ToolTimeout     int
    AgentTimeout    int
    
    // Confidence Thresholds
    ClarificationThreshold  float64
    HighConfidenceThreshold float64
    
    // Agent Behavior
    MetaValidationEnabled bool
    MetaValidationDelay   float64
    
    // Orchestration Feature Flags
    EnableLoopBack                    bool
    EnableArbiter                     bool
    SkipArbiterForReadOnly            bool
    RequireConfirmationForDestructive bool
    
    // Loop Control
    MaxReplanIterations    int
    MaxLoopBackRejections  int
    ReplanOnPartialSuccess bool
    
    // Determinism
    LLMSeed                    *int
    StrictTransitionValidation bool
    
    // Idempotency
    EnableIdempotency           bool
    IdempotencyTTLMs            int
    EnforceIdempotencyForTools  bool
    EnforceIdempotencyForAgents bool
    
    // Logging
    LogLevel string
}
```

### Default Values

```go
func DefaultExecutionConfig() *ExecutionConfig {
    return &ExecutionConfig{
        MaxPlanSteps:      20,
        MaxToolRetries:    2,
        MaxLLMRetries:     3,
        MaxLoopIterations: 10,
        
        LLMTimeout:      120,
        ExecutorTimeout: 60,
        ToolTimeout:     30,
        AgentTimeout:    300,
        
        ClarificationThreshold:  0.7,
        HighConfidenceThreshold: 0.9,
        
        EnableLoopBack:                    true,
        EnableArbiter:                     true,
        SkipArbiterForReadOnly:            true,
        RequireConfirmationForDestructive: true,
        
        MaxReplanIterations:    3,
        MaxLoopBackRejections:  2,
        ReplanOnPartialSuccess: true,
        
        StrictTransitionValidation: true,
        
        EnableIdempotency:          true,
        IdempotencyTTLMs:           3600000, // 1 hour
        EnforceIdempotencyForTools:  true,
        EnforceIdempotencyForAgents: false,
        
        LogLevel: "INFO",
    }
}
```

### ExecutionConfig Functions

| Function | Description |
|----------|-------------|
| `DefaultExecutionConfig()` | Returns config with defaults |
| `ExecutionConfigFromMap(map)` | Creates config from map |
| `ToMap()` | Converts config to map |
| `GetGlobalExecutionConfig()` | Gets global instance |
| `SetExecutionConfig(config)` | Sets global instance |
| `ResetExecutionConfig()` | Resets to nil (for testing) |

---

## PipelineConfig

**File**: `pipeline.go`

Pipeline structure configuration:

```go
type PipelineConfig struct {
    Name   string
    Agents []*AgentConfig
    
    // Run Mode
    DefaultRunMode RunMode  // "sequential" or "parallel"
    
    // Bounds
    MaxIterations         int
    MaxLLMCalls           int
    MaxAgentHops          int
    DefaultTimeoutSeconds int
    
    // Edge Limits (for cyclic routing)
    EdgeLimits []EdgeLimit
    
    // Resume Stages (for interrupts)
    ClarificationResumeStage string
    ConfirmationResumeStage  string
    AgentReviewResumeStage   string
}
```

### Constructor

```go
func NewPipelineConfig(name string) *PipelineConfig
```

Creates pipeline with defaults:
- `MaxIterations`: 3
- `MaxLLMCalls`: 10
- `MaxAgentHops`: 21
- `DefaultTimeoutSeconds`: 300

### PipelineConfig Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `AddAgent` | `(agent *AgentConfig) error` | Add agent to pipeline |
| `Validate` | `() error` | Validate configuration |
| `GetAgent` | `(name string) *AgentConfig` | Get agent by name |
| `GetStageOrder` | `() []string` | Get ordered agent names |
| `GetReadyStages` | `(completed map) []string` | Get stages ready to execute |
| `GetDependents` | `(stage string) []string` | Get dependent stages |
| `GetEdgeLimit` | `(from, to string) int` | Get edge transition limit |

### RunMode

```go
type RunMode string

const (
    RunModeSequential RunMode = "sequential"
    RunModeParallel   RunMode = "parallel"
)
```

---

## AgentConfig

**File**: `pipeline.go`

Individual agent configuration:

```go
type AgentConfig struct {
    // Identity
    Name       string
    StageOrder int
    
    // Dependencies
    Requires     []string      // Hard dependencies
    After        []string      // Soft ordering
    JoinStrategy JoinStrategy  // JoinAll or JoinAny
    
    // Capability Flags
    HasLLM      bool
    HasTools    bool
    HasPolicies bool
    
    // Tool Access
    ToolAccess   ToolAccess
    AllowedTools map[string]bool
    
    // LLM Configuration
    ModelRole   string
    PromptKey   string
    Temperature *float64
    MaxTokens   *int
    
    // Output Configuration
    OutputKey            string
    RequiredOutputFields []string
    
    // Routing Configuration
    RoutingRules []RoutingRule
    DefaultNext  string
    ErrorNext    string
    
    // Bounds
    TimeoutSeconds int
    MaxRetries     int
}
```

### ToolAccess

```go
type ToolAccess string

const (
    ToolAccessNone  ToolAccess = "none"   // No tool access
    ToolAccessRead  ToolAccess = "read"   // Read-only tools
    ToolAccessWrite ToolAccess = "write"  // Write-only tools
    ToolAccessAll   ToolAccess = "all"    // All tools
)
```

### AgentCapability

```go
type AgentCapability string

const (
    CapabilityLLM      AgentCapability = "llm"
    CapabilityTools    AgentCapability = "tools"
    CapabilityPolicies AgentCapability = "policies"
    CapabilityService  AgentCapability = "service"
)
```

### JoinStrategy

For handling multiple prerequisites:

```go
type JoinStrategy string

const (
    JoinAll JoinStrategy = "all"  // Wait for ALL prerequisites
    JoinAny JoinStrategy = "any"  // Proceed when ANY completes
)
```

### AgentConfig Methods

| Method | Description |
|--------|-------------|
| `Validate()` | Validate agent configuration |
| `GetAllDependencies()` | Returns Requires + After |
| `HasDependencies()` | Returns true if any dependencies |

---

## Routing Rules

**File**: `pipeline.go`

Conditional transitions between stages:

```go
type RoutingRule struct {
    Condition string  // Key in agent output to check
    Value     any     // Expected value
    Target    string  // Next stage to route to
}
```

### Routing Evaluation

Rules are evaluated in order. First match determines next stage:

```go
// Agent output: {"verdict": "loop_back", "confidence": 0.3}
RoutingRules: []RoutingRule{
    {Condition: "verdict", Value: "loop_back", Target: "planner"},
    {Condition: "verdict", Value: "proceed", Target: "synthesizer"},
}
// → Routes to "planner"
```

If no rule matches, uses `DefaultNext`. If that's empty, uses `"end"`.

---

## Edge Limits

**File**: `pipeline.go`

Per-edge transition limits for cyclic routing:

```go
type EdgeLimit struct {
    From     string  // Source stage
    To       string  // Target stage
    MaxCount int     // Maximum transitions (0 = use global)
}
```

### Example

```go
EdgeLimits: []EdgeLimit{
    {From: "critic", To: "planner", MaxCount: 3},
    {From: "executor", To: "planner", MaxCount: 2},
}
```

This allows critic→planner up to 3 times and executor→planner up to 2 times per request.

---

## Config Providers

**File**: `execution_config.go`

Interface for dependency injection:

```go
type ConfigProvider interface {
    GetExecutionConfig() *ExecutionConfig
}
```

### DefaultConfigProvider

Uses global configuration:

```go
type DefaultConfigProvider struct{}

func (p *DefaultConfigProvider) GetExecutionConfig() *ExecutionConfig {
    return GetGlobalExecutionConfig()
}
```

### StaticConfigProvider

For testing with specific config:

```go
type StaticConfigProvider struct {
    Config *ExecutionConfig
}

func NewStaticConfigProvider(config *ExecutionConfig) *StaticConfigProvider
```

---

## Usage Examples

### Creating a Pipeline

```go
cfg := config.NewPipelineConfig("code-analysis")
cfg.MaxIterations = 5
cfg.MaxLLMCalls = 15

// Add planner
cfg.AddAgent(&config.AgentConfig{
    Name:       "planner",
    StageOrder: 1,
    HasLLM:     true,
    ModelRole:  "planner",
    PromptKey:  "planner_v2",
    OutputKey:  "plan",
    RequiredOutputFields: []string{"steps"},
    DefaultNext: "executor",
    RoutingRules: []config.RoutingRule{
        {Condition: "needs_clarification", Value: true, Target: "clarification"},
    },
})

// Add executor
cfg.AddAgent(&config.AgentConfig{
    Name:       "executor",
    StageOrder: 2,
    HasTools:   true,
    ToolAccess: config.ToolAccessAll,
    OutputKey:  "execution",
    DefaultNext: "critic",
})

// Add critic with loop-back
cfg.AddAgent(&config.AgentConfig{
    Name:       "critic",
    StageOrder: 3,
    HasLLM:     true,
    ModelRole:  "critic",
    OutputKey:  "critic",
    DefaultNext: "synthesizer",
    RoutingRules: []config.RoutingRule{
        {Condition: "verdict", Value: "loop_back", Target: "planner"},
    },
})

// Add edge limits
cfg.EdgeLimits = []config.EdgeLimit{
    {From: "critic", To: "planner", MaxCount: 3},
}

// Validate
if err := cfg.Validate(); err != nil {
    log.Fatal(err)
}
```

### Using ExecutionConfig

```go
// Get defaults
cfg := config.DefaultExecutionConfig()

// Modify
cfg.MaxPlanSteps = 30
cfg.LLMTimeout = 180
cfg.EnableLoopBack = true

// Set globally
config.SetExecutionConfig(cfg)

// Or use from map
cfg = config.ExecutionConfigFromMap(map[string]any{
    "max_plan_steps": 30,
    "llm_timeout":    180,
})
```

### Parallel Pipeline with Dependencies

```go
cfg := config.NewPipelineConfig("parallel-analysis")

// Independent stages (can run in parallel)
cfg.AddAgent(&config.AgentConfig{
    Name:       "syntax_analysis",
    StageOrder: 1,
    HasTools:   true,
    OutputKey:  "syntax",
})

cfg.AddAgent(&config.AgentConfig{
    Name:       "semantic_analysis",
    StageOrder: 1,  // Same stage order = parallel
    HasTools:   true,
    OutputKey:  "semantic",
})

// Dependent stage (waits for both)
cfg.AddAgent(&config.AgentConfig{
    Name:       "synthesizer",
    StageOrder: 2,
    HasLLM:     true,
    Requires:   []string{"syntax_analysis", "semantic_analysis"},
    OutputKey:  "report",
    DefaultNext: "end",
})
```

### Using ConfigProvider

```go
// For testing
provider := config.NewStaticConfigProvider(&config.ExecutionConfig{
    MaxPlanSteps: 5,
    LLMTimeout:   10,
})

cfg := provider.GetExecutionConfig()
```

---

## Related Documentation

- [Core Engine Overview](README.md)
- [Runtime Execution](runtime.md)
- [Agent System](agents.md)
- [Envelope System](envelope.md)
