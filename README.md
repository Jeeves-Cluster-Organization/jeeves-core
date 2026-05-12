# Jeeves Core

Rust micro-kernel for AI agent orchestration. Consumed as a library; no service binary, no language bindings.

See `CONSTITUTION.md` for the architectural principles and `docs/API_REFERENCE.md` for the consumer-facing API.

## Quick Start

```bash
cargo test                              # 175 lib + 23 integration tests
cargo clippy --all-features             # lint
```

## Layout

```
src/
├── kernel/      # Orchestration: pipeline sessions, routing, bounds, lifecycle, interrupts
├── worker/      # Agent execution: kernel actor, KernelHandle, Agent impls, LLM provider, tools
├── envelope/    # Envelope, bounds, TerminalReason
├── tools/       # ToolAccessPolicy, ToolCatalog, ToolHealthTracker
└── types/       # IDs, errors, config
schema/          # JSON Schema for pipeline.json
tests/           # Integration tests
```

## Consumer Pattern

```rust
use jeeves_core::prelude::*;
use jeeves_core::worker::llm::genai_provider::GenaiProvider;
use std::sync::Arc;

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

| Feature | Purpose |
|---------|---------|
| `test-harness` | Test utilities for consumer integration tests |
| `otel` | OpenTelemetry tracing layer |

## Prerequisites

Rust 1.75+.

## License

Apache 2.0.
