# Jeeves-Core Framework Audit: Agentic Orchestration Evaluation

**Date:** 2026-03-15 (updated with commits through a723e25)
**Purpose:** Evaluate jeeves-core as an orchestration framework for agentic use cases vs. OSS alternatives

---

## 1. What Is Jeeves-Core?

A **Rust microkernel for multi-agent AI orchestration**, exposed to Python via PyO3. It implements a Kubernetes/Temporal-inspired **evaluator pattern** — a deterministic runtime that dispatches agents through typed pipelines with expression-based routing, resource bounds, and checkpoint durability.

**It is not** a LangChain/LlamaIndex competitor. It's the **orchestration runtime layer underneath** those frameworks. Think of it as the scheduler and state machine, not the prompt library.

## 2. Architecture Overview

| Layer | What It Does | Key Files |
|---|---|---|
| **Kernel** | Actor loop, process lifecycle, rate limiting | `src/kernel/` |
| **Orchestrator** | Instruction generation, routing evaluation, parallel joins | `orchestrator.rs`, `routing.rs` |
| **Worker** | Agent execution, LLM calls, tool dispatch, streaming | `src/worker/` |
| **Envelope** | Per-process state container: bounds, outputs, audit trail | `src/envelope/` (2400+ lines) |
| **CommBus** | Federation layer: pub/sub events, commands, queries | `src/commbus/` |
| **Python** | PyO3 native module: PipelineRunner, streaming iterator, tool bridge | `src/python/` |
| **MCP** | Model Context Protocol stdio server for tool proxying | `src/worker/mcp.rs` |

**Core abstractions:**
- **Pipeline** = ordered stages (agents, gates, forks, routers) with routing rules
- **Envelope** = process state bag (inputs, outputs, bounds, metrics, audit)
- **Instruction** = kernel → worker dispatch (RunAgent, RunAgents, WaitInterrupt, Terminate)
- **RoutingExpr** = 13 operators × 5 scopes for deterministic branching

## 3. Orchestration Capabilities

### Routing System (13 operators, 5 scopes)

- **Operators:** `Eq`, `Neq`, `Gt`, `Gte`, `Lt`, `Lte`, `Contains`, `Exists`, `Not`, `And`, `Or`, `In`, `Always`
- **Scopes:** `Current` (last agent output), `Agent` (specific prior agent), `State` (merged state), `Meta` (audit metadata), `Interrupt` (human response)

### Topology Nodes

- **Agent** — standard LLM/tool execution stage
- **Gate** — pure routing (no agent execution, evaluated in a loop up to step_limit=100)
- **Fork** — parallel fan-out to multiple agents with join strategies (WaitAll, WaitFirst)
- **Router** *(new)* — agent-determined routing. The LLM agent picks the next stage from a declared set of `RouterTarget`s. Kernel auto-generates an output schema constraining `chosen_target` to the valid enum of available targets, and injects target metadata into the agent context. Guard conditions (`when` expressions) can dynamically filter which targets are available. Falls back to `error_next`/`default_next` on invalid or missing selection.

### State Management

- Schema-driven merge strategies: `Replace` (last-write-wins), `Append` (accumulate array), `MergeDict` (shallow merge)
- Envelope carries full state between stages — no external session store required
- Checkpoint/resume at instruction boundaries for crash recovery

### Resource Bounds Enforcement

- Max LLM calls, max agent hops, max iterations, per-edge traversal limits, max context tokens
- Pre- and post-routing validation — pipeline terminates deterministically when bounds hit

### Human-in-the-Loop

- Flow interrupts with configurable expiry
- Interrupt response feeds back into routing expressions (`Interrupt` scope)

### Parallel Execution

- Fork nodes dispatch to multiple agents concurrently via tokio
- Join strategies: WaitAll (all must complete) or WaitFirst (first success wins)
- Post-join routing continues from fork's `default_next`

### Federation

- CommBus: pub/sub events, fire-and-forget commands, request/response queries with timeout
- Pipeline subscriptions/publishes for cross-pipeline communication
- MCP stdio server for tool proxying to external systems

## 4. Code Quality & Maturity

| Metric | Assessment |
|---|---|
| **Language** | Rust (`#[deny(unsafe_code)]`) — memory safe |
| **Test count** | 348 tests (unit, integration, property-based) |
| **Error handling** | Typed `Error` enum with variants, no `.unwrap()` in production paths |
| **Async** | Full tokio runtime with cancellation support |
| **Observability** | `#[instrument]` tracing spans, envelope snapshots, OpenTelemetry bridge |
| **Python bindings** | PyO3 native module, GIL-aware streaming, zero-copy |
| **Schema validation** | Build-time pipeline config + runtime envelope validation |
| **JSON Schema** | `--emit-schema` CLI for editor autocompletion |

**Concerns:**
- Single-actor-per-process — no distributed consensus or multi-node clustering
- No pre-built monitoring dashboards (tracing + OTel bridge exists but DIY)
- Expression-only routing — no arbitrary lambda/predicate functions
- No built-in retry at the framework level (must use routing loops)

## 5. Competitive Landscape Comparison

| Dimension | **Jeeves-Core** | **LangGraph** | **CrewAI** | **Claude Agent SDK** | **OpenAI Agents SDK** |
|---|---|---|---|---|---|
| **Language** | Rust + PyO3 | Python | Python | Python | Python |
| **Stars** | Private | ~26K | ~46K | New | ~19K |
| **Pattern** | Evaluator/microkernel | State graph | Role-based crews | Agentic loop | Minimal 3-concept |
| **Routing** | Expression trees (13 ops) + Router nodes (LLM-directed) | Conditional edges + LLM | Sequential/hierarchical | Tool-driven | Handoffs |
| **Parallelism** | Fork nodes (native) | Fan-out/fan-in | Sequential default | Sequential | Sequential |
| **State** | Envelope + schema merge | Typed state channels | Shared memory | Context window | Context/handoff |
| **Bounds** | Built-in (LLM, hops, edges, tokens) | Manual | Manual | `max_turns` | `max_turns` |
| **Human-in-loop** | Flow interrupts + routing | `interrupt_before/after` | Human tool | Not built-in | Not built-in |
| **Durability** | Checkpoint/resume | Checkpointer interface | No | No | No |
| **Streaming** | Native SSE events | LangGraph Cloud | No | Token streaming | Token streaming |
| **Tool system** | ACL-filtered registry + MCP | LangChain tools | CrewAI tools | Tool use (native) | Function tools + MCP |
| **Model lock-in** | None (LlmProvider trait + role resolver) | None | None | Claude-only | OpenAI-only |
| **Performance** | Rust-native (<5ms orchestration) | Python (~50-100ms) | Python | Python | Python |

**Also evaluated:**
- **AutoGen** (~50K stars): Maintenance mode, merging into Microsoft Agent Framework
- **Semantic Kernel** (~27K stars): Transitioning to Agent Framework, enterprise-focused
- **Haystack** (~24K stars): Pipeline-based, strongest for RAG/document workflows
- **DSPy** (~28K stars): Complementary — solves prompt optimization, not orchestration

**Industry trend:** MCP (Model Context Protocol) is becoming the universal tool interoperability layer across most of these frameworks.

## 6. Recent Additions (commits 6fd4679, a723e25)

### NodeKind::Router — Agent-Determined Routing

A new pipeline stage type where the **LLM agent chooses the next node** from a declared set of targets. Unlike expression-based routing (deterministic, rule-driven), Router nodes delegate the routing decision to the agent's reasoning.

**How it works:**
1. Stage declares `router_targets: Vec<RouterTarget>` — each with a `target` name, `description`, and optional `when` guard condition
2. Kernel evaluates guard conditions, filters to available targets, and auto-generates an output schema constraining `chosen_target` to valid options
3. Agent receives available targets in its context and must output `{"chosen_target": "stage_name"}`
4. Orchestrator validates the choice; invalid/missing selections fall back to `error_next` → `default_next` → terminate

**Why it matters:** Bridges the gap between fully-deterministic routing (Jeeves's strength) and LLM-directed control flow (LangGraph's strength). Pipelines can now mix both paradigms — use expressions for predictable branches, Router nodes for nuanced decisions.

### Model Role Resolver — Semantic Model Selection

Replaces hardcoded model names with **environment-driven role mapping**. Pipeline stages reference abstract roles (`"fast"`, `"reasoning"`) instead of concrete model IDs.

**Resolution chain:**
1. Stage sets `agent_config.model_role = "fast"`
2. GenaiProvider looks up `MODEL_FAST` env var → `gpt-4o-mini`
3. Falls back to the role string itself (treated as explicit model name) if no env mapping
4. Falls back to provider's default model if no role set

**Builder API:** `stage("classify", "agent").model_role("fast").done()`

**Why it matters:** Pipeline definitions become provider-agnostic and environment-portable. Same pipeline config works across dev (cheap models) and prod (best models) by changing env vars.

## 7. Strengths for Agentic Orchestration

1. **Deterministic execution** — Expression-based routing with first-match-wins, plus optional agent-determined routing via `Router` nodes for cases where LLM judgment is the right choice.
2. **Resource safety** — Built-in bounds on LLM calls, agent hops, edge traversals, context tokens.
3. **Parallel-native** — Fork/join is a first-class topology node, not bolted on.
4. **Performance** — Rust core = <5ms orchestration overhead vs. 50-100ms in Python frameworks.
5. **Checkpoint durability** — Crash recovery and migration at instruction boundaries.
6. **Schema-driven state** — Declarative merge strategies prevent shared-state bugs.
7. **Federation** — CommBus enables cross-pipeline communication without external brokers.

## 8. Weaknesses & Risks

1. **Community & ecosystem** — Private/small repo vs. 20-50K star competitors. No plugin marketplace.
2. **No distributed mode** — Single-actor-per-process. No multi-node clustering.
3. **Expression-only routing** — Complex business logic requires gate chains or Router nodes; more verbose than Python lambdas (though Router nodes now bridge the gap for LLM-directed decisions).
4. **Rust learning curve** — Extending the core requires Rust expertise.
5. **No built-in retries** — Must implement via routing loops and `max_visits`.
6. **LLM provider support** — Multi-provider via `genai` crate (OpenAI, Anthropic, Google, Cohere). New model role resolver maps semantic roles (`fast`, `reasoning`) to concrete models via `MODEL_*` env vars, enabling provider-agnostic pipeline definitions.
7. **No built-in RAG** — No vector store, retrieval, or embedding integrations.

## 9. Verdict

### Use Jeeves-Core if you need:
- Deterministic, bounded multi-agent pipelines (not "let the LLM figure it out")
- Sub-millisecond orchestration overhead at scale
- Checkpoint/resume durability for long-running workflows
- Human-in-the-loop approval gates with automatic expiry
- Parallel agent execution with join strategies
- Cross-pipeline federation without external message brokers
- Rust-level reliability with Python developer experience

### Use LangGraph instead if you need:
- Largest community and ecosystem, most production battle-testing
- LLM-driven dynamic routing
- Tight integration with LangChain tools/retrievers/embeddings
- LangGraph Cloud for managed deployment

### Use CrewAI instead if you need:
- Fastest time-to-prototype for role-based multi-agent systems
- Non-technical stakeholders defining agent "crews"
- Developer speed over determinism

### Use Claude/OpenAI Agent SDK instead if you need:
- Deep integration with a specific model provider
- Simplest possible API surface (3-5 concepts)
- Model-native tool use without abstraction layers

### Bottom Line

Jeeves-Core is a **production-grade orchestration runtime** that trades community size and ecosystem breadth for performance, determinism, and resource safety. Architecturally it's closer to a **Temporal for AI agents** than a LangChain alternative. If your agentic use case demands predictable execution, resource bounds, and durability, it's worth the investment. If you need rapid prototyping, broad model support, or a large plugin ecosystem, LangGraph or CrewAI will get you moving faster.
