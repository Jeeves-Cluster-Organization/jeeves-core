# Jeeves Core

A micro-kernel for AI agent orchestration written in Go.

[![Go](https://img.shields.io/badge/Go-1.21+-00ADD8?logo=go&logoColor=white)](https://go.dev)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/Tests-400%2B%20passing-brightgreen)](.)

## Overview

Jeeves Core is a **micro-kernel** that provides the foundational runtime for AI agent systems. It handles:

- **Pipeline Orchestration** - Multi-stage agent pipelines with routing rules
- **Envelope State Management** - Immutable state transitions with bounds checking
- **Resource Quotas** - Defense-in-depth limits on iterations, LLM calls, and agent hops
- **gRPC Services** - High-performance communication with Python infrastructure layer
- **Circuit Breakers** - Fault tolerance for external service calls

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Capabilities (User Space)                                       │
│  mini-swe-agent, chat-agent, etc.                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│  jeeves-infra (Infrastructure Layer)                            │
│  LLM providers, database clients, HTTP gateway                  │
└─────────────────────────────────────────────────────────────────┘
                              │ gRPC
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│  jeeves-core (Micro-Kernel)  ← THIS PACKAGE                     │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │   Runtime    │  │   Envelope   │  │   CommBus    │          │
│  │  (Pipeline)  │  │   (State)    │  │  (Messaging) │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │    Config    │  │    Tools     │  │    gRPC      │          │
│  │  (Pipeline)  │  │  (Registry)  │  │  (Services)  │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
└─────────────────────────────────────────────────────────────────┘
```

## Installation

```bash
go get github.com/Jeeves-Cluster-Organization/jeeves-core
```

Or clone and build:

```bash
git clone https://github.com/Jeeves-Cluster-Organization/jeeves-core.git
cd jeeves-core
go build ./...
```

## Quick Start

### Running the Kernel

```bash
# Start the gRPC server
go run ./cmd -addr :50051

# With metrics enabled
go run ./cmd -addr :50051 -metrics-addr :9091
```

### Using as a Library

```go
package main

import (
    "github.com/Jeeves-Cluster-Organization/jeeves-core/coreengine/config"
    "github.com/Jeeves-Cluster-Organization/jeeves-core/coreengine/runtime"
    "github.com/Jeeves-Cluster-Organization/jeeves-core/coreengine/envelope"
)

func main() {
    // Create pipeline configuration
    cfg := &config.PipelineConfig{
        Name:          "my_pipeline",
        MaxIterations: 10,
        MaxLLMCalls:   5,
        MaxAgentHops:  20,
        Agents: []config.AgentConfig{
            {
                Name:        "analyzer",
                OutputKey:   "analysis",
                HasLLM:      true,
                HasTools:    true,
                DefaultNext: "executor",
            },
            {
                Name:        "executor",
                OutputKey:   "result",
                HasLLM:      true,
                HasTools:    true,
                DefaultNext: "end",
            },
        },
    }

    // Create runtime
    rt := runtime.NewRuntime(cfg)

    // Create envelope with task
    env := envelope.NewEnvelope("task-123", "Fix the bug in auth.py")

    // Execute pipeline (integrate with jeeves-infra for LLM calls)
    // ...
}
```

## Core Concepts

### Envelope

The `Envelope` is the immutable state container that flows through the pipeline:

```go
type Envelope struct {
    EnvelopeID    string
    Task          string
    CurrentStage  string
    Outputs       map[string]interface{}
    IterationCount int
    LLMCallCount   int
    AgentHopCount  int
}
```

### Pipeline Configuration

Pipelines are configured declaratively with agents, routing rules, and bounds:

```go
type PipelineConfig struct {
    Name          string
    MaxIterations int
    MaxLLMCalls   int
    MaxAgentHops  int
    Agents        []AgentConfig
}

type AgentConfig struct {
    Name         string
    OutputKey    string
    HasLLM       bool
    HasTools     bool
    DefaultNext  string
    RoutingRules []RoutingRule
}
```

### Circuit Breakers

Built-in fault tolerance for external service calls:

```go
cb := commbus.NewCircuitBreaker(commbus.CircuitBreakerConfig{
    Threshold:   5,
    ResetPeriod: 30 * time.Second,
})
```

## Testing

```bash
# Run all tests
go test ./...

# With coverage
go test ./... -cover

# Verbose output
go test ./... -v

# Specific package
go test ./coreengine/runtime -v
```

## Package Structure

| Package | Description |
|---------|-------------|
| `coreengine/runtime` | Pipeline execution engine |
| `coreengine/envelope` | State container and transitions |
| `coreengine/config` | Pipeline configuration types |
| `coreengine/commbus` | Message bus with circuit breakers |
| `coreengine/tools` | Tool registry and definitions |
| `coreengine/grpc` | gRPC service implementations |
| `coreengine/testutil` | Test helpers and fixtures |

## Metrics

Prometheus metrics are exposed when running with `-metrics-addr`:

```promql
# Pipeline execution rate
rate(jeeves_pipeline_executions_total[5m])

# Agent latency (P95)
histogram_quantile(0.95, rate(jeeves_agent_duration_seconds_bucket[5m]))
```

## Related Projects

- [jeeves-infra](https://github.com/Jeeves-Cluster-Organization/jeeves-infra) - Python infrastructure layer
- [mini-swe-agent](https://github.com/Jeeves-Cluster-Organization/mini-swe-agent) - Software engineering capability

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

Apache License 2.0 - see [LICENSE](LICENSE) for details.

```
Copyright 2024 Jeeves Cluster Organization

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
