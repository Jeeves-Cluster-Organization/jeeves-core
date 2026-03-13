# Jeeves Core

Rust micro-kernel for AI agent orchestration. Single-process runtime with embedded HTTP gateway.

The kernel provides process lifecycle, pipeline orchestration (routing, bounds, termination), interrupt handling, resource quotas, and an HTTP API. Agent execution runs as concurrent tokio tasks within the same process.

## Quick Start

```bash
# Build and test
cargo build
cargo test                          # 252 tests
cargo clippy -- -D warnings         # lint-clean

# Run the kernel with HTTP gateway
cp .env.example .env                # edit for your env
cargo run -- run                    # HTTP on 0.0.0.0:8080

# Or specify options directly
cargo run -- run --http-addr 0.0.0.0:9000 --llm-model gpt-4o
```

If you have [just](https://github.com/casey/just) installed:

```bash
just check     # cargo check + clippy + test
just run       # start the kernel
just fmt       # cargo fmt
```

## Repository Structure

```
jeeves-core/
├── src/
│   ├── kernel/           # Orchestration engine (routing, bounds, process lifecycle)
│   ├── worker/           # Agent execution, LLM providers, HTTP gateway
│   │   ├── actor.rs      # Kernel actor (mpsc channel, typed dispatch)
│   │   ├── handle.rs     # KernelHandle (typed channel wrapper)
│   │   ├── agent.rs      # Agent trait, LlmAgent, DeterministicAgent
│   │   ├── llm/          # LLM provider trait + OpenAI-compatible client
│   │   ├── gateway.rs    # HTTP gateway (axum)
│   │   ├── tools.rs      # Tool executor trait + registry
│   │   └── prompts.rs    # Prompt template loading + rendering
│   ├── envelope/         # Envelope types, enums (TerminalReason), bounds
│   ├── commbus/          # Message bus (events, commands, queries)
│   └── main.rs           # CLI entry point (clap)
├── tests/                # Rust integration tests
├── benches/              # Rust benchmarks
└── docs/                 # Deployment, API reference
```

## HTTP API

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/v1/chat/messages` | Run a pipeline with input, return response |
| `POST` | `/api/v1/pipelines/run` | Alias for chat/messages |
| `GET` | `/api/v1/sessions/{id}` | Get orchestration session state |
| `GET` | `/api/v1/status` | System status (process counts, health) |
| `GET` | `/health` | Liveness probe (always 200) |
| `GET` | `/ready` | Readiness probe |

## Kernel Modules

| Module | Description |
|--------|-------------|
| `kernel::lifecycle` | Process state machine and scheduling |
| `kernel::resources` | Quota enforcement and usage tracking |
| `kernel::interrupts` | Human-in-the-loop interrupt handling |
| `kernel::rate_limiter` | Per-user rate limiting (sliding window) |
| `kernel::orchestrator` | Pipeline orchestration (session management, routing) |
| `kernel::cleanup` | Background garbage collection |
| `kernel::recovery` | Panic recovery for fault isolation |
| `commbus` | Message bus (events, commands, queries) |
| `envelope` | State container for pipeline execution |

## Worker Modules

| Module | Description |
|--------|-------------|
| `worker::actor` | Kernel actor — single `&mut Kernel` behind mpsc channel |
| `worker::handle` | `KernelHandle` — typed channel wrapper (Clone, Send+Sync) |
| `worker::agent` | `Agent` trait, `LlmAgent`, `DeterministicAgent`, `AgentRegistry` |
| `worker::llm` | `LlmProvider` trait, OpenAI-compatible HTTP client, mock provider |
| `worker::gateway` | Axum HTTP gateway with CORS |
| `worker::tools` | `ToolExecutor` trait + `ToolRegistry` |
| `worker::prompts` | Template loading and `{var}` substitution |

## Architecture

```
HTTP clients (frontends, capabilities)
       | HTTP (axum)
       v
┌─────────────────────────────────────────┐
│  Single process                         │
│                                         │
│  Kernel actor ← mpsc ← KernelHandle    │
│  (tokio task)   (typed)  (Clone)        │
│       │                    ↑            │
│       ▼                    │            │
│  Agent tasks (concurrent tokio tasks)   │
│       │                                 │
│       ▼                                 │
│  LLM calls (reqwest, concurrent)        │
└─────────────────────────────────────────┘
```

## Consumer Contract

Capabilities interact via HTTP API:

```bash
# Run a pipeline
curl -X POST http://localhost:8080/api/v1/chat/messages \
  -H 'Content-Type: application/json' \
  -d '{"message": "hello", "user_id": "u1", "pipeline_config": {...}}'

# Check health
curl http://localhost:8080/health
```

For capabilities compiled into the binary, use the Rust crate API:

```rust
use jeeves_core::worker::agent::{Agent, AgentContext, AgentOutput};
use jeeves_core::worker::handle::KernelHandle;
use jeeves_core::kernel::orchestrator_types::PipelineConfig;
```

## Prerequisites

- Rust 1.75+

## License

Apache License 2.0
