# API Reference

`jeeves-core` is a Rust library. Consumers depend on it directly — there is no service binary, no HTTP gateway, no Python module.

```rust
use jeeves_core::prelude::*;
```

---

## Workflow

A workflow is a `Workflow` value (constructed in Rust or deserialized from JSON). The JSON schema in `schema/pipeline.schema.json` matches the Rust types.

### Workflow

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Workflow name. Used for event attribution. |
| `stages` | `[Stage]` | yes | Ordered list of stages. First stage is the entry point. |
| `max_iterations` | int | yes | Global iteration bound. Terminates with `MaxIterationsExceeded`. |
| `max_llm_calls` | int | yes | Global LLM-call bound across all stages. |
| `max_agent_hops` | int | yes | Bound on transitions between stages. |
| `state_schema` | `[StateField]` | no | Typed state fields with merge strategies for loop-back accumulation. |

### Stage

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | string | — | Stage identifier (unique within the workflow). |
| `agent` | string | — | Agent name to dispatch. |
| `routing_fn` | string | null | Name of a registered `RoutingFn` called after agent completion. |
| `default_next` | string | null | Fallback target when `routing_fn` is unset or returns `Terminate`. |
| `error_next` | string | null | Target when the agent fails (checked before `routing_fn`). |
| `max_visits` | int | null | Per-stage visit cap. Terminates with `MaxStageVisitsExceeded`. |
| `response_format` | object | null | Verbatim hint forwarded to the LLM provider for grammar-constrained generation. The kernel does not interpret it — consumers parse agent outputs with `serde::Deserialize` on their own typed structs. |
| `output_key` | string | null | State-field key for this stage's output (defaults to stage name). |
| `max_context_tokens` | int | null | Estimated-token cap on LLM context (chars/4 heuristic). |
| `context_overflow` | enum | `Fail` | `Fail` or `TruncateOldest` when context exceeds the cap. |
| `timeout_seconds` | int | null | Wall-clock cancellation deadline for agent execution. |
| `retry_policy` | `RetryPolicy` | null | Retry-with-backoff for transient agent failures. |
| `has_llm` | bool | `false` | Whether this stage's agent calls an LLM (in `agent_config`). |
| `prompt_key` | string | null | Prompt template key for LLM agents. |
| `temperature` | float | null | LLM temperature. |
| `max_tokens` | int | null | LLM max output tokens. |
| `model_role` | string | null | Model role override. |

### StateField & MergeStrategy

`state_schema` controls how stage outputs merge into `run.state` across workflow iterations:

| Strategy | Behavior |
|---|---|
| `Replace` | Overwrite (default). |
| `Append` | Append to an array (creates the array if missing). |
| `MergeDict` | Shallow-merge object keys. |

```json
"state_schema": [
  {"key": "retrieved_docs", "merge": "Append"},
  {"key": "context", "merge": "MergeDict"}
]
```

---

## Routing

Routing is code, not data. Consumers register named closures on the `Kernel` before spawning; stages reference them by name via `routing_fn`. Static wiring (`default_next`, `error_next`) is declarative on the stage.

### RoutingContext

Read-only view of run state passed to a routing function:

- `current_stage`, `agent_name`, `agent_failed`
- `outputs` — `agent_name → { key → value }`
- `metadata` — run metadata
- `state` — accumulated state across iterations
- `interrupt_response` — resolved interrupt response, if any

### RoutingResult

| Variant | Description |
|---|---|
| `Next(String)` | Route to the named target stage. |
| `Terminate` | End the workflow (`COMPLETED`). |

### Evaluation order

1. Agent failed AND `error_next` set → `error_next`.
2. `routing_fn` registered → call it.
3. `default_next` → route there.
4. None of the above → terminate `COMPLETED`.

---

## TerminalReason

`#[non_exhaustive]` — match exhaustively against current variants but expect new ones in future versions.

Current variants: `Completed`, `BreakRequested`, `MaxIterationsExceeded`, `MaxLlmCallsExceeded`, `MaxAgentHopsExceeded`, `UserCancelled`, `ToolFailedFatally`, `LlmFailedFatally`, `PolicyViolation`, `MaxStageVisitsExceeded`.

---

## Per-stage stream modes

Each workflow stage picks one of two LLM stream modes via the presence of `response_format`:

| `response_format` | LLM behavior | `Delta` tokens carry | Output lands at |
|---|---|---|---|
| `None` | Free-form generation | **prose tokens** | `outputs[stage]["response"]` (raw text) |
| `Some(schema)` | Grammar-constrained JSON | **JSON tokens** | `outputs[stage][field]` per the schema (parsed into the output map automatically) |

Both modes coexist within a single workflow — pick per stage based on what consumes the output. Prose mode is the right choice when the stage's output is shown to the user directly. JSON mode is the right choice when the next stage or post-run mutation code reads structured fields.

**Display policy belongs to the consumer.** The kernel emits `Delta` events for every stage that runs an LLM, regardless of mode. The consumer decides per stage whether to hide the deltas, render a status label while the stage runs, or stream the deltas into a UI text field. There is no kernel concept of "user-visible stage" — that's a UX decision that lives in the consumer.

## Streaming Events

`run_streaming` returns `(JoinHandle<Result<WorkerResult>>, mpsc::Receiver<RunEvent>)`. Consumers drain the receiver for real-time events.

`RunEvent` is `#[non_exhaustive]` with these variants (current set):

| Variant | Payload | When |
|---|---|---|
| `StageStarted` | `stage`, `pipeline` | Right before an agent starts executing a stage. |
| `Delta` | `content`, `stage?`, `pipeline` | Incremental LLM token chunk. `stage = None` for ad-hoc calls. |
| `ToolCallStart` | `id`, `name`, `stage?`, `pipeline` | LLM emitted a tool call; before execution. |
| `ToolResult` | `id`, `content`, `stage?`, `pipeline` | Tool produced a result. Matches a prior `ToolCallStart` by `id`. |
| `StageCompleted` | `stage`, `pipeline`, `metrics?` | Stage finished (success or recovered failure). |
| `RoutingDecision` | `from_stage`, `to_stage?`, `reason`, `pipeline` | Kernel selected the next stage (or terminated, `to_stage = None`). |
| `InterruptPending` | `run_id`, `interrupt_id`, `kind`, `question?`, `message?`, `pipeline` | A tool-confirmation gate is open. |
| `Error` | `message`, `stage?`, `pipeline` | Error emitted out-of-band; workflow may continue if `error_next` is set. |
| `Done` | `run_id`, `terminated`, `terminal_reason?`, `outputs?`, `pipeline`, `aggregate_metrics?` | Workflow reached a terminal state. Always last. |

(The `pipeline` field name is retained on events for wire compatibility with consumer event readers; it carries the workflow name.)

### Ordering guarantees

- `StageStarted("X")` precedes any `Delta { stage: Some("X"), .. }`, `ToolCallStart { stage: Some("X"), .. }`, or `StageCompleted("X")`.
- Every `ToolResult { id, .. }` follows the `ToolCallStart` with the same `id`.
- `Done` is terminal — no further events follow it on the channel.
- `RoutingDecision` is emitted after `StageCompleted` and before the next `StageStarted` (or before `Done`).

---

## Rust Crate API

### Key types

| Type | Module | Purpose |
|---|---|---|
| `Kernel` | `kernel` | Run manager + orchestrator (owned, not shared). |
| `KernelHandle` | `kernel::handle` | Typed mpsc channel to the kernel actor (`Clone + Send + Sync`). |
| `Workflow` | `workflow` | Workflow definition (stages + global bounds). |
| `Stage` | `workflow` | Stage definition. |
| `Run` | `run` | Per-request mutable state (raw_input, outputs, state, metadata, metrics, audit). |
| `RunRecord` | `kernel` | Per-run kernel-side bookkeeping (lifecycle, quota, started_at). |
| `RunSnapshot` | `kernel::protocol` | Serializable session-state snapshot returned by `KernelHandle::get_session_state`. |
| `Instruction` | `kernel::protocol` | Kernel→runner command (`#[non_exhaustive]`). |
| `Agent` | `agent` | Agent trait. |
| `AgentContext` | `agent` | Execution context passed to agents. |
| `LlmAgent` | `agent` | LLM agent with ReAct tool loop + hooks. |
| `ToolDelegatingAgent` | `agent` | Single-tool dispatcher. |
| `DeterministicAgent` | `agent` | Passthrough / no-op agent. |
| `AgentRegistry` | `agent` | `name → Arc<dyn Agent>` map. |
| `AgentFactoryBuilder` | `agent::factory` | Auto-creates agents from a `Workflow`. |
| `RunEvent` | `agent::llm` | Streaming event variants (see above). |
| `LlmProvider` | `agent::llm` | Trait for LLM HTTP backends. |
| `PromptRegistry` | `agent::prompts` | Prompt template loader (`{var}` substitution). |
| `ToolExecutor` | `tools` | Tool implementation trait. |
| `ToolRegistry` | `tools` | Composed tool registry with optional ACL / catalog / health gates. |
| `ToolRegistryBuilder` | `tools` | Composable builder for `ToolRegistry`. |
| `ToolAccessPolicy` | `tools::access` | Agent×tool ACL consulted by `ToolRegistry::execute_for`. |
| `ToolCatalog` | `tools::catalog` | Typed `ParamDef` metadata + parameter validation. |
| `ToolHealthTracker` | `tools::health` | Sliding-window metrics + circuit breaker per tool. |
| `LlmAgentHook` | `agent::hooks` | Pluggable lifecycle hook around the ReAct loop. |
| `FlowInterrupt` | `run` | Tool-confirmation gate request. |
| `InterruptService` | `kernel::interrupts` | Pending-interrupt bookkeeping inside the kernel. |
| `RunId` | `types` | Strongly-typed run identifier. |
| `Error` | `types` | Kernel error enum (`#[non_exhaustive]`). |

### Driving a workflow

`kernel::runner` exposes three entry points:

| Function | Use |
|---|---|
| `run(&handle, run_id, workflow, run, &agents)` | Synchronous run to completion. Returns `WorkerResult`. |
| `run_streaming(handle, run_id, workflow, run, agents)` | Async streaming. Returns `(JoinHandle, mpsc::Receiver<RunEvent>)`. |
| `run_loop(&handle, &run_id, &agents, event_tx, workflow_name)` | Drive an already-initialized session. Used internally; rarely consumer-facing. |

### Agent auto-creation (AgentFactoryBuilder)

Given a `Workflow`, agents are created per stage:

| Stage condition | Agent type |
|---|---|
| `has_llm: true` | `LlmAgent` |
| `has_llm: false` AND a tool with the same name exists | `ToolDelegatingAgent` |
| `has_llm: false` AND no matching tool | `DeterministicAgent` (passthrough) |

---

## ToolRegistry — policy / catalog / health gates

`ToolRegistry::execute_for(agent_name, tool_name, params)` traverses an optional chain in this order:

1. **`ToolAccessPolicy`** (if attached) — denies tools the agent has not been granted; returns `Error::policy_violation`. Default-deny.
2. **`ToolCatalog`** (if attached AND the tool is in the catalog) — validates `params` against the tool's `ParamDef`s; returns `Error::validation`.
3. **`ToolHealthTracker`** (if attached) — short-circuits with `Error::policy_violation` when the breaker is open.
4. Executes the tool, recording `(success, latency_ms, error_code)` into the health tracker.

The same `ToolAccessPolicy` is also consulted by `AgentFactoryBuilder` at agent-construction time — each agent's tool registry is wrapped to expose only the tools its grants permit (so the LLM never sees forbidden tool defs in its prompt). One policy, two enforcement points. Without a policy attached, the strict default applies: agents get zero tools.

Attach via builder:

```rust
let policy = Arc::new(ToolAccessPolicy::new()); // .grant("agent", "tool")
let catalog = Arc::new(ToolCatalog::new());     // .register(ToolEntry { .. })
let health = Arc::new(RwLock::new(ToolHealthTracker::new(HealthConfig::default())));

let tools = ToolRegistryBuilder::new()
    .add_executor(my_tools)
    .with_access_policy(policy)
    .with_catalog(catalog)
    .with_health_tracker(health)
    .build();
```

All three are independently optional — passing none preserves the original execute path.

---

## LlmAgent hooks

`LlmAgentHook` (in `agent::hooks`) is a single trait with three default-impl methods. Hooks register on `LlmAgent.hooks` (via `AgentFactoryBuilder::with_hook(...)` or direct mutation) and run in registration order.

| Method | When | Return / effect |
|---|---|---|
| `before_llm_call(messages: &mut Vec<ChatMessage>)` | Before each LLM call. | Mutate the message window (compaction, redaction, prompt-injection defense). |
| `before_tool_call(call: &ToolCall) -> HookDecision` | After the LLM emits a tool call, before execution. | `Continue` (run normally), `Replace(ToolOutput)` (skip executor; deliver canned result), `Block { reason }` (skip executor; deliver error). First non-`Continue` decision wins. |
| `after_tool_call(call, output: &mut ToolOutput)` | After a tool result is produced (real or via `HookDecision::Replace`). | Mutate the output (redact, decorate). Skipped for `Block`. |

---

## Consumer pattern

```rust
use jeeves_core::prelude::*;
use std::sync::Arc;

let llm: Arc<dyn LlmProvider> = /* your LlmProvider impl */;
let prompts = Arc::new(PromptRegistry::from_dir("prompts/"));
let tools = ToolRegistryBuilder::new()
    .add_executor(Arc::new(MyTools::new()))
    .build();

let mut kernel = Kernel::new();
kernel.register_routing_fn("router", Arc::new(|ctx: &RoutingContext<'_>| {
    RoutingResult::Next("next_stage".into())
}));

let cancel = tokio_util::sync::CancellationToken::new();
let handle = jeeves_core::kernel::actor::spawn(kernel, cancel);

let agents = AgentFactoryBuilder::new(llm, prompts, tools)
    .add_pipeline(workflow.clone())
    .build();

let run = Run::new("user1", "session1", "hello", None);
let result = run(&handle, RunId::new(), workflow, run, &agents).await?;
```

For streaming:

```rust
let (join, mut rx) = run_streaming(
    handle, RunId::new(), workflow, run, agents,
).await?;
while let Some(event) = rx.recv().await {
    match event {
        RunEvent::Delta { content, .. } => print!("{content}"),
        RunEvent::Done { terminal_reason, .. } => break,
        _ => {}
    }
}
let _ = join.await?;
```

---

## Feature flags

| Feature | Purpose |
|---|---|
| `test-harness` | Test utilities for consumer integration tests. |
| `otel` | OpenTelemetry tracing layer (`opentelemetry`, `tracing-opentelemetry`). |

---

## Test references

| Test location | What it covers |
|---|---|
| `src/kernel/mod.rs` | `MergeStrategy` (Replace / Append / MergeDict), lifecycle, user usage. |
| `src/kernel/routing.rs` | RoutingFn registry + evaluation order. |
| `src/kernel/orchestrator.rs` | Routing, `max_visits`, `error_next`. |
| `src/kernel/resources.rs` | Per-user resource tracking. |
| `src/kernel/interrupts.rs` | Tool-confirmation gate. |
| `src/agent/mod.rs` | LlmAgent ReAct loop, context overflow, hook invocations. |
| `src/agent/hooks.rs` | `HookDecision` paths. |
| `src/agent/prompts.rs` | Template rendering. |
| `src/tools/registry.rs` | `ToolRegistry`, `AclToolExecutor`, policy/catalog/health gates, confirmation. |
| `src/tools/access.rs` | `ToolAccessPolicy` grant/revoke. |
| `src/tools/catalog.rs` | `ParamDef` validation, prompt generation. |
| `src/tools/health.rs` | Sliding-window metrics, circuit breaker. |
| `tests/runner.rs` | Full pipeline integration tests (linear, routing, streaming, interrupts). |
| `tests/schema.rs` | JSON Schema drift + deserialization sanity. |

Use the `test-harness` feature flag for test utilities in consumer integration tests.
