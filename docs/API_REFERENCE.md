# API Reference

`jeeves-core` is a Rust library. Consumers depend on it directly — there is no service binary, no HTTP gateway, no Python module.

```rust
use jeeves_core::prelude::*;
```

---

## Pipeline Config

A pipeline is a `PipelineConfig` value (constructed in Rust or deserialized from JSON). The JSON schema in `schema/pipeline.schema.json` matches the Rust types.

### PipelineConfig

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Pipeline name. Used for event attribution. |
| `stages` | `[PipelineStage]` | yes | Ordered list of stages. First stage is the entry point. |
| `max_iterations` | int | yes | Global iteration bound. Terminates with `MaxIterationsExceeded`. |
| `max_llm_calls` | int | yes | Global LLM-call bound across all stages. |
| `max_agent_hops` | int | yes | Bound on transitions between stages. |
| `state_schema` | `[StateField]` | no | Typed state fields with merge strategies for loop-back accumulation. |

### PipelineStage

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | string | — | Stage identifier (unique within the pipeline). |
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

`state_schema` controls how stage outputs merge into `envelope.state` across pipeline iterations:

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

Read-only view of pipeline state passed to a routing function:

- `current_stage`, `agent_name`, `agent_failed`
- `outputs` — `agent_name → { key → value }`
- `metadata` — envelope metadata
- `state` — accumulated state across iterations
- `interrupt_response` — resolved interrupt response, if any

### RoutingResult

| Variant | Description |
|---|---|
| `Next(String)` | Route to the named target stage. |
| `Terminate` | End the pipeline (`COMPLETED`). |

### Evaluation order

1. Agent failed AND `error_next` set → `error_next`.
2. `routing_fn` registered → call it.
3. `default_next` → route there.
4. None of the above → terminate `COMPLETED`.

---

## TerminalReason

`Completed`, `BreakRequested`, `MaxIterationsExceeded`, `MaxLlmCallsExceeded`, `MaxAgentHopsExceeded`, `UserCancelled`, `ToolFailedFatally`, `LlmFailedFatally`, `PolicyViolation`, `MaxStageVisitsExceeded`.

---

## Per-stage stream modes

Each pipeline stage picks one of two LLM stream modes via the presence of `response_format`:

| `response_format` | LLM behavior | `Delta` tokens carry | Output lands at |
|---|---|---|---|
| `None` | Free-form generation | **prose tokens** | `outputs[stage]["response"]` (raw text — see `build_output_value` in `src/worker/agent.rs`) |
| `Some(schema)` | Grammar-constrained JSON | **JSON tokens** | `outputs[stage][field]` per the schema (parsed into the output map automatically) |

Both modes coexist within a single pipeline — pick per stage based on what consumes the output. Prose mode is the right choice when the stage's output is shown to the user directly. JSON mode is the right choice when the next stage or post-pipeline mutation code reads structured fields.

**Display policy belongs to the consumer.** The kernel emits `Delta` events for every stage that runs an LLM, regardless of mode. The consumer decides per stage whether to hide the deltas, render a status label while the stage runs, or stream the deltas into a UI text field. There is no kernel concept of "user-visible stage" — that's a UX decision that lives in the consumer.

## Streaming Events

`run_pipeline_streaming` returns `(JoinHandle<Result<WorkerResult>>, mpsc::Receiver<PipelineEvent>)`. Consumers drain the receiver for real-time events.

`PipelineEvent` is `#[non_exhaustive]` with these variants (current set):

| Variant | Payload | When |
|---|---|---|
| `StageStarted` | `stage`, `pipeline` | Right before an agent starts executing a stage. |
| `Delta` | `content`, `stage?`, `pipeline` | Incremental LLM token chunk. `stage = None` for ad-hoc calls. |
| `ToolCallStart` | `id`, `name`, `stage?`, `pipeline` | LLM emitted a tool call; before execution. |
| `ToolResult` | `id`, `content`, `stage?`, `pipeline` | Tool produced a result. Matches a prior `ToolCallStart` by `id`. |
| `StageCompleted` | `stage`, `pipeline`, `metrics?` | Stage finished (success or recovered failure). |
| `RoutingDecision` | `from_stage`, `to_stage?`, `reason`, `pipeline` | Kernel selected the next stage (or terminated, `to_stage = None`). |
| `InterruptPending` | `process_id`, `interrupt_id`, `kind`, `question?`, `message?`, `pipeline` | A tool-confirmation gate is open. |
| `Error` | `message`, `stage?`, `pipeline` | Error emitted out-of-band; pipeline may continue if `error_next` is set. |
| `Done` | `process_id`, `terminated`, `terminal_reason?`, `outputs?`, `pipeline`, `aggregate_metrics?` | Pipeline reached a terminal state. Always last. |

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
| `Kernel` | `kernel` | Process manager + orchestrator. |
| `KernelHandle` | `worker::handle` | Typed mpsc channel to the kernel actor (`Clone + Send + Sync`). |
| `PipelineConfig` | `kernel::orchestrator_types` | Pipeline definition. |
| `PipelineStage` | `kernel::orchestrator_types` | Stage definition. |
| `Envelope` | `envelope` | Request state container (raw_input, outputs, state, metadata). |
| `Instruction` | `kernel::orchestrator_types` | Kernel→worker command. |
| `Agent` | `worker::agent` | Agent trait. |
| `AgentContext` | `worker::agent` | Execution context passed to agents (`outputs`, `state`, `metadata`, `stage_name`, `event_tx`, ...). |
| `LlmAgent` | `worker::agent` | LLM agent with ReAct tool loop + hooks. |
| `ToolDelegatingAgent` | `worker::agent` | Single-tool dispatcher. |
| `DeterministicAgent` | `worker::agent` | Passthrough / no-op agent. |
| `AgentRegistry` | `worker::agent` | `name → Arc<dyn Agent>` map. |
| `AgentFactoryBuilder` | `worker::agent_factory` | Auto-creates agents from a `PipelineConfig`. |
| `PipelineEvent` | `worker::llm` | Streaming event variants (see above). |
| `LlmProvider` | `worker::llm` | Trait for LLM HTTP backends. Default impl: `GenaiProvider`. |
| `PromptRegistry` | `worker::prompts` | Prompt template loader (`{var}` substitution). |
| `ToolExecutor` | `worker::tools` | Tool implementation trait. |
| `ToolRegistry` | `worker::tools` | Composed tool registry with optional ACL / catalog / health gates. |
| `ToolRegistryBuilder` | `worker::tools` | Composable builder for `ToolRegistry`. |
| `AclToolExecutor` | `worker::tools` | Wraps a `ToolExecutor` with an explicit allow-list (per-stage filter). |
| `ToolAccessPolicy` | `tools::access` | Agent×tool ACL consulted by `ToolRegistry::execute_for`. |
| `ToolCatalog` | `tools::catalog` | Typed `ParamDef` metadata + parameter validation. |
| `ToolHealthTracker` | `tools::health` | Sliding-window metrics + circuit breaker per tool. |
| `LlmAgentHook` | `worker::hooks` | Pluggable lifecycle hook around the ReAct loop. |
| `FlowInterrupt` | `envelope` | Tool-confirmation gate request. |
| `InterruptService` | `kernel::interrupts` | Pending-interrupt bookkeeping inside the kernel. |

### Agent auto-creation (AgentFactoryBuilder)

Given a `PipelineConfig`, agents are created per stage:

| Stage condition | Agent type |
|---|---|
| `has_llm: true` | `LlmAgent` |
| `has_llm: false` AND a tool with the same name exists | `ToolDelegatingAgent` |
| `has_llm: false` AND no matching tool | `DeterministicAgent` (passthrough) |

---

## ToolRegistry — policy / catalog / health gates

`ToolRegistry::execute_for(agent_name, tool_name, params)` traverses an optional chain in this order:

1. **`ToolAccessPolicy`** (if attached) — denies tools the agent has not been granted; returns `Error::policy_violation`. Default-deny: when a policy is attached, an agent with no matching grant cannot call any tool.
2. **`ToolCatalog`** (if attached AND the tool is in the catalog) — validates `params` against the tool's `ParamDef`s; returns `Error::validation`.
3. **`ToolHealthTracker`** (if attached) — short-circuits with `Error::policy_violation` when the breaker is open.
4. Executes the tool, recording `(success, latency_ms, error_code)` into the health tracker.

The same `ToolAccessPolicy` is also consulted by `AgentFactoryBuilder` at agent-construction time — each agent's tool registry is wrapped to expose only the tools its grants permit (so the LLM never sees forbidden tool defs in its prompt). One policy, two enforcement points (prompt-construction filter + runtime gate). Without a policy attached, the strict default applies: agents get zero tools. Consumers opt into tool access by declaring grants.

Attach via builder:

```rust
let policy = Arc::new(ToolAccessPolicy::new()); // .grant("agent", "tool")
let catalog = Arc::new(ToolCatalog::new());     // .register(ToolEntry { .. })
let health = Arc::new(Mutex::new(ToolHealthTracker::new(HealthConfig::default())));

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

`LlmAgentHook` (in `worker::hooks`) is a single trait with three default-impl methods. Hooks register on `LlmAgent.hooks` (via `AgentFactoryBuilder::with_hook(...)` or direct mutation) and run in registration order.

| Method | When | Return / effect |
|---|---|---|
| `before_llm_call(messages: &mut Vec<ChatMessage>)` | Before each LLM call. | Mutate the message window (compaction, redaction, prompt-injection defense). |
| `before_tool_call(call: &ToolCall) -> HookDecision` | After the LLM emits a tool call, before execution. | `Continue` (run normally), `Replace(ToolOutput)` (skip executor; deliver canned result), `Block { reason }` (skip executor; deliver error). First non-`Continue` decision wins. |
| `after_tool_call(call, output: &mut ToolOutput)` | After a tool result is produced (real or via `HookDecision::Replace`). | Mutate the output (redact, decorate). Skipped for `Block`. |

---

## Consumer pattern

```rust
use jeeves_core::prelude::*;
use jeeves_core::worker::llm::genai_provider::GenaiProvider;
use std::sync::Arc;

let llm: Arc<dyn LlmProvider> = Arc::new(GenaiProvider::new("qwen3-14b"));
let prompts = Arc::new(PromptRegistry::from_dir("prompts/"));
let tools = ToolRegistryBuilder::new()
    .add_executor(Arc::new(MyTools::new()))
    .build();

let mut kernel = Kernel::new();
kernel.register_routing_fn("router", Arc::new(|ctx: &RoutingContext<'_>| {
    RoutingResult::Next("next_stage".into())
}));

let cancel = tokio_util::sync::CancellationToken::new();
let handle = jeeves_core::worker::actor::spawn_kernel(kernel, cancel);

let agents = AgentFactoryBuilder::new(llm, prompts, tools)
    .add_pipeline(config.clone())
    .build();

let envelope = Envelope::new_minimal("user1", "session1", "hello", None);
let result = run_pipeline_with_envelope(
    &handle, ProcessId::new(), config, envelope, &agents,
).await?;
```

For streaming:

```rust
let (join, mut rx) = run_pipeline_streaming(
    handle, ProcessId::new(), config, envelope, agents,
).await?;
while let Some(event) = rx.recv().await {
    match event {
        PipelineEvent::Delta { content, .. } => print!("{content}"),
        PipelineEvent::Done { terminal_reason, .. } => break,
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
| `src/kernel/mod.rs` | `MergeStrategy` (Replace / Append / MergeDict). |
| `src/kernel/routing.rs` | RoutingFn registry + evaluation order. |
| `src/kernel/orchestrator.rs` | Routing, `max_visits`, `error_next`. |
| `src/kernel/resources.rs` | Quota enforcement. |
| `src/kernel/interrupts.rs` | Tool-confirmation gate. |
| `src/worker/tools.rs` | `ToolRegistry`, `AclToolExecutor`, policy/catalog/health gates, confirmation. |
| `src/worker/agent.rs` | LlmAgent ReAct loop, context overflow, hook invocations. |
| `src/worker/hooks.rs` | `HookDecision` paths. |
| `src/worker/prompts.rs` | Template rendering. |
| `src/tools/access.rs` | `ToolAccessPolicy` grant/revoke. |
| `src/tools/catalog.rs` | `ParamDef` validation, prompt generation. |
| `src/tools/health.rs` | Sliding-window metrics, circuit breaker. |
| `tests/worker_integration.rs` | Full pipeline integration tests (linear, routing, streaming, interrupts). |

Use the `test-harness` feature flag for test utilities in consumer integration tests.
