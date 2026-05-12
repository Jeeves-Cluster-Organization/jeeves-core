# Jeeves Core

Rust kernel for AI agent orchestration. Consumed as a library; no service binary, no language bindings.

See `CONSTITUTION.md` for architectural principles and `docs/API_REFERENCE.md` for the consumer-facing API.

## Quick Start

```bash
cargo test                              # 170 lib + 23 integration + 2 schema
cargo clippy --all-features             # lint
```

## Layout

```
src/
├── kernel/        # Kernel actor, KernelHandle, runner, orchestrator, lifecycle, interrupts
├── agent/         # Agent trait + impls (LlmAgent, ToolDelegatingAgent, Deterministic), hooks
├── run/           # Run state, identity, metrics, audit, FlowInterrupt, RunEvent stream
├── workflow/      # Workflow + Stage definition, retry/context policy, state_schema
├── tools/         # ToolRegistry, ToolAccessPolicy, ToolCatalog, ToolHealthTracker
└── types/         # IDs, errors, config
schema/            # JSON Schema for workflow JSON
tests/             # Integration tests (runner.rs, schema.rs)
```

Dependency direction is one-way: `types → {workflow, run, tools} → agent → kernel`.

## Consumer Pattern

```rust
use jeeves_core::prelude::*;
use std::sync::Arc;

let llm: Arc<dyn LlmProvider> = /* your LlmProvider impl */;
let prompts = Arc::new(PromptRegistry::from_dir("prompts/"));
let tools = ToolRegistryBuilder::new().add_executor(my_tools).build();

let mut kernel = Kernel::new();
kernel.register_routing_fn("router", Arc::new(|ctx: &RoutingContext<'_>| {
    RoutingResult::Next("next_stage".into())
}));

let cancel = tokio_util::sync::CancellationToken::new();
let handle = jeeves_core::kernel::actor::spawn(kernel, cancel);

let agents = AgentFactoryBuilder::new(llm, prompts, tools)
    .add_pipeline(workflow.clone()).build();

let run = Run::new("user1", "session1", "hello", None);
let result = run(&handle, RunId::new(), workflow, run, &agents).await?;
```

For streaming events use `run_streaming` (returns `mpsc::Receiver<RunEvent>`).

## Feature Flags

| Feature | Purpose |
|---------|---------|
| `test-harness` | Test utilities for consumer integration tests |
| `otel` | OpenTelemetry tracing layer |

## Prerequisites

Rust 1.75+.

## License

Apache 2.0.
