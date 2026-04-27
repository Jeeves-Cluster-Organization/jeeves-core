# Jeeves Core

Rust micro-kernel for AI agent orchestration. Consumed as a Rust library — no Python bindings, no service binary.

## Quick Start

```bash
cargo test                              # 242 lib + 23 integration tests
cargo clippy --all-features             # lint
```

## Repository Structure

```
jeeves-core/
├── src/
│   ├── kernel/               # Orchestration engine
│   │   ├── orchestrator.rs   # Pipeline sessions, routing, bounds
│   │   ├── orchestrator_types.rs  # PipelineConfig, PipelineStage, Instruction
│   │   ├── routing.rs        # RoutingFn trait, RoutingRegistry, dispatch
│   │   ├── lifecycle.rs      # Process state machine
│   │   ├── resources.rs      # Quota enforcement
│   │   └── interrupts.rs     # Tool confirmation gate
│   ├── worker/               # Agent execution
│   │   ├── actor.rs          # Kernel actor (mpsc channel, typed dispatch)
│   │   ├── handle.rs         # KernelHandle (typed channel wrapper, Clone)
│   │   ├── agent.rs          # Agent trait, LlmAgent, DeterministicAgent
│   │   ├── agent_factory.rs  # AgentFactoryBuilder (agent auto-creation)
│   │   ├── llm/              # LlmProvider trait, GenaiProvider
│   │   ├── tools.rs          # ToolExecutor trait + ToolRegistry
│   │   └── prompts.rs        # Prompt template loading + {var} substitution
│   ├── envelope/             # Envelope types, bounds, TerminalReason
│   └── types/                # IDs, errors, config
├── schema/                   # JSON Schema for pipeline.json
└── tests/                    # Integration tests
```

## Consumer Pattern

```rust
use jeeves_core::prelude::*;
use jeeves_core::worker::llm::genai_provider::GenaiProvider;

let llm: Arc<dyn LlmProvider> = Arc::new(GenaiProvider::new("qwen3-14b"));
let prompts = Arc::new(PromptRegistry::from_dir("prompts/"));
let tools = ToolRegistryBuilder::new().add_executor(my_tools).build();

let mut kernel = Kernel::new();
kernel.register_routing_fn("router", Arc::new(|ctx: &RoutingContext<'_>| {
    RoutingResult::Next("next_stage".into())
}));

let cancel = tokio_util::sync::CancellationToken::new();
let handle = jeeves_core::worker::actor::spawn_kernel(kernel, cancel);

let agents = AgentFactoryBuilder::new(llm, prompts, tools)
    .add_pipeline(config.clone()).build();

let envelope = Envelope::new_minimal("user1", "session1", "hello", None);
let result = run_pipeline_with_envelope(&handle, ProcessId::new(), config, envelope, &agents).await?;
```

## Feature Flags

| Feature | Dependencies | Purpose |
|---------|-------------|---------|
| `test-harness` | — | Test utilities for integration tests |
| `otel` | opentelemetry, tracing-opentelemetry | OpenTelemetry observability bridge |

## Architecture

```
Rust capability crate
       │ direct fn call
       ▼
┌──────────────────────────────────────────────┐
│  Kernel actor ← mpsc ← KernelHandle         │
│  (tokio task)   (typed)  (Clone, Send+Sync)  │
│       │                    ↑                 │
│       ▼                    │                 │
│  Agent tasks (concurrent tokio tasks)        │
│  ├── LlmAgent (genai HTTP)                   │
│  ├── McpDelegatingAgent (tool dispatch)      │
│  └── DeterministicAgent (passthrough)        │
└──────────────────────────────────────────────┘
```

## Prerequisites

- Rust 1.75+

## License

Apache License 2.0
