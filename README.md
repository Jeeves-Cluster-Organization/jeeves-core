# Jeeves Core

Rust micro-kernel for AI agent orchestration. Provides process lifecycle, IPC, interrupt handling, and resource quotas via IPC (TCP+msgpack).

## Quick Start

```bash
# Build
cargo build --release

# Run (IPC on :50051)
cargo run --release

# Custom port
JEEVES_GRPC_PORT=50052 cargo run --release

# Tests
cargo test
```

## IPC Methods

| Service | RPCs | Purpose |
|---------|------|---------|
| KernelService | 12 | Process lifecycle (create, schedule, terminate) |
| EngineService | 6 | Envelope and pipeline management |
| OrchestrationService | 4 | Session and instruction flow |
| CommBusService | 4 | IPC (pub/sub/query) |

See `proto/` for service definitions.

## Module Structure

| Module | Description |
|--------|-------------|
| `kernel::lifecycle` | Process state machine and scheduling |
| `kernel::resources` | Quota enforcement and usage tracking |
| `kernel::interrupts` | Human-in-the-loop interrupt handling |
| `kernel::rate_limiter` | Per-user rate limiting (sliding window) |
| `kernel::orchestrator` | Pipeline orchestration (session management) |
| `kernel::cleanup` | Background garbage collection |
| `kernel::recovery` | Panic recovery for fault isolation |
| `commbus` | Message bus (events, commands, queries) |
| `envelope` | State container for pipeline execution |

## Process Lifecycle

```
New → Ready → Running → Blocked → Ready → ... → Terminated → Zombie
```

## Resource Quotas

```rust
ResourceQuota {
    max_llm_calls: usize,       // Limit LLM API calls
    max_output_tokens: usize,   // Limit token generation
    max_agent_hops: usize,      // Limit pipeline depth
    max_iterations: usize,      // Prevent infinite loops
}
```

## Interrupt Types

- **Clarification**: Agent requests user input
- **Confirmation**: Agent seeks approval to proceed
- **ResourceExhausted**: Quota exceeded, request extension

## Prerequisites

- Rust 1.70+
- Protocol Buffers compiler (`protoc`)

## License

Apache License 2.0
