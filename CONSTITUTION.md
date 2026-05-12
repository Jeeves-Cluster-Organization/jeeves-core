# Jeeves Constitution

Architectural principles for the Rust micro-kernel.

## Purpose

Jeeves Core is the **micro-kernel** for AI agent orchestration. It provides minimal, essential primitives for pipeline-driven multi-agent execution.

## Core Principles

### 1. Minimal Kernel Surface

The kernel provides only:
- **Workflow Orchestration** — declarative stages, routing functions, default/error transitions, termination decisions
- **Resource Quotas** — defense-in-depth bounds on iterations, LLM calls, agent hops, per-stage visits, per-stage context tokens
- **Run Lifecycle** — slim state machine for agent execution
- **Tool Confirmation** — interrupt-and-resume gate for destructive tool calls
- **Agent Execution** — `Agent` trait, `LlmAgent` (with ReAct tool loop + hooks), `ToolDelegatingAgent`, `DeterministicAgent`
- **Tool Policy Chain** — optional `ToolAccessPolicy` (agent×tool ACL), `ToolCatalog` (typed param validation), `ToolHealthTracker` (sliding-window metrics + circuit breaker), all opt-in via `ToolRegistryBuilder`
- **Streaming Events** — `mpsc::Receiver<RunEvent>` channel for token deltas, stage lifecycle, tool calls, routing decisions

The kernel does NOT provide:
- Pub/sub, command/query buses, or cross-workflow federation
- Workflow checkpoints, durable resume, background cleanup tickers
- Per-user rate limiting or service registries
- MCP transports (stdio/HTTP) — consumers wire `ToolExecutor` directly
- Language bindings (PyO3, FFI) — Rust crate is the only consumption surface
- Domain-specific tools or prompt templates (capability layer)
- Fan-out / fork-join routing — `RoutingResult` is `Next` or `Terminate`; consumers compose pipelines linearly with conditional routing

### 2. No Backward Compatibility

Clean break. No serde aliases, no dual fields, no shims. If something changes, it changes everywhere. Consumers are migrated, not accommodated.

### 3. Defense in Depth

All execution is bounded:

| Bound | Purpose |
|-------|---------|
| `max_iterations` | Prevent infinite agent loops |
| `max_llm_calls` | Control LLM API costs |
| `max_agent_hops` | Limit pipeline depth |
| `max_visits` | Per-stage visit limit |

Bounds are enforced at the kernel level. Capabilities cannot bypass them.

### 4. Routing as Code

Routing is registered functions (`RoutingFn` trait), not JSON expression trees. Consumers register named closures on the Kernel; pipeline stages reference them by name via `routing_fn`. Static wiring (`default_next`, `error_next`) remains declarative.

Evaluation order per stage:
1. Agent failed AND `error_next` set → `error_next`
2. `routing_fn` registered → call it; use `RoutingResult::Next` or `Terminate`
3. `default_next` → next stage
4. None of the above → terminate (Completed)

### 5. Kernel is Sole Termination Authority

Only the kernel terminates pipelines via `Instruction::Terminate`. Workers execute agents and report results. Workers have no control over what runs next.

### 6. Single-Actor Kernel

All kernel state lives behind one mpsc channel (`KernelHandle` → `Kernel`). Zero locks; sequential message processing. Agent tasks run as concurrent tokio tasks and communicate with the kernel only via typed `KernelCommand` messages.

### 7. Consumer Contract

Capabilities consume the kernel as a Rust crate:

```rust
use jeeves_core::prelude::*;
```

There is no service binary, no HTTP gateway, no Python module. The kernel is a library.

## Safety Requirements

- **Zero unsafe code** — `unsafe_code = "deny"` in lints
- **Clippy strict mode** — `unwrap_used`, `expect_used`, `panic` all warn
