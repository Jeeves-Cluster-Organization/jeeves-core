# Jeeves Core

Rust micro-kernel + Python infrastructure library for AI agent orchestration.

The kernel provides process lifecycle, IPC, interrupt handling, and resource quotas via TCP+msgpack. The Python layer (`jeeves_infra`) provides LLM providers, gateway, pipeline execution, and bootstrap — consumed by capabilities as a library.

## Quick Start

```bash
# Build kernel
cargo build --release

# Run kernel (IPC on :50051)
cargo run --release

# Install Python infrastructure
cd python && pip install -e ".[dev,all]"

# Run Python tests
cd python && pytest

# Docker (kernel + wheel)
docker build -t jeeves-core .
```

## Repository Structure

```
jeeves-core/
├── src/              # Rust kernel
├── python/           # Python infrastructure (jeeves_infra)
│   ├── jeeves_infra/ # The package
│   ├── tests/        # Python tests
│   └── pyproject.toml
├── tests/            # Rust integration tests
├── benches/          # Rust benchmarks
├── docs/             # IPC protocol, deployment, API reference
└── Dockerfile        # Multi-stage: kernel binary + Python wheel
```

## IPC Methods

| Service | RPCs | Purpose |
|---------|------|---------|
| KernelService | 14 | Process lifecycle, quotas, rate limiting |
| EngineService | 5 | Envelope and pipeline management |
| OrchestrationService | 4 | Session and instruction flow |
| CommBusService | 4 | Message bus (pub/sub/query) |

## Kernel Modules (Rust)

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

## Infrastructure Modules (Python)

| Module | Description |
|--------|-------------|
| `jeeves_infra.kernel_client` | IPC bridge to Rust kernel (TCP+msgpack) |
| `jeeves_infra.gateway` | FastAPI HTTP/WS/SSE server |
| `jeeves_infra.llm` | LLM provider abstraction (OpenAI, LiteLLM, mock) |
| `jeeves_infra.bootstrap` | AppContext creation, composition root |
| `jeeves_infra.orchestrator` | Event orchestration and governance |
| `jeeves_infra.protocols` | Type definitions and interfaces |
| `jeeves_infra.capability_wiring` | Capability registration and discovery |

## Architecture

```
Capability Layer (agents, prompts, tools)
       | imports jeeves_infra
       v
Python Infrastructure (python/jeeves_infra)
       | TCP+msgpack IPC
       v
Rust Kernel (src/)
```

## Prerequisites

- Rust 1.75+
- Python 3.11+

## License

Apache License 2.0
