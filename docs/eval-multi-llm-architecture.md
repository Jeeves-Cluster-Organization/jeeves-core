# Architecture Evaluation: Multi-LLM-Call Execution in jeeves-core

Evaluation of whether jeeves-core's current architecture supports disciplined
multi-LLM-call execution for a self-hosted assistant — covering orchestration,
memory/state, tool control, and local inference backend suitability.

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

### b) No conversation memory across pipeline runs

`Envelope` is created fresh per request (`src/envelope/mod.rs:76-138`). No
built-in session memory persists across separate `PipelineRunner::run()` calls.
Multi-turn assistant conversations require external state injection into
`envelope.state` or `envelope.metadata`.

### c) Tool registries are static per pipeline

`PipelineRunner::rebuild_registries()` (`src/worker/runner.rs:311-331`)
reconstructs all registries at setup time. Stages cannot dynamically grant or
revoke tools for the next stage based on runtime conditions — tool ACLs are
configured statically, not computed from state.

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

### 1. Stage-to-stage message history forwarding

**Problem**: Each `LlmAgent` starts from `[system, user]` — no way to carry
forward conversation context from a prior LLM stage.

**Fix**: Add `prior_messages: Vec<ChatMessage>` to `AgentContext`. Worker loop
populates it from `envelope.state` when stage config has `carry_messages: true`.
~50 lines across `agent.rs` and the worker loop.

### 2. Pluggable token counter

**Problem**: `chars/4` heuristic at `agent.rs:152-154` is fragile for local models.

**Fix**: `Arc<dyn TokenCounter>` trait object on `LlmAgent`, defaulting to
current heuristic. ~30 lines.

### 3. Dynamic tool scoping via routing functions

**Problem**: Tool ACLs are static per stage config.

**Fix**: Extend `RoutingResult` to carry optional `allowed_tools` overrides.
~40 lines across `routing.rs` and `orchestrator.rs`.

### 4. Session-scoped state persistence hook

**Problem**: No built-in multi-turn memory across pipeline runs.

**Fix**: `StateStore` trait (`load(session_id)`, `save(session_id, state)`)
called at pipeline start/end. ~60 lines.

### 5. LLM backend health tracking

**Problem**: No circuit breaking for unreliable local model servers.

**Fix**: Mirror existing `ToolHealthTracker` for `LlmProvider`. ~100 lines.

---

## Summary

jeeves-core is **already well-architected for multi-LLM-call execution**. The
kernel/worker separation, pipeline graph with routing, per-stage model selection,
envelope state management, bounds enforcement, and checkpoint/resume form a solid
foundation.

The main gap is the **message history boundary at stage transitions**: each
LlmAgent starts fresh rather than carrying forward conversation context. This is
the single biggest hidden single-call assumption, and the fix is surgical.

For a self-hosted assistant with specialized multi-model pipelines, this
architecture works today. The recommended changes tighten execution discipline
without restructuring.
