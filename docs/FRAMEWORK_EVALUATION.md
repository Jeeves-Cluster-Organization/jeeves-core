# Jeeves Core — Framework Evaluation Audit

**Date:** 2026-03-16
**Scope:** Architecture, extensibility, code quality, competitive landscape

---

## 1. What It Is

**Jeeves Core** is a Rust-based micro-kernel for multi-agent AI orchestration (~19K LOC, 56 source files). It's a **DAG pipeline runtime** with a Unix-inspired process model — not a prompt library, not a wrapper SDK. It ships as:

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
| **PipelineConfig** | DAG definition: ordered stages with routing rules, bounds, state schema |
| **PipelineStage** | Node in the DAG with a `NodeKind`: `Agent`, `Gate`, `Fork`, or `Router` |
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

### Routing Engine (13 operators, 5 field scopes)

Routing is expression-tree based with max depth 16:
- **Operators**: `Eq`, `Neq`, `Gt`, `Lt`, `Gte`, `Lte`, `Contains`, `Exists`, `NotExists`, `And`, `Or`, `Not`, `Always`
- **Scopes**: `Current` (this agent's output), `Agent` (cross-agent ref), `Meta` (envelope metadata), `Interrupt` (user response), `State` (pipeline accumulator)

### State Management

Three merge strategies for pipeline state accumulation:
- `Replace` — overwrite (default)
- `Append` — array accumulation (multi-hop RAG)
- `MergeDict` — shallow object merge (parallel fork outputs)

Plus full **checkpoint/resume** at instruction boundaries (serializable snapshots).

---

## 3. Extensibility & Integrations

### LLM Providers (via `genai` crate)

OpenAI, Anthropic, Google Gemini, Ollama (local), Groq, DeepSeek, Cohere, xAI/Grok — all via env-var API keys. Model roles (`MODEL_FAST`, `MODEL_REASONING`) resolve at runtime.

### Tool System

```rust
#[async_trait]
pub trait ToolExecutor: Send + Sync + Debug {
    async fn execute(&self, name: &str, params: Value) -> Result<Value>;
    fn list_tools(&self) -> Vec<ToolInfo>;
    fn requires_confirmation(&self, name: &str, params: &Value) -> Option<ConfirmationRequest>;
}
```

- Builder pattern composition, ACL filtering per-stage, confirmation gates for destructive ops
- **MCP integration**: HTTP and stdio transports with auto-discovery via `tools/list`

### Agent Extension

Implement the `Agent` trait, register with `AgentRegistry`. Four built-in types auto-selected from config.

### Inter-Pipeline Communication

Kernel-mediated **CommBus** with pub/sub events, fire-and-forget commands, and request/response queries. Pipelines subscribe via config (`"subscriptions": ["game.event"]`).

### Streaming

Token-level streaming via `PipelineEvent`: `StageStarted → Delta → ToolCallStart → ToolResult → StageCompleted → Done`

---

## 4. Code Quality & Maturity

| Aspect | Rating | Evidence |
|--------|--------|---------|
| **Tests** | Excellent | 349 tests, 0 failures, 75% coverage enforced in CI |
| **CI/CD** | Excellent | 3-job pipeline: check (fmt+clippy+compile), test (tarpaulin+codecov), audit (cargo-audit) |
| **Safety** | Excellent | `unsafe_code = "deny"`, `RUSTFLAGS="-Dwarnings"`, zero `unwrap()` in lib code |
| **Error Handling** | Excellent | Typed `Error` enum (10 variants) via `thiserror`, gRPC-style error codes, full source chains |
| **Docs** | Very Good | 922 lines across README, API_REFERENCE, DEPLOYMENT, CONSTITUTION; inline `//!` on all modules |
| **Tech Debt** | Zero | 0 TODO/FIXME/HACK/XXX comments found |
| **Observability** | Good | `tracing` + `#[instrument]` throughout, optional OpenTelemetry |

### Concerns

- Version 0.0.2 — pre-production by any measure
- Single contributor — bus factor of 1
- No community yet — no issues, no PRs, no external adopters documented

---

## 5. Competitive Landscape (March 2026)

| Framework | Abstraction | Language | LLM Lock-in | Stars | Maturity | Orchestration |
|-----------|------------|----------|-------------|-------|----------|--------------|
| **Jeeves Core** | DAG pipeline + microkernel | Rust (PyO3 Python) | None (8+ providers) | New | Pre-alpha (0.0.2) | Stateful DAG with routing expressions |
| **LangGraph** | Stateful graphs | Python, TS | None | ~25K | Production (600+ cos) | Graph state machine with conditional edges |
| **CrewAI** | Role-based crews | Python | None | ~46K | Production (PwC, IBM) | Sequential/parallel/hierarchical crews |
| **MS Agent Framework** | Conversation + graph | Python, C#, Java | Azure-first | ~55K (AutoGen) | RC → GA Q1 2026 | LLM-driven + deterministic workflows |
| **Claude Agent SDK** | Agent loop + tools | Python, TS | Claude only | ~5K | v0.1.x | Single loop + subagent delegation |
| **OpenAI Agents SDK** | Agents + handoffs | Python, TS | OpenAI-first | ~19K | Moderate | Linear handoff-based |
| **DSPy** | Declarative modules | Python | None | ~28K | Moderate-High | Module composition + auto-optimization |
| **Pydantic AI** | Type-safe agents | Python | None | ~16K | Moderate-High (v1.68) | Agent-with-tools + subagents |
| **smolagents** | Code agents | Python | None | ~16K | Moderate | Manager → sub-agent delegation |

---

## 6. Where Jeeves Core Fits — Honest Assessment

### Genuine Strengths (vs. the field)

1. **Performance ceiling.** Rust with zero unsafe code, single-actor kernel with no locks, tokio concurrency. Nothing else in this list is Rust-native. For latency-sensitive workloads (game NPCs, real-time orchestration), this is a material advantage.

2. **Architectural rigor.** The microkernel pattern with typed channels, process lifecycle, bounded resource quotas, and checkpoint/resume is more OS-like than any competitor. LangGraph's graphs are the closest analogy, but Jeeves' process model adds scheduling, priority queues, and per-user rate limiting at the kernel level.

3. **Fork/Gate/Router primitives.** The 4-node-kind system (`Agent`, `Gate`, `Fork`, `Router`) with expression-tree routing is more expressive than CrewAI's role model or OpenAI's handoff model, and comparable to LangGraph's conditional edges — but evaluated in Rust, not Python.

4. **Multi-provider by default.** 8 LLM backends with model-role abstraction. No vendor lock-in, unlike Claude Agent SDK (Claude-only) or OpenAI Agents SDK (OpenAI-first).

5. **MCP + CommBus federation.** Cross-pipeline pub/sub plus MCP tool integration positions it well for the A2A/MCP convergence trend.

### Real Risks & Gaps

1. **Pre-alpha maturity.** 1 contributor, v0.0.2. Every competitor has months-to-years of production hardening, community contributions, and battle-tested edge cases. This is the single biggest risk.

2. **No ecosystem.** Zero pre-built tools, no hub/marketplace, no community extensions. CrewAI ships 100+ tools. LangGraph has the entire LangChain ecosystem. Jeeves has MCP integration (which helps), but you'll build everything else yourself.

3. **Python bindings are secondary.** The PyO3 bridge is functional but the primary API is Rust. Your team needs Rust competency to extend the kernel, debug issues, or contribute upstream. Every other framework is Python/TS-native.

4. **No auto-optimization.** DSPy auto-tunes prompts. LangGraph has LangSmith for tracing-driven optimization. Jeeves has tracing but no feedback loop for improving agent performance.

5. **No durable execution out of the box.** Checkpoint/resume exists but requires external persistence. LangGraph Platform ships durable execution with auto-recovery. For production, you'd need to build this layer.

6. **Documentation is good but untested.** The docs read well, but with zero external users, there's no validation that the getting-started path actually works for someone who isn't the author.

---

## 7. Recommendation Matrix

| If your use case is... | Use Jeeves Core? | Better alternative |
|------------------------|------------------|--------------------|
| **Game NPC dialogue / real-time agents** | **Yes, strong fit.** Rust performance, low-latency kernel, streaming, fork/gate primitives map well to game loops. | — |
| **SLM orchestration (local models)** | **Yes, good fit.** Ollama support, resource quotas prevent runaway local inference, bounded channels. | — |
| **Enterprise production agentic workflows** | **No, not yet.** Too early, no community, no durable execution. | LangGraph or MS Agent Framework |
| **Rapid prototyping / hackathons** | **No.** Rust compilation overhead, no pre-built tools. | CrewAI or OpenAI Agents SDK |
| **Type-safe Python agent dev** | **No.** PyO3 bridge adds indirection. | Pydantic AI |
| **Prompt optimization / ML research** | **No.** No optimization primitives. | DSPy |
| **Claude-specific autonomous agents** | **No.** Use the native SDK. | Claude Agent SDK |
| **Performance-critical orchestration runtime** | **Yes, unique fit.** Only Rust-native option in the space. | — |
| **Multi-pipeline federation / A2A** | **Yes, good fit.** CommBus + MCP + pipeline composition is ahead of most frameworks here. | LangGraph (if you need maturity) |

---

## 8. Bottom Line

**Jeeves Core is architecturally impressive and fills a real gap** — there is no other Rust-native, multi-provider, DAG-based agent orchestration kernel in the OSS landscape. The microkernel design, expression-tree routing, process lifecycle management, and zero-unsafe-code discipline are genuinely differentiated.

**But it's very early with a single contributor.** For production enterprise use today, you'd be taking on significant risk: no community support, no battle-tested edge cases, no ecosystem of pre-built integrations.

**The right framing is:** Jeeves Core is a **high-potential runtime kernel** worth watching (or contributing to) if your use case demands Rust-level performance, multi-provider flexibility, and DAG-based orchestration. It is **not yet a substitute** for LangGraph/CrewAI/MS Agent Framework if you need production reliability, ecosystem breadth, or team onboarding ease today.

**If you adopt it now**, plan to: (a) invest Rust engineering capacity, (b) build your own tool integrations, (c) contribute back to accelerate maturity, and (d) pair it with external persistence and monitoring.
