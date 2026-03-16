# Jeeves Core — Framework Evaluation Audit

**Date:** 2026-03-16
**Scope:** Architecture, extensibility, code quality, competitive landscape

---

## 1. What It Is

**Jeeves Core** is a Rust-based micro-kernel for multi-agent AI orchestration (~19K LOC, 56 source files). It's a **general directed-graph pipeline runtime** with a Unix-inspired process model — not a prompt library, not a wrapper SDK. It ships as:

- **PyO3 Python module** (primary): `from jeeves_core import PipelineRunner`
- **Rust crate**: `use jeeves_core::prelude::*`
- **MCP stdio server** (optional): JSON-RPC tool federation

**Version:** 0.0.2 | **License:** Apache 2.0

---

## 2. Architecture Deep Dive

### Core Abstractions

| Concept | What It Does |
|---------|-------------|
| **Envelope** | State container flowing through the pipeline — inputs, outputs, bounds, audit trail |
| **PipelineConfig** | Graph definition: ordered stages with routing rules, bounds, state schema |
| **PipelineStage** | Node in the graph with a `NodeKind`: `Agent`, `Gate`, `Fork`, or `Router` |
| **Kernel** | Single-actor owner of all state — no locks, uses mpsc channels |
| **ProcessControlBlock** | Unix-like metadata: state, priority, quotas, usage tracking |
| **Instruction** | Kernel→Worker commands: `RunAgent`, `RunAgents`, `Terminate`, `WaitInterrupt`, `WaitParallel` |

### Orchestration Pattern — Microkernel

```
Python/MCP → KernelHandle (typed channel) → mpsc queue → Kernel Actor (single &mut)
                                                               ↓
                                                      Agent Tasks (concurrent tokio)
                                                      ├── LlmAgent (ReAct tool loop)
                                                      ├── McpDelegatingAgent (tool-only)
                                                      ├── PipelineAgent (child pipeline / A2A)
                                                      └── DeterministicAgent (passthrough)
```

Key properties:
- **Single mutable kernel** — no concurrent access, no locks needed
- **Concurrent agent execution** — tokio tasks communicate back via channel
- **Process lifecycle**: `NEW → READY → RUNNING → WAITING → TERMINATED → ZOMBIE (GC'd)`
- **Priority queue** scheduling with per-process resource quotas

### Graph Topology — General Directed Graph (not just DAG)

The orchestrator advances stages **by name lookup, not index**, meaning routing targets can point forward, backward, or to themselves. This makes Jeeves a **general directed-graph runtime with bounded cycle support**, not merely a DAG executor.

#### Cyclic Routing & Loop Control

Cycles are first-class with 5 layers of termination guarantees:

| Guard | Scope | What it does |
|-------|-------|-------------|
| `max_iterations` | Global | Caps total iterations across the entire pipeline run |
| `max_agent_hops` | Global | Caps total stage-to-stage transitions |
| `max_visits` | Per-stage | Caps how many times a specific stage can be entered |
| `edge_limits` | Per-edge | Caps how many times a specific A→B transition can fire |
| `stage_visits` | Per-stage | HashMap counter incremented on every entry, feeds the guards above |

**Self-loops** (`default_next` pointing to the same stage) are allowed but validation requires `max_visits` to be set — unbounded self-loops are rejected at definition time (`orchestrator_types.rs:358-365`). Multi-stage cycles (A→B→A or longer rings) are unrestricted and bounded by global limits.

This is tested extensively: backward cycles terminated by bounds, edge-limit enforcement, self-loops with max_visits, gate self-loops, conditional exit from temporal loops — 8+ dedicated cycle tests in `orchestrator.rs:1261-1790`.

### Routing Engine (13 operators, 5 field scopes)

Routing is expression-tree based with max depth 16:
- **Operators**: `Eq`, `Neq`, `Gt`, `Lt`, `Gte`, `Lte`, `Contains`, `Exists`, `NotExists`, `And`, `Or`, `Not`, `Always`
- **Scopes**: `Current` (this agent's output), `Agent` (cross-agent ref), `Meta` (envelope metadata), `Interrupt` (user response), `State` (pipeline accumulator)

### Node Kinds (Graph Primitives)

| Kind | Behavior |
|------|----------|
| **Agent** | Standard execution — runs the agent, then evaluates routing rules on the output |
| **Gate** | Pure routing — evaluates routing rules WITHOUT executing any agent (zero-cost branching) |
| **Fork** | Parallel fan-out — evaluates ALL matching rules and dispatches agents concurrently |
| **Router** | Agent-determined routing — the agent itself picks from a declared set of targets |

### State Management

Three merge strategies for pipeline state accumulation:
- `Replace` — overwrite (default)
- `Append` — array accumulation (multi-hop RAG across loop iterations)
- `MergeDict` — shallow object merge (parallel fork outputs)

Plus full **checkpoint/resume** at instruction boundaries (serializable snapshots including stage_visits, edge_traversals, and parallel group state).

---

## 3. Extensibility & Integrations

### LLM Providers (via `genai` crate)

8 backends out of the box: OpenAI, Anthropic, Google Gemini, Ollama (local), Groq, DeepSeek, Cohere, xAI/Grok — all via env-var API keys. Model roles (`MODEL_FAST`, `MODEL_REASONING`) resolve at runtime. Custom providers via the `LlmProvider` trait. Automatic retry with exponential backoff on 429/5xx/network errors.

### Tool System

```rust
#[async_trait]
pub trait ToolExecutor: Send + Sync + Debug {
    async fn execute(&self, name: &str, params: Value) -> Result<Value>;
    fn list_tools(&self) -> Vec<ToolInfo>;
    fn requires_confirmation(&self, name: &str, params: &Value) -> Option<ConfirmationRequest>;
}
```

- **Builder pattern** composition via `ToolRegistryBuilder`
- **ACL filtering** per-stage (`allowed_tools` in pipeline config)
- **Confirmation gates** for destructive operations — triggers human-in-the-loop interrupt
- **MCP integration**: HTTP and stdio transports with auto-discovery via `tools/list` JSON-RPC
- **ReAct tool loop**: up to `max_tool_rounds` (default 10) iterations per agent execution

### Agent Extension

Implement the `Agent` trait, register with `AgentRegistry`. Four built-in types auto-selected from pipeline config:

| Agent | Selected When |
|-------|--------------|
| **LlmAgent** | `has_llm: true` — full ReAct loop with streaming |
| **McpDelegatingAgent** | `has_llm: false` + `allowed_tools` — tool dispatch only |
| **DeterministicAgent** | `has_llm: false`, no tools, or `node_kind: Gate` — passthrough |
| **PipelineAgent** | `child_pipeline` set — runs a child pipeline as a stage (A2A composition) |

### Inter-Pipeline Communication

Kernel-mediated **CommBus** with three patterns:
- **Events** (pub/sub) — fan-out to all subscribers
- **Commands** (fire-and-forget) — routed to single handler
- **Queries** (request/response) — timeout-aware RPC

Pipelines subscribe via config: `"subscriptions": ["game.event", "world.update"]`, `"publishes": ["npc.response"]`.

### Streaming

Token-level streaming via `PipelineEvent`:
```
StageStarted → Delta (token-by-token) → ToolCallStart → ToolResult → StageCompleted → Done
```

Python consumer:
```python
for event in runner.stream("hello", user_id="user1"):
    match event["type"]:
        case "delta": print(event["content"], end="")
        case "done": print(event["outputs"])
```

### Human-in-the-Loop

Tools can declare `requires_confirmation()`. When triggered:
1. Agent suspends with `InterruptPending` event
2. Pipeline yields `WaitInterrupt` instruction
3. Consumer resolves via `ResolveInterrupt`
4. Pipeline resumes with `interrupt_response` available in routing scope

### Configuration

- **Pipeline config**: JSON with JSON Schema validation (`schema/pipeline.schema.json`)
- **Environment variables**: LLM keys, MCP servers, bounds defaults, rate limits, log format
- **Prompt templates**: Directory-backed `.txt` files with `{var}` placeholders, or in-memory overrides
- **Output validation**: Optional `output_schema` (JSON Schema) per stage

---

## 4. Code Quality & Maturity

| Aspect | Rating | Evidence |
|--------|--------|---------|
| **Tests** | Excellent | 349 tests, 0 failures, 75% coverage minimum enforced in CI via cargo-tarpaulin |
| **CI/CD** | Excellent | 3-job pipeline: check (fmt + clippy + compile), test (tarpaulin + codecov), audit (cargo-audit) |
| **Safety** | Excellent | `unsafe_code = "deny"`, `RUSTFLAGS="-Dwarnings"`, zero `unwrap()` in lib code, zero unsafe blocks outside PyO3 FFI |
| **Error Handling** | Excellent | Typed `Error` enum (10 variants) via `thiserror`, gRPC-style error codes, full source chains, convenience constructors |
| **Docs** | Very Good | 922+ lines across README, API_REFERENCE, DEPLOYMENT, CONSTITUTION; inline `//!` on all public modules |
| **Tech Debt** | Zero | 0 TODO/FIXME/HACK/XXX comments found across entire codebase |
| **Observability** | Good | `tracing` + `#[instrument]` throughout, optional OpenTelemetry behind `otel` feature flag |
| **Type Safety** | Excellent | Strongly-typed IDs (`ProcessId`, `EnvelopeId`), `#[must_use]` on important returns, `Result<T>` everywhere |
| **Dependencies** | Lean | Minimal production deps: tokio, reqwest, serde, genai, thiserror, tracing. Security audited in CI |

### Maturity Concerns

- **Version 0.0.2** — pre-production by any measure
- **Single contributor** (50 commits over ~5 days) — bus factor of 1
- **No community yet** — no external issues, PRs, or documented adopters
- **Rapid iteration** (~10 releases in 5 days) — architecture may still shift
- **75% coverage** is solid but not exceptional; critical paths (orchestrator, routing) appear well-covered but edge cases in MCP/CommBus less so

---

## 5. Competitive Landscape (March 2026)

| Framework | Abstraction | Language | LLM Lock-in | Stars | Maturity | Orchestration |
|-----------|------------|----------|-------------|-------|----------|--------------|
| **Jeeves Core** | General directed graph + microkernel | Rust (PyO3 Python) | None (8 providers) | New | Pre-alpha (0.0.2) | Cyclic graph with expression-tree routing, 5-layer bounds |
| **LangGraph** | Stateful graphs | Python, TS | None | ~25K | Production (600+ cos) | Graph state machine with conditional edges, cycles via send() |
| **CrewAI** | Role-based crews | Python | None | ~46K | Production (PwC, IBM) | Sequential/parallel/hierarchical crews + Flows |
| **MS Agent Framework** | Conversation + graph | Python, C#, Java | Azure-first | ~55K | RC → GA Q1 2026 | LLM-driven + deterministic workflows, A2A + MCP |
| **Claude Agent SDK** | Agent loop + tools | Python, TS | Claude only | ~5K | v0.1.x | Single loop + subagent delegation |
| **OpenAI Agents SDK** | Agents + handoffs | Python, TS | OpenAI-first | ~19K | Moderate | Linear handoff-based, no persistence |
| **DSPy** | Declarative modules | Python | None | ~28K | Moderate-High | Module composition + auto-optimization |
| **Pydantic AI** | Type-safe agents | Python | None | ~16K | Moderate-High (v1.68) | Agent-with-tools + deep agents extension |
| **smolagents** | Code agents | Python | None | ~16K | Moderate | Manager → sub-agent, code-as-action |

### Head-to-Head: Jeeves Core vs. LangGraph (closest competitor)

| Dimension | Jeeves Core | LangGraph |
|-----------|------------|-----------|
| **Runtime** | Rust (native perf, zero-cost abstractions) | Python (interpreted, GIL) |
| **Graph model** | General digraph with 5-layer bounded cycles | StateGraph with conditional edges, cycles via `send()` |
| **Routing** | 13-operator expression trees evaluated in Rust | Python functions as edge conditions |
| **Node types** | 4 built-in kinds (Agent, Gate, Fork, Router) | Generic nodes (any Python callable) |
| **Parallelism** | Fork node + tokio tasks (true concurrency) | Fan-out via `send()` (async, GIL-bound) |
| **State** | Typed merge strategies (Replace, Append, MergeDict) | Reducers on state channels |
| **Persistence** | Checkpoint/resume (bring your own storage) | Built-in with SQLite/Postgres, durable execution |
| **Human-in-the-loop** | Interrupt service + confirmation gates | `interrupt_before`/`interrupt_after` on nodes |
| **Ecosystem** | MCP tools only, no pre-built integrations | Full LangChain ecosystem (1000+ integrations) |
| **Deployment** | Library (embed in your runtime) | LangGraph Platform (managed, auto-scaling) |
| **Maturity** | 5 days, 1 contributor | 2+ years, 600+ production companies |

### Head-to-Head: Jeeves Core vs. CrewAI

| Dimension | Jeeves Core | CrewAI |
|-----------|------------|--------|
| **Abstraction** | Graph stages with explicit routing | Role-based crews with implicit delegation |
| **Control flow** | Developer-defined expression trees | Framework-managed (sequential, hierarchical, consensus) |
| **Prototyping speed** | Slower (Rust compilation, JSON config) | Fast (Python decorators, role descriptions) |
| **Tools** | Bring your own + MCP | 100+ pre-built tools |
| **Loops** | Native cyclic routing with 5 bounds layers | Built into crew execution modes |
| **Observability** | tracing + OpenTelemetry | CrewAI dashboard + LangSmith |

### Head-to-Head: Jeeves Core vs. Vendor SDKs (Claude Agent SDK, OpenAI Agents SDK)

| Dimension | Jeeves Core | Vendor SDKs |
|-----------|------------|-------------|
| **LLM flexibility** | 8 providers, model-role abstraction | Locked to vendor (Claude-only / OpenAI-first) |
| **Orchestration** | Explicit graph with routing rules | Implicit (agent loop decides next action) |
| **Multi-agent** | Pipeline stages + Fork + CommBus federation | Subagents / handoffs |
| **Control** | Full control over execution flow | LLM-driven; less deterministic |
| **Complexity** | Higher (graph definition, Rust) | Lower (just define tools and go) |

---

## 6. Strengths — What Jeeves Core Does Better Than Alternatives

1. **Performance ceiling.** Rust-native with zero unsafe code, single-actor kernel with no locks, tokio true concurrency. No other framework in this space is Rust-native. For latency-sensitive workloads (game NPCs, real-time orchestration, high-throughput SLM pipelines), this is a material advantage that Python frameworks cannot match.

2. **Graph expressiveness with safety.** General directed-graph topology with cyclic routing is more expressive than most competitors. The 5-layer bounded cycle system (`max_iterations`, `max_agent_hops`, `max_visits`, `edge_limits`, `stage_visits`) provides stronger termination guarantees than LangGraph's `recursion_limit` or CrewAI's `max_iter`. Self-loops require explicit `max_visits` at definition time — the framework won't let you create an unbounded loop.

3. **Architectural rigor.** The microkernel pattern with Unix-like process lifecycle, priority-queue scheduling, per-user rate limiting, and resource quotas per process is unique in this space. This isn't just an agent runner — it's a process scheduler for AI workloads.

4. **Fork/Gate/Router primitives.** Four node kinds cover the full spectrum: zero-cost branching (Gate), parallel fan-out (Fork), deterministic routing (Agent with rules), and LLM-determined routing (Router). Expression-tree routing with 13 operators and 5 field scopes evaluated in Rust is faster and more type-safe than Python lambda conditions.

5. **Multi-provider by default.** 8 LLM backends with model-role abstraction (`MODEL_FAST`, `MODEL_REASONING`). Mix providers within a single pipeline — use GPT-4o-mini for classification, Claude for reasoning, Ollama for local inference. No vendor lock-in.

6. **MCP + CommBus federation.** Cross-pipeline pub/sub communication plus MCP tool integration positions it well for the A2A/MCP convergence trend. PipelineAgent enables true A2A composition — pipelines as stages within other pipelines.

7. **Zero-tolerance safety culture.** `unsafe_code = "deny"`, zero `unwrap()` in lib code, typed error enum with gRPC error codes, bounded channels, 349 tests with 75% coverage enforcement, cargo-audit in CI. This is production-quality Rust engineering.

---

## 7. Weaknesses — Real Risks & Gaps

1. **Pre-alpha maturity.** Single contributor, v0.0.2, ~5 days old. Every competitor has months-to-years of production hardening, community contributions, and battle-tested edge cases. This is the single biggest risk. Architecture may still shift fundamentally.

2. **No ecosystem.** Zero pre-built tools, no hub/marketplace, no community extensions. CrewAI ships 100+ tools. LangGraph has the entire LangChain ecosystem (1000+ integrations). Jeeves has MCP integration (which helps bridge this gap), but you'll build most integrations yourself.

3. **Rust competency required.** The PyO3 Python bindings are functional for consuming pipelines, but extending the kernel, adding new node kinds, debugging orchestration issues, or contributing upstream requires Rust expertise. Every other framework is Python/TS-native with lower onboarding friction.

4. **No auto-optimization.** DSPy auto-tunes prompts. LangGraph has LangSmith for tracing-driven optimization. Jeeves has tracing and OpenTelemetry but no feedback loop for automatically improving agent performance over time.

5. **No durable execution out of the box.** Checkpoint/resume exists but requires you to build the external persistence layer. LangGraph Platform ships durable execution with auto-recovery and Postgres backing. For production, you'd need to pair Jeeves with your own storage and recovery logic.

6. **No managed deployment.** LangGraph has LangGraph Platform. CrewAI has CrewAI AMP. Jeeves is a library — you own deployment, scaling, monitoring, and lifecycle management entirely.

7. **Documentation is good but untested by outsiders.** The docs read well, but with zero external users, there's no validation that the getting-started path, error messages, and edge-case behaviors actually work for someone who isn't the author.

---

## 8. Recommendation Matrix

| If your use case is... | Use Jeeves Core? | Better alternative |
|------------------------|------------------|--------------------|
| **Game NPC dialogue / real-time agents** | **Yes, strong fit.** Rust performance, low-latency kernel, streaming, cyclic routing for dialogue loops, Fork for parallel NPC behaviors. | — |
| **SLM orchestration (local models)** | **Yes, good fit.** Ollama support, resource quotas prevent runaway inference, bounded channels, model-role abstraction for mixing local/cloud. | — |
| **Iterative agentic loops (research, RAG)** | **Yes, good fit.** Native cyclic routing with Append state merge for accumulating results across iterations. 5-layer bounds prevent runaway. | LangGraph (if ecosystem matters more) |
| **Performance-critical orchestration runtime** | **Yes, unique fit.** Only Rust-native option in the space. True concurrency via tokio, no GIL. | — |
| **Multi-pipeline federation / A2A** | **Yes, good fit.** CommBus + MCP + PipelineAgent composition is ahead of most frameworks. | LangGraph (if you need maturity) |
| **Enterprise production agentic workflows** | **No, not yet.** Too early, no community, no durable execution, no managed deployment. | LangGraph or MS Agent Framework |
| **Rapid prototyping / hackathons** | **No.** Rust compilation overhead, JSON config, no pre-built tools. | CrewAI or OpenAI Agents SDK |
| **Type-safe Python agent dev** | **No.** PyO3 bridge adds indirection vs. native Python typing. | Pydantic AI |
| **Prompt optimization / ML research** | **No.** No optimization primitives. | DSPy |
| **Claude/OpenAI-specific autonomous agents** | **No.** Vendor SDKs are simpler when you're locked to one provider. | Claude Agent SDK / OpenAI Agents SDK |
| **Enterprise .NET / Azure shop** | **No.** No C#/.NET support. | MS Agent Framework / Semantic Kernel |

---

## 9. Bottom Line

**Jeeves Core is architecturally impressive and fills a real gap.** There is no other Rust-native, multi-provider, general-directed-graph agent orchestration kernel in the OSS landscape. The microkernel design, bounded cyclic routing with 5-layer termination guarantees, expression-tree routing, Unix-like process lifecycle, and zero-unsafe-code discipline are genuinely differentiated. The graph model is more expressive than most competitors — it's not a DAG, it's a full directed graph with safety rails.

**But it's very early with a single contributor.** For production enterprise use today, you'd be taking on significant risk: no community support, no battle-tested edge cases, no ecosystem of pre-built integrations, no managed deployment platform.

**The right framing:** Jeeves Core is a **high-potential runtime kernel** worth adopting if your use case demands Rust-level performance, multi-provider flexibility, and cyclic graph orchestration — particularly for game AI, real-time agents, SLM orchestration, or performance-critical pipelines where Python framework overhead is unacceptable. It is **not yet a substitute** for LangGraph/CrewAI/MS Agent Framework if you need production reliability, ecosystem breadth, managed deployment, or team onboarding ease today.

**If you adopt it now**, plan to:
- (a) Invest Rust engineering capacity for kernel-level extensions
- (b) Build your own tool integrations (MCP helps bridge this)
- (c) Build a persistence layer around checkpoint/resume for durability
- (d) Contribute back to accelerate maturity and reduce bus-factor risk
- (e) Pair with external monitoring (OpenTelemetry support is there, wire it up)
