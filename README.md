# Jeeves Core

A high-performance micro-kernel for AI agent orchestration written in Rust.

[![Rust](https://img.shields.io/badge/Rust-1.70+-orange?logo=rust&logoColor=white)](https://rust-lang.org)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/Tests-96%20passing-brightgreen)](.)
[![Coverage](https://img.shields.io/badge/Coverage-81.36%25-green)](COVERAGE_REPORT.md)

## Overview

Jeeves Core is a **production-grade micro-kernel** that provides the foundational runtime for AI agent systems. Built in Rust for safety, performance, and reliability.

### Key Features

- **Process Lifecycle Management** - Unix-like process states with resource quotas
- **Kernel-Mediated IPC** - CommBus for secure inter-agent communication
- **Interrupt Handling** - Human-in-the-loop patterns with timeout enforcement
- **Resource Quotas** - Defense-in-depth limits on LLM calls, tokens, and execution hops
- **gRPC Services** - High-performance communication with infrastructure layer
- **Background Cleanup** - Automatic garbage collection of stale processes
- **Panic Recovery** - Fault isolation prevents single-agent failures from crashing the kernel

## Architecture

\`\`\`
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
│  │  Lifecycle   │  │  Interrupts  │  │   CommBus    │          │
│  │  Manager     │  │   Service    │  │ (IPC/Pub-Sub)│          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │  Resources   │  │ Rate Limiter │  │    gRPC      │          │
│  │  Tracker     │  │              │  │  (Services)  │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │ Orchestrator │  │   Cleanup    │  │   Recovery   │          │
│  │  (Pipeline)  │  │   Service    │  │   (Panics)   │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
└─────────────────────────────────────────────────────────────────┘
\`\`\`

## Installation

### Prerequisites

- Rust 1.70 or later
- Protocol Buffers compiler (\`protoc\`)

### Build from Source

\`\`\`bash
git clone https://github.com/Jeeves-Cluster-Organization/jeeves-core.git
cd jeeves-core
cargo build --release
\`\`\`

### Run Tests

\`\`\`bash
# Run all tests
cargo test

# Run with coverage report
cargo tarpaulin --lib --out Stdout

# Run specific module tests
cargo test --lib kernel::lifecycle
\`\`\`

## Quick Start

### Running the Kernel

\`\`\`bash
# Start the gRPC server (default port 50051)
cargo run --release

# With custom configuration
JEEVES_GRPC_PORT=50052 cargo run --release
\`\`\`

### Using as a Library

Add to your \`Cargo.toml\`:

\`\`\`toml
[dependencies]
jeeves-core = { git = "https://github.com/Jeeves-Cluster-Organization/jeeves-core" }
tokio = { version = "1", features = ["full"] }
\`\`\`

Example usage:

\`\`\`rust
use jeeves_core::kernel::{Kernel, SchedulingPriority, ResourceQuota};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // Create kernel with default quotas
    let kernel = Arc::new(Mutex::new(Kernel::new()));

    // Create a process with resource quotas
    let quota = ResourceQuota {
        max_llm_calls: 10,
        max_output_tokens: 50000,
        max_agent_hops: 20,
        max_iterations: 5,
    };

    let mut k = kernel.lock().await;
    let process = k.create_process(
        "process-123".to_string(),
        "request-456".to_string(),
        "user-789".to_string(),
        "session-abc".to_string(),
        SchedulingPriority::Normal,
        Some(quota),
    ).expect("Failed to create process");

    println!("Created process: {}", process.pid);
}
\`\`\`

## Core Concepts

### Process Lifecycle

Processes follow Unix-like state transitions:

\`\`\`
New → Ready → Running → Blocked → Ready → ... → Terminated → Zombie
\`\`\`

- **New**: Process created but not scheduled
- **Ready**: Waiting in priority queue for CPU
- **Running**: Currently executing
- **Blocked**: Waiting for interrupt resolution
- **Terminated**: Execution complete
- **Zombie**: Awaiting cleanup

### Resource Quotas

Defense-in-depth resource limits:

\`\`\`rust
pub struct ResourceQuota {
    pub max_llm_calls: usize,       // Limit LLM API calls
    pub max_output_tokens: usize,   // Limit token generation
    pub max_agent_hops: usize,      // Limit pipeline depth
    pub max_iterations: usize,      // Prevent infinite loops
}
\`\`\`

### Interrupts (Human-in-the-Loop)

Three interrupt types for human interaction:

- **Clarification**: Agent requests user input
- **Confirmation**: Agent seeks approval to proceed
- **ResourceExhausted**: Quota exceeded, request extension

\`\`\`rust
// Create an interrupt
let interrupt = k.create_interrupt(
    "process-123".to_string(),
    InterruptKind::Clarification,
    "Need input: Which file should I modify?".to_string(),
    "user-789".to_string(),
)?;

// Resolve with user response
k.resolve_interrupt(&interrupt.id, "user-789", true, Some("auth.py"))?;
\`\`\`

### CommBus (IPC)

Kernel-mediated inter-process communication:

\`\`\`rust
// Event pub/sub (fan-out)
let (subscription, mut rx) = k.commbus
    .subscribe("agent-123".to_string(), vec!["system.event".to_string()])
    .await?;

// Command (fire-and-forget)
k.commbus.send_command(Command {
    command_type: "agent.restart".to_string(),
    payload: b"{}".to_vec(),
    source: "supervisor".to_string(),
}).await?;

// Query (request-response with timeout)
let response = k.commbus.query(Query {
    query_type: "agent.status".to_string(),
    payload: b"{}".to_vec(),
    timeout_ms: 1000,
    source: "monitor".to_string(),
}).await?;
\`\`\`

## Module Structure

| Module | Description |
|--------|-------------|
| \`kernel::lifecycle\` | Process state machine and scheduling |
| \`kernel::resources\` | Quota enforcement and usage tracking |
| \`kernel::interrupts\` | Human-in-the-loop interrupt handling |
| \`kernel::rate_limiter\` | Per-user rate limiting (sliding window) |
| \`kernel::services\` | Service registry for agent dispatch |
| \`kernel::orchestrator\` | Pipeline orchestration (session management) |
| \`kernel::cleanup\` | Background garbage collection |
| \`kernel::recovery\` | Panic recovery for fault isolation |
| \`commbus\` | Message bus (events, commands, queries) |
| \`envelope\` | State container for pipeline execution |
| \`grpc\` | gRPC service implementations |
| \`types\` | Error types and core data structures |

## gRPC Services

The kernel exposes four gRPC services:

1. **KernelService** (12 RPCs): Process lifecycle operations
2. **EngineService** (6 RPCs): Envelope and pipeline management
3. **OrchestrationService** (4 RPCs): Session and instruction flow
4. **CommBusService** (4 RPCs): IPC operations (pub/sub/query)

See \`proto/\` directory for service definitions.

## Testing

The codebase has **81.36% line coverage** with **96 comprehensive tests**.

\`\`\`bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific module
cargo test kernel::lifecycle::tests

# Run with coverage
cargo tarpaulin --lib --out Stdout
\`\`\`

See [COVERAGE_REPORT.md](COVERAGE_REPORT.md) for detailed coverage analysis.

## Production Features

### Background Cleanup

Automatic garbage collection of stale processes:

\`\`\`rust
use jeeves_core::kernel::cleanup::{CleanupService, CleanupConfig};

let config = CleanupConfig {
    interval_seconds: 300,              // Run every 5 minutes
    process_retention_seconds: 86400,   // Keep zombies for 24 hours
    session_retention_seconds: 3600,    // Keep sessions for 1 hour
    interrupt_retention_seconds: 86400, // Keep interrupts for 24 hours
};

let mut cleanup = CleanupService::new(kernel.clone(), config);
let handle = cleanup.start(); // Spawns background task
\`\`\`

### Panic Recovery

Prevent single-agent failures from crashing the kernel:

\`\`\`rust
use jeeves_core::kernel::with_recovery;

let result = with_recovery(|| {
    // Potentially panicking agent operation
    execute_agent()?;
    Ok(())
}, "agent_execution");

match result {
    Ok(_) => println!("Success"),
    Err(e) => eprintln!("Agent panicked: {}", e),
}
\`\`\`

## Performance

- **Zero-copy message passing** via Arc and tokio channels
- **Compile-time memory safety** (no garbage collector pauses)
- **Lock-free scheduling** with priority queues
- **Efficient resource tracking** with HashMap lookups

## Safety Guarantees

- **Zero unsafe code blocks** in the entire codebase
- **Compile-time null safety** (no null pointer dereferences)
- **Thread safety** enforced by the borrow checker
- **Panic recovery** prevents cascading failures

## Related Projects

- [jeeves-infra](https://github.com/Jeeves-Cluster-Organization/jeeves-infra) - Python infrastructure layer
- [mini-swe-agent](https://github.com/Jeeves-Cluster-Organization/mini-swe-agent) - Software engineering capability

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

1. Fork the repository
2. Create a feature branch (\`git checkout -b feature/amazing-feature\`)
3. Write tests for your changes
4. Ensure \`cargo test\` passes
5. Ensure \`cargo fmt\` and \`cargo clippy\` are clean
6. Push to your fork and open a Pull Request

## License

Apache License 2.0 - see [LICENSE](LICENSE) for details.

\`\`\`
Copyright 2024-2026 Jeeves Cluster Organization

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
\`\`\`
