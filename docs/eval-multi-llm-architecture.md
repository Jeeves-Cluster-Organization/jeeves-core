# Architecture Evaluation: Multi-LLM-Call Execution in jeeves-core

Evaluation of whether jeeves-core's current architecture supports disciplined
multi-LLM-call execution for a self-hosted assistant — covering orchestration,
memory/state, tool control, and local inference backend suitability.

Each proposed change has been audited against the codebase to verify it is not
already implemented under a different name or pattern.

---

## 1. Where multi-call orchestration is already implemented

### Pipeline graph engine (the core strength)

The kernel/worker separation enforces that **workers execute, the kernel decides**.
The worker loop (`src/worker/mod.rs:107-276`) calls `get_next_instruction()`,
runs the agent, reports the result, and loops. Workers have no control over
sequencing — this is the right architecture for disciplined multi-call execution.

**Explicit phase separation** via `PipelineConfig` stages
(`src/kernel/orchestrator_types.rs:17-26`). Each stage is a named node with
`NodeKind` (Agent, Gate, Fork), configurable routing, per-stage
model/temperature/timeout overrides.

**Per-stage model selection**: Each `PipelineStage` can specify a different
`model` role (e.g., `"fast"`, `"reasoning"`), resolved by
`GenaiProvider::resolve_model()` via `MODEL_*` env vars
(`src/worker/llm/genai_provider.rs:111-118`).

**Routing functions** (`src/kernel/routing.rs`) enable data-dependent branching.
A routing function receives outputs, state, and metadata, and returns `Next`,
`Fan`, or `Terminate`.

**Parallel fan-out with join** (`src/kernel/orchestrator.rs:250-277`). Fork nodes
dispatch multiple agents concurrently via `tokio::spawn` with configurable
`JoinStrategy`.

**Bounds enforcement** across the pipeline (`src/envelope/mod.rs:173-184`):
`max_llm_calls`, `max_iterations`, `max_agent_hops`, per-edge limits, per-stage
visit limits.

**Checkpoint/resume** (`src/kernel/checkpoint.rs`). Full pipeline state is
serializable — envelope, edge traversals, stage visits, parallel group state.

---

## 2. Hidden single-call assumptions

### a) ReAct loop builds messages from scratch each stage

`src/worker/agent.rs:148-329` — `LlmAgent.process()` always starts with a fresh
`[system, user]` message pair. There is no mechanism to carry conversation history
across stages. Prior stage outputs are available only as template variables
(`agent.rs:113-123`) string-interpolated into the system prompt — this flattens
structured LLM reasoning and loses assistant/tool message structure.

This is the **single biggest hidden single-call assumption**.

**Audit**: Verified no field like `prior_messages`, `history`, `conversation`, or
`chat_history` exists on `AgentContext`, `AgentDispatchContext`, `Envelope`, or
`PipelineStage`. No flag like `carry_messages` or `forward_history` exists in
`AgentConfig`. The `state_schema` Append merge strategy *could* theoretically
accumulate serialized messages in `envelope.state`, but nothing in the agent
execution path deserializes or hydrates them — `build_agent_context()`
(`src/worker/mod.rs:357-390`) only extracts `raw_input`, `outputs`, `state`, and
`metadata`, never reconstructing chat messages.

### b) No conversation memory across pipeline runs

`Envelope` is created fresh per request (`src/envelope/mod.rs:76-138`). No
built-in session memory persists across separate `PipelineRunner::run()` calls.
Multi-turn assistant conversations require external state injection into
`envelope.state` or `envelope.metadata`.

**Audit**: No `StateStore`, `SessionStore`, `StateBackend`, or persistence trait
exists. `session_id` is identity-only — never used for state lookup. The
checkpoint system (`src/kernel/checkpoint.rs`) captures in-memory snapshots as
serializable JSON, but the *caller* is responsible for persisting them externally.
No database, file, or Redis dependency exists in Cargo.toml. However, callers
*can* pre-populate `envelope.state` before a pipeline run (the field is public
and `merge_updates()` exists), and the Python bindings expose `checkpoint()` and
`resume_from_checkpoint()` for manual persistence. The gap is a *hook*, not a
missing data path.

### c) Tool registries are static per pipeline

`PipelineRunner::rebuild_registries()` (`src/worker/runner.rs:311-331`)
reconstructs all registries at setup time. Stages cannot dynamically grant or
revoke tools for the next stage based on runtime conditions — tool ACLs are
configured statically, not computed from state.

**Audit**: Confirmed tool ACLs are baked at two levels:

1. **Agent creation time** (`src/worker/agent_factory.rs:96-102`):
   `AclToolExecutor::wrap_registry()` creates a filtered `ToolRegistry` per
   agent based on `PipelineStage.allowed_tools`. This wrapped registry is stored
   inside `LlmAgent.tools` / `McpDelegatingAgent.tools` and never changes.

2. **Session initialization** (`src/kernel/kernel_orchestration.rs:30-34`):
   `self.tools.access.grant_many(&stage.agent, tools)` is called once during
   `initialize_orchestration()`. No `grant` or `revoke` calls happen during
   routing or agent execution.

`AgentDispatchContext` *does* carry an `allowed_tools: Option<Vec<String>>` field
(`src/kernel/orchestrator_types.rs:70`), and it gets populated from
`ToolAccessPolicy` at dispatch time (`kernel_orchestration.rs:121-124`). But this
field is **never consumed** — `build_agent_context()` does not transfer it to
`AgentContext`, and the agent uses its baked-in `ToolRegistry` instead.
`RoutingResult` (`src/kernel/routing.rs`) carries only stage names (`Next`,
`Fan`, `Terminate`) — no tool metadata.

---

## 3. Modularity of memory, context, and tool flows

### Memory/state: Good

`envelope.state` HashMap with `MergeStrategy` (Replace/Append/MergeDict) at
`src/kernel/mod.rs:57-94`. Stages write structured data; subsequent stages
receive it via `AgentContext.state`. The `state_schema` on `PipelineStage`
controls merge behavior per field.

### Context: Modular but limited

`AgentContext` (`src/worker/agent.rs:37-60`) cleanly separates `raw_input`,
`outputs`, `state`, and `metadata`. However, `max_context_tokens` uses a
`chars/4` heuristic (`agent.rs:152-154`) — fragile for local models with
different tokenizers.

### Tool flows: Well-isolated

`ToolRegistry` + `AclToolExecutor` + per-stage `allowed_tools` gives each stage
exactly the tools it should see. MCP integration (`src/worker/mcp.rs`) enables
external tool servers. The `requires_confirmation` gate enables human-in-the-loop.

### Prompt management: Clean

`PromptRegistry` loads templates from a directory with variable substitution.
Each stage has its own `prompt_key`. No magic coupling between stages.

---

## 4. Suitability for local/self-hosted inference backends

### Already supported

- `LLM_API_BASE` env var (`src/worker/llm/genai_provider.rs:71-84`) routes all
  calls through a custom OpenAI-compatible endpoint (llama.cpp, vLLM, Ollama).
- `MODEL_*` role mapping (`genai_provider.rs:64-69`) supports multi-model local
  setups: `MODEL_FAST=local-phi-3`, `MODEL_REASONING=local-llama-70b`.
- Streaming path (`src/worker/llm/mod.rs:287-324`) benefits slow local models.

### Gaps for local deployment

- **No LLM backend health awareness.** `GenaiProvider` has basic retry
  (`MAX_RETRIES=3`, exponential backoff) but no circuit breaker for consistently
  failing/slow local servers. `ToolHealthTracker` exists for tools but not for
  LLM backends.
- **Token counting is approximate.** The `chars/4` heuristic won't match actual
  tokenizer counts for local models with tight context budgets.

---

## 5. Smallest high-leverage changes

### 1. Stage-to-stage message history forwarding — CONFIRMED MISSING

**Problem**: Each `LlmAgent` starts from `[system, user]` — no way to carry
forward conversation context from a prior LLM stage.

**Audit result**: No existing mechanism. `AgentContext` has no message history
field. `build_agent_context()` does not extract or reconstruct chat messages from
any source. `PipelineStage` and `AgentConfig` have no conversation forwarding
flags. The `state_schema` Append strategy could theoretically accumulate messages
in `envelope.state`, but the agent execution path never deserializes them back
into `ChatMessage` objects. This is a genuine gap.

**Fix**: Add `prior_messages: Option<Vec<ChatMessage>>` to `AgentContext`.
In `LlmAgent.process()`, prepend them after the system prompt and before the user
message when present. Add a `carry_messages: bool` flag to `AgentConfig`.
In the worker loop, when a stage completes with `carry_messages`, serialize its
final message list into `envelope.state["__messages"]` using the Append merge
strategy. The next stage's `build_agent_context()` hydrates the field.
~50 lines across `agent.rs`, `orchestrator_types.rs`, and `src/worker/mod.rs`.

### 2. Pluggable token counter — CONFIRMED MISSING

**Problem**: `chars/4` heuristic at `agent.rs:152-154` is fragile for local
models with different tokenizers.

**Audit result**: Token estimation happens in exactly one place
(`agent.rs:152-154` and the truncation at `agent.rs:165`). No `TokenCounter`,
`Tokenizer`, or `TokenEstimator` trait exists. `LlmAgent` has no tokenizer
field. `GenaiProvider` has no tokenizer field. `LlmProvider` trait has no token
counting methods. No tokenizer crate (`tiktoken`, `sentencepiece`, `tokenizers`)
appears in Cargo.toml. The `genai` crate (v0.5) provides `TokenUsage` from API
responses but no client-side tokenizer. API-reported token counts
(`resp.usage.prompt_tokens`) are tracked in metrics but never fed back into
context window checks. This is a genuine gap.

**Fix**: Define a `TokenCounter` trait with `fn estimate(messages: &[ChatMessage])
-> i64`. Add `token_counter: Arc<dyn TokenCounter>` to `LlmAgent`, defaulting to
`CharDiv4Counter` (the current heuristic). Replace the two inline estimation
sites in `LlmAgent.process()`. Wire it through `AgentFactoryBuilder` so
consumers can inject a model-specific tokenizer. ~30 lines.

### 3. Dynamic tool scoping via routing functions — CONFIRMED MISSING

**Problem**: Tool ACLs are static per stage config. A classifier stage cannot
decide at runtime which tools the next stage should access.

**Audit result**: Two-level static binding confirmed:

- `AclToolExecutor::wrap_registry()` creates a filtered `Arc<ToolRegistry>` at
  agent creation time (`src/worker/agent_factory.rs:99-102`), baked into
  `LlmAgent.tools`.
- `ToolAccessPolicy::grant_many()` is called only at session initialization
  (`src/kernel/kernel_orchestration.rs:32`), never during routing or dispatch.
- `RoutingResult` (`src/kernel/routing.rs`) is `Next(String) | Fan(Vec<String>)
  | Terminate` — carries no tool metadata.
- `AgentDispatchContext.allowed_tools` exists and gets populated from
  `ToolAccessPolicy` at dispatch time, but is **dead data** — never consumed by
  `build_agent_context()` or `AgentContext`.

The `AgentDispatchContext.allowed_tools` field is the natural injection point
here — the plumbing half-exists but is disconnected.

**Fix**: Thread `AgentDispatchContext.allowed_tools` into `AgentContext`. In
`LlmAgent.process()`, when `ctx.allowed_tools` is set and differs from the
baked-in registry, apply a runtime filter (re-wrap or subset the tool defs sent
to the LLM). Extend `RoutingResult::Next` to carry an optional
`tool_overrides: Option<Vec<String>>` that the orchestrator stores in the
dispatch context. ~40 lines across `routing.rs`, `orchestrator.rs`, and
`src/worker/mod.rs`. The dead `allowed_tools` field on `AgentDispatchContext`
already handles half the wiring.

### 4. Session-scoped state persistence hook — CONFIRMED MISSING (with partial workaround)

**Problem**: No built-in multi-turn memory across pipeline runs.

**Audit result**: No `StateStore` or persistence trait exists. No database or
storage dependencies. `session_id` is identity-only, never used for state
lookup. `process_envelopes` and `orchestrator.pipelines` are in-memory
`HashMap`s keyed by `ProcessId`, not `SessionId`.

However, there is a **partial workaround path**:
- `envelope.state` is a public `HashMap` that callers can pre-populate before
  calling `PipelineRunner::run()`.
- The checkpoint system serializes full pipeline state to JSON, and
  `resume_from_checkpoint()` restores it.
- Python bindings expose `checkpoint()` → JSON and `resume_from_checkpoint(json)`.

An application can implement session persistence externally today:
`load state → inject into envelope.state → run pipeline → extract state → save`.
The gap is that this requires boilerplate in every consumer. A `StateStore` trait
with lifecycle hooks would standardize it.

**Fix**: Define `StateStore` trait: `async fn load(session_id: &str) ->
Option<HashMap<String, Value>>` and `async fn save(session_id: &str,
state: &HashMap<String, Value>)`. In `run_pipeline_with_envelope()`, call
`load()` and merge into `envelope.state` before session init. After pipeline
completes, call `save()` with the final `envelope.state`. Default impl is a
no-op (preserves current behavior). ~60 lines across the trait definition and
`src/worker/mod.rs`.

### 5. LLM backend health tracking — CONFIRMED MISSING

**Problem**: No circuit breaking for unreliable local model servers.

**Audit result**: `ToolHealthTracker` (`src/tools/health.rs`) provides
production-grade tool health tracking: sliding window of 100 records, per-tool
success rate and latency monitoring, health status (Healthy/Degraded/Unhealthy),
circuit breaker (5 errors in 5 minutes opens the circuit), configurable
thresholds. Tool execution records are fed into it from
`kernel_orchestration.rs:245-248`.

**No equivalent exists for LLM backends.** `GenaiProvider` has retry logic
(`MAX_RETRIES=3`, exponential backoff starting at 500ms, retries on 429/5xx/
network errors at `genai_provider.rs:22-39`) but no health state, no sliding
window, no circuit breaker. `AgentExecutionMetrics` tracks per-agent totals
(llm_calls, tokens_in, tokens_out, duration_ms) but not per-call LLM latency
or failure rates.

The `ServiceRegistry` (`src/kernel/services.rs`) tracks service health status
(Healthy/Degraded/Unhealthy) and the `HealthStatus` enum is shared
(`src/envelope/enums.rs`), but LLM providers are not registered as services.
`ResourceTracker` tracks quota usage, not health.

**Fix**: Create `LlmHealthTracker` mirroring `ToolHealthTracker`'s API — same
`record_execution()`, `get_health()`, `should_circuit_break()` interface with
the same `HealthConfig` structure. Add `health: LlmHealthTracker` to `Kernel`
(alongside `tools.health`). Record LLM call results in
`process_agent_result()` where tool results are already recorded. Optionally
register LLM backends as services in `ServiceRegistry` for unified health
monitoring. ~100 lines, directly modeled on `src/tools/health.rs`.

---

## Summary

jeeves-core is **already well-architected for multi-LLM-call execution**. The
kernel/worker separation, pipeline graph with routing, per-stage model selection,
envelope state management, bounds enforcement, and checkpoint/resume form a solid
foundation.

All five proposed changes have been **audited and confirmed as genuine gaps** —
none are already implemented under different names. The closest to "partially
exists" are:

- **Dynamic tool scoping (#3)**: `AgentDispatchContext.allowed_tools` is
  populated but never consumed — half the plumbing exists.
- **Session persistence (#4)**: Callers can manually inject state into
  `envelope.state` and use checkpoint/resume for serialization — the data path
  works, but there is no standardized hook.

The main gap remains the **message history boundary at stage transitions**: each
LlmAgent starts fresh rather than carrying forward conversation context. This is
the single biggest hidden single-call assumption, and the fix is surgical.

For a self-hosted assistant with specialized multi-model pipelines, this
architecture works today. The recommended changes tighten execution discipline
without restructuring.
