# Jeeves Constitution

Architectural principles for the Rust micro-kernel.

## Purpose

Jeeves Core is the **micro-kernel** for AI agent orchestration. It provides minimal, essential primitives that all higher layers depend on.

## Core Principles

### 1. Minimal Kernel Surface

The kernel provides only:
- **Process Lifecycle** — Unix-like state machine for agent execution
- **Resource Quotas** — Defense-in-depth bounds enforcement
- **Interrupt Handling** — Human-in-the-loop patterns
- **Pipeline Orchestration** — Routing, bounds, termination decisions
- **CommBus** — Kernel-mediated inter-process communication (events, commands, queries)
- **Agent Execution** — Agent trait, LlmAgent, DeterministicAgent, PipelineAgent

The kernel does NOT provide:
- Tool implementations (capability layer)
- Prompt templates (capability layer)
- Domain-specific logic (capability layer)

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
| `edge_limits` | Per-edge traversal limit |

Bounds are enforced at the kernel level. Capabilities cannot bypass them.

### 4. Kernel-Mediated Communication

The CommBus provides three patterns:

- **Events** — Pub/sub with fan-out to all subscribers
- **Commands** — Fire-and-forget to single handler
- **Queries** — Request/response with timeout

All inter-agent communication flows through the kernel.

### 5. Process Isolation

Processes follow Unix-like principles:

```
New → Ready → Running → Blocked → Terminated → Zombie
```

- Each process has isolated resource quota
- Processes cannot directly access other processes
- All IPC is kernel-mediated

### 6. Consumer Contract

Capabilities consume the kernel via:
- **PyO3** — `from jeeves_core import PipelineRunner` (primary pattern)
- **Rust crate** — direct library import
- **MCP stdio** — JSON-RPC tool proxy (for tool aggregation)

No HTTP gateway. The kernel is a library, not a service.

### 7. Routing as Code

Routing is registered functions (`RoutingFn` trait), not JSON expression trees. Consumers register named closures on the Kernel; pipeline stages reference them by name via `routing_fn`. Static wiring (`default_next`, `error_next`) remains declarative.

### 8. Kernel is Sole Termination Authority

Only the kernel terminates pipelines via `Instruction::Terminate`. Workers execute agents and report results. Workers have no control over what runs next.

## Safety Requirements

- **Zero unsafe code** unless absolutely necessary
- **`unsafe_code = "deny"`** in lints
- **Clippy strict mode** — `unwrap_used`, `expect_used`, `panic` all warn
