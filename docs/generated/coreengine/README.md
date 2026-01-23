# Core Engine

> **File Location**: `jeeves-core/coreengine/`

The Core Engine is the orchestration layer of the Jeeves system, providing pipeline execution, agent management, configuration, and inter-process communication via gRPC.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Package Structure](#package-structure)
- [Quick Start](#quick-start)
- [Related Documentation](#related-documentation)

---

## Overview

The Core Engine implements the runtime infrastructure for executing agent pipelines. It is designed with the following principles:

- **Protocol-First Design**: All components depend on protocols, not implementations
- **Configuration-Driven Agents**: Agents are defined declaratively, not as code
- **Cyclic Routing Support**: Pipeline graphs can have cycles with bounds enforcement
- **Parallel Execution**: Independent stages can run concurrently
- **Go-Native Bounds Checking**: Resource limits enforced authoritatively in Go

### Key Capabilities

| Capability | Description |
|------------|-------------|
| **Pipeline Runtime** | Execute multi-stage agent pipelines |
| **Unified Agent** | Single agent class driven by configuration |
| **Generic Envelope** | Dynamic state container for pipeline execution |
| **Tool Execution** | Registry and execution of tools |
| **gRPC Server** | IPC mechanism between Python and Go |
| **Configuration** | Declarative pipeline and agent configuration |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                       Core Engine                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │   Runtime    │───▶│    Agents    │───▶│  Tool Executor   │   │
│  │              │    │              │    │                  │   │
│  │ - Sequential │    │ - Unified    │    │ - Registry       │   │
│  │ - Parallel   │    │ - Contracts  │    │ - Execute        │   │
│  │ - Streaming  │    │ - Hooks      │    │ - Risk Levels    │   │
│  └──────────────┘    └──────────────┘    └──────────────────┘   │
│         │                   │                                    │
│         ▼                   ▼                                    │
│  ┌──────────────┐    ┌──────────────┐                           │
│  │   Envelope   │◀───│    Config    │                           │
│  │              │    │              │                           │
│  │ - State      │    │ - Pipeline   │                           │
│  │ - Outputs    │    │ - Agent      │                           │
│  │ - Interrupts │    │ - Routing    │                           │
│  └──────────────┘    └──────────────┘                           │
│                                                                  │
├─────────────────────────────────────────────────────────────────┤
│                        gRPC Server                               │
│                  (Python ↔ Go Communication)                     │
└─────────────────────────────────────────────────────────────────┘
```

### Execution Flow

1. **Envelope Creation**: Create a `GenericEnvelope` with user input
2. **Pipeline Configuration**: Load `PipelineConfig` with agent definitions
3. **Runtime Initialization**: Build `Runtime` with LLM factory and tools
4. **Execution**: Run pipeline (sequential, parallel, or streaming)
5. **Bounds Checking**: Enforce limits on iterations, LLM calls, and hops
6. **Interrupt Handling**: Support for clarification/confirmation interrupts
7. **Result Extraction**: Get final response from envelope

---

## Package Structure

| Package | Description | Documentation |
|---------|-------------|---------------|
| `runtime/` | Pipeline execution engine | [runtime.md](runtime.md) |
| `agents/` | UnifiedAgent and contracts | [agents.md](agents.md) |
| `config/` | Pipeline and agent configuration | [config.md](config.md) |
| `envelope/` | GenericEnvelope state container | [envelope.md](envelope.md) |
| `tools/` | Tool registry and executor | [tools.md](tools.md) |
| `grpc/` | gRPC server implementation | [grpc.md](grpc.md) |
| `proto/` | Protocol buffer definitions | - |
| `typeutil/` | Type-safe utility functions | - |
| `testutil/` | Testing utilities | - |

---

## Quick Start

### Creating a Pipeline

```go
import (
    "github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
    "github.com/jeeves-cluster-organization/codeanalysis/coreengine/runtime"
    "github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// 1. Create pipeline configuration
cfg := config.NewPipelineConfig("analysis-pipeline")
cfg.MaxIterations = 3
cfg.MaxLLMCalls = 10

// 2. Add agents
cfg.AddAgent(&config.AgentConfig{
    Name:       "planner",
    StageOrder: 1,
    HasLLM:     true,
    ModelRole:  "planner",
    OutputKey:  "plan",
    DefaultNext: "executor",
})

cfg.AddAgent(&config.AgentConfig{
    Name:       "executor",
    StageOrder: 2,
    HasTools:   true,
    ToolAccess: config.ToolAccessAll,
    OutputKey:  "execution",
    DefaultNext: "end",
})

// 3. Validate configuration
if err := cfg.Validate(); err != nil {
    log.Fatal(err)
}

// 4. Create runtime
rt, err := runtime.NewRuntime(cfg, llmFactory, toolExecutor, logger)
if err != nil {
    log.Fatal(err)
}

// 5. Create envelope
env := envelope.NewGenericEnvelope()
env.RawInput = "Analyze the codebase"
env.UserID = "user_123"

// 6. Execute
result, err := rt.Run(ctx, env, "thread_123")
```

### Using Parallel Execution

```go
// Run independent stages concurrently
result, err := rt.RunParallel(ctx, env, "thread_123")
```

### Streaming Results

```go
outputChan, err := rt.RunWithStream(ctx, env, "thread_123")
if err != nil {
    log.Fatal(err)
}

for output := range outputChan {
    if output.Stage == "__end__" {
        break
    }
    fmt.Printf("Stage %s completed: %v\n", output.Stage, output.Output)
}
```

---

## Related Documentation

- [CommBus - Communication Bus](../commbus/README.md)
- [Runtime Execution Model](runtime.md)
- [Agent System](agents.md)
- [Pipeline Configuration](config.md)
- [Envelope System](envelope.md)
- [Tool Executor](tools.md)
- [gRPC Server](grpc.md)
