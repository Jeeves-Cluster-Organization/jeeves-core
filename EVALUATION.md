# Jeeves Core — Evaluation & OSS Audit

**Date:** 2026-03-17
**Subject:** Should your company adopt Jeeves Core for agentic orchestration?
**Approach:** Audit jeeves-core against major open-source alternatives on architecture, safety, maturity, and fit.

---

## Executive Summary

Jeeves Core is a **Rust micro-kernel for multi-agent AI orchestration** (v0.0.2, Apache-2.0). It provides pipeline orchestration, agent execution, resource bounding, human-in-the-loop interrupts, and inter-pipeline communication — consumable as a Rust library, PyO3 Python module, or MCP stdio server.

**Bottom line:** Jeeves Core occupies a unique niche — a low-level, high-performance orchestration kernel with defense-in-depth safety — but is very early-stage (v0.0.2) with no visible community or production track record. Most teams should use a mature OSS framework today and only consider Jeeves Core for specific high-performance or embedded-runtime use cases.

---

## 1. Jeeves Core Profile

| Dimension | Details |
|-----------|---------|
| **Language** | Rust (with PyO3 Python bindings, MCP stdio mode) |
| **Version** | 0.0.2 (pre-release) |
| **License** | Apache-2.0 |
| **Codebase** | ~17,700 lines Rust, 348 tests |
| **LLM Support** | OpenAI-compatible APIs via `genai` crate (configurable base URL) |
| **Orchestration** | Sequential, conditional branching, parallel fan-out/join, loop-back, nested pipelines (A2A), gate routing |
| **State** | Envelope-based with schema-driven merge (Replace, Append, MergeDict) |
| **Safety/Bounds** | max_iterations, max_llm_calls, max_agent_hops, max_visits (per-stage), edge_limits, timeouts, rate limiting, quotas |
| **Human-in-the-loop** | First-class interrupt/resume (Clarification, Confirmation, AgentReview, Checkpoint) |
| **Observability** | tracing + optional OpenTelemetry, audit trail in envelope |
| **Community** | No visible public community, GitHub stars, or third-party adoption |
| **Dependencies** | Minimal: tokio, reqwest, serde, tracing (no heavy framework deps) |

---

## 2. OSS Alternatives Comparison

### 2.1 LangGraph (LangChain)

| Dimension | Details |
|-----------|---------|
| **Language** | Python, TypeScript |
| **GitHub Stars** | ~10k+ |
| **License** | MIT |
| **Architecture** | Stateful, cyclic graph execution engine built on LangChain primitives |
| **Orchestration** | DAG + cycles, conditional edges, parallel branches, subgraphs, human-in-the-loop |
| **LLM Support** | All providers via LangChain (OpenAI, Anthropic, Google, local models, etc.) |
| **State** | Checkpointed state with reducers (similar to Redux); persistence backends (SQLite, Postgres, Redis) |
| **Safety** | Recursion limits, breakpoints, time-travel debugging |
| **Deployment** | LangGraph Platform (managed), self-hosted, or embedded |
| **Maturity** | Production-used at scale; strong docs, LangSmith observability |

**Strengths:** Largest ecosystem, best LLM provider coverage, persistence/checkpointing, LangSmith tracing, active community, good docs.
**Weaknesses:** Python GIL limits true parallelism, LangChain abstraction overhead, vendor lock-in toward LangSmith/LangGraph Platform, can be over-abstracted for simple use cases.

### 2.2 CrewAI

| Dimension | Details |
|-----------|---------|
| **Language** | Python |
| **GitHub Stars** | ~25k+ |
| **License** | MIT |
| **Architecture** | Role-based multi-agent framework; agents have roles, goals, backstories |
| **Orchestration** | Sequential, hierarchical (manager delegates), conditional |
| **LLM Support** | OpenAI, Anthropic, local via LiteLLM |
| **State** | Shared memory between agents (short-term + long-term via RAG) |
| **Safety** | Max iterations per agent, delegation controls |
| **Deployment** | CrewAI Enterprise (managed) or self-hosted |
| **Maturity** | Popular for prototyping; enterprise tier available |

**Strengths:** Intuitive role-based metaphor, fast prototyping, strong community, easy to learn.
**Weaknesses:** Limited orchestration patterns (no true DAG/graph), weak state management, less control over execution flow, production-hardening concerns at scale.

### 2.3 Microsoft AutoGen / Agent Framework

| Dimension | Details |
|-----------|---------|
| **Language** | Python, .NET (C#) |
| **GitHub Stars** | ~38k+ (combined AutoGen repo) |
| **License** | MIT |
| **Architecture** | v0.4 rewrite: actor-based, event-driven, layered (Core → AgentChat → Extensions) |
| **Orchestration** | Conversational (multi-agent chat), group chat patterns, custom topologies |
| **LLM Support** | OpenAI, Azure OpenAI, Anthropic, Google, local |
| **State** | Distributed via actor model; state per agent |
| **Safety** | Termination conditions, max turns, human-in-the-loop |
| **Deployment** | Local, distributed (gRPC actors), Azure integration |
| **Maturity** | Major v0.4 rewrite disrupted ecosystem; AG2 fork complicates story |

**Strengths:** Microsoft backing, distributed actor model scales well, .NET support unique, strong research pedigree.
**Weaknesses:** Community fractured (AG2 fork vs Microsoft path), v0.4 breaking changes, complex API surface, migration burden.

### 2.4 OpenAI Agents SDK (formerly Swarm)

| Dimension | Details |
|-----------|---------|
| **Language** | Python |
| **GitHub Stars** | ~15k+ |
| **License** | MIT |
| **Architecture** | Minimalist: agents with instructions + tools + handoffs |
| **Orchestration** | Agent handoffs (sequential delegation), tool use, guardrails |
| **LLM Support** | OpenAI only (by design) |
| **State** | Context variables passed between handoffs |
| **Safety** | Guardrails (input/output validation), max turns |
| **Deployment** | Embedded in Python apps; OpenAI Responses API integration |
| **Maturity** | Official OpenAI product; simple but limited |

**Strengths:** Simplest mental model, official OpenAI support, built-in tracing, guardrails pattern.
**Weaknesses:** OpenAI lock-in (no other providers), no parallel execution, no graph/DAG patterns, limited state management, not suitable for complex orchestration.

### 2.5 Google Agent Development Kit (ADK)

| Dimension | Details |
|-----------|---------|
| **Language** | Python |
| **GitHub Stars** | ~12k+ |
| **License** | Apache-2.0 |
| **Architecture** | Hierarchical agent trees with Gemini-native integration |
| **Orchestration** | Sequential, parallel, loop agents, custom agents |
| **LLM Support** | Gemini (native), LiteLLM bridge for others |
| **State** | Session-based state management, artifact storage |
| **Safety** | Callbacks for pre/post processing, evaluation framework |
| **Deployment** | Vertex AI Agent Engine, Cloud Run, self-hosted |
| **Maturity** | Released April 2025; growing but Google-ecosystem focused |

**Strengths:** Deep Google Cloud integration, A2A protocol support, built-in eval framework, good for Gemini shops.
**Weaknesses:** Gemini-centric (others via bridge), Google ecosystem lock-in, newer project, smaller community outside Google.

### 2.6 Semantic Kernel (Microsoft)

| Dimension | Details |
|-----------|---------|
| **Language** | C#/.NET, Python, Java |
| **License** | MIT |
| **Architecture** | Plugin-based kernel with AI service abstraction |
| **Orchestration** | Planners (sequential, stepwise), process framework (state machines) |
| **LLM Support** | OpenAI, Azure OpenAI, Hugging Face, local |
| **State** | Kernel memory, context variables |
| **Maturity** | Production-ready; merging with AutoGen into "Microsoft Agent Framework" |

**Strengths:** Enterprise .NET ecosystem, strong Azure integration, plugin model, multi-language.
**Weaknesses:** Merging with AutoGen creates uncertainty, less focused on multi-agent patterns, complex API.

---

## 3. Head-to-Head Comparison Matrix

| Capability | Jeeves Core | LangGraph | CrewAI | AutoGen 0.4 | OpenAI Agents | Google ADK |
|---|---|---|---|---|---|---|
| **Language** | Rust (+Python) | Python/TS | Python | Python/.NET | Python | Python |
| **Performance** | Native speed | Python-limited | Python-limited | Python (distributed) | Python-limited | Python-limited |
| **DAG/Graph orchestration** | Yes (routing fns) | Yes (native) | No | Partial | No | Partial |
| **Parallel fan-out** | Yes (Fork + Join) | Yes | No | Yes (actors) | No | Yes |
| **Nested pipelines** | Yes (PipelineAgent) | Yes (subgraphs) | Hierarchical only | Yes (nested groups) | Handoffs only | Yes (sub-agents) |
| **Human-in-the-loop** | First-class interrupts | Breakpoints | Limited | Yes | No | Callbacks |
| **State persistence** | In-memory only | Checkpointed (DB) | In-memory | Actor state (distributed) | Context vars | Session store |
| **LLM providers** | OpenAI-compatible | All major | All via LiteLLM | All major | OpenAI only | Gemini + bridge |
| **Cost/bound controls** | Comprehensive (5+ types) | Recursion limit | Max iterations | Max turns | Max turns | Callbacks |
| **Observability** | tracing + OTel | LangSmith | Basic logging | Built-in tracing | Built-in tracing | Cloud Trace |
| **Inter-pipeline comms** | CommBus (pub/sub) | No | No | Actor messaging | No | A2A protocol |
| **Tool ACL** | Per-stage whitelist | No | No | No | No | No |
| **Community** | None visible | Large | Very large | Large (fractured) | Growing | Growing |
| **Maturity** | Pre-release (v0.0.2) | Production | Production | Transitioning | Production | Early production |
| **License** | Apache-2.0 | MIT | MIT | MIT | MIT | Apache-2.0 |

---

## 4. Where Jeeves Core Differentiates

### 4.1 Genuine Strengths (vs OSS field)

1. **Defense-in-depth bounding** — No OSS framework matches its layered safety controls (max_iterations, max_llm_calls, max_agent_hops, max_visits per stage, edge_limits, timeouts, rate limiting, quotas). Most frameworks offer only a recursion/turn limit.

2. **Performance** — Rust native with zero `unsafe` code. Single-process actor model on tokio avoids Python GIL limitations. Meaningful for high-throughput or latency-sensitive workloads.

3. **Micro-kernel architecture** — Clean separation: kernel handles orchestration only, no domain logic. This is architecturally sounder than monolithic frameworks that bundle prompts, tools, and orchestration.

4. **Tool ACL per stage** — Unique capability. No major OSS framework restricts which tools are visible to which stage. Important for security-sensitive pipelines.

5. **Inter-pipeline communication** — CommBus (events, commands, queries) enables pipeline federation. Only AutoGen's actor model and Google's A2A protocol offer comparable cross-agent communication.

6. **Human-in-the-loop as a first-class primitive** — Typed interrupts with auto-expiry (Clarification: 24h, Confirmation: 1h, etc.) and resume flow. More structured than LangGraph's breakpoints.

7. **Routing as code** — Named closures rather than expression trees. More powerful and testable than JSON-based routing in other frameworks.

### 4.2 Genuine Weaknesses (vs OSS field)

1. **Pre-release maturity (v0.0.2)** — No production track record, no visible users, no community. This is the single biggest risk factor.

2. **Single LLM provider** — Only OpenAI-compatible APIs via `genai` crate. No native Anthropic, Google, or local model support. Every major competitor supports multiple providers.

3. **No state persistence** — In-memory envelope only. No checkpointing, no crash recovery, no distributed state. LangGraph and AutoGen are far ahead here.

4. **No ecosystem** — No pre-built tools, no prompt libraries, no integrations, no marketplace. You build everything yourself.

5. **Python bindings are secondary** — PyO3 works but the primary experience is Rust. Most AI/ML teams work in Python; the ergonomic gap matters.

6. **No managed deployment** — No hosted offering, no deployment guides, no Kubernetes patterns. LangGraph Platform, CrewAI Enterprise, and Google Vertex all offer managed options.

7. **Small bus factor** — "Jeeves Team" with no visible contributors. If the maintainers walk away, you own a Rust codebase with no community to fall back on.

8. **Documentation gap** — Constitution and README exist, but no tutorials, guides, or example applications.

---

## 5. Decision Framework: When Should You Use Jeeves Core?

### Use Jeeves Core IF:

- You need **embedded orchestration in a Rust or performance-critical system** (game engine, real-time system, edge device)
- You have a **Rust-capable team** that can maintain and extend the kernel
- You need **strict execution bounds and cost controls** that go beyond what OSS frameworks offer
- You need **tool-level access control per pipeline stage** for security compliance
- You're building a **platform** where the orchestration kernel is infrastructure, not application code
- You're willing to **build the capability layer** (tools, prompts, integrations) yourself
- You need **MCP server aggregation** with orchestration on top

### Do NOT Use Jeeves Core IF:

- You need **production-ready today** — use LangGraph or OpenAI Agents SDK
- You need **broad LLM provider support** — use LangGraph or CrewAI
- You're a **Python-first team** without Rust expertise — use any Python framework
- You need **state persistence and crash recovery** — use LangGraph
- You need **an ecosystem of pre-built tools and integrations** — use LangChain/LangGraph
- You need **managed deployment** — use LangGraph Platform, CrewAI Enterprise, or Google Vertex
- You want **community support and battle-tested patterns** — use LangGraph or AutoGen
- You're **prototyping quickly** — use CrewAI or OpenAI Agents SDK

---

## 6. Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| Project abandonment (v0.0.2, unknown team) | **High** | Apache-2.0 lets you fork; but Rust maintenance cost is real |
| No production validation | **High** | Extensive test suite (348 tests) helps, but no substitute for production |
| Single LLM provider | **Medium** | OpenAI-compatible base URL supports many providers (Anthropic via proxy, vLLM, etc.) |
| No persistence/recovery | **Medium** | Acceptable for stateless request-response; blocker for long-running workflows |
| Rust talent requirement | **Medium** | PyO3 bindings help; but debugging requires Rust knowledge |
| No security audit | **Medium** | Zero unsafe code is a strong foundation; but no formal audit |
| API instability ("no backward compatibility" policy) | **Medium** | Constitution explicitly states no backward compat — expect breaking changes |

---

## 7. Recommended Alternatives by Use Case

| Your Use Case | Recommended Framework | Why |
|---|---|---|
| **General-purpose multi-agent** | LangGraph | Best ecosystem, state management, observability |
| **Quick prototyping** | CrewAI or OpenAI Agents SDK | Fastest time-to-demo |
| **OpenAI-only shop** | OpenAI Agents SDK | Simplest, official support |
| **Google Cloud / Gemini shop** | Google ADK | Native integration |
| **.NET / Enterprise** | Semantic Kernel | Microsoft ecosystem, multi-language |
| **Distributed multi-agent** | AutoGen 0.4 | Actor model, gRPC scaling |
| **High-performance / embedded** | **Jeeves Core** | Only Rust option with proper orchestration |
| **Strict safety/cost controls** | **Jeeves Core** | Most comprehensive bounding system |
| **Security-sensitive pipelines** | **Jeeves Core** | Tool ACL, process isolation |

---

## 8. Conclusion

Jeeves Core is **architecturally impressive but commercially premature**. Its Rust micro-kernel design, defense-in-depth bounding, tool ACL, and inter-pipeline communication are genuinely differentiated against a field of Python frameworks that focus on ecosystem breadth over runtime safety.

**For most companies today:** Use LangGraph as your primary orchestration framework. It has the broadest ecosystem, best state management, and strongest community.

**Consider Jeeves Core if** you're building infrastructure where performance, safety bounds, or tool-level access control are non-negotiable requirements — and you have the Rust expertise to maintain it through its pre-release phase.

**Watch Jeeves Core if** you're interested in the architectural direction. The micro-kernel approach (separating orchestration from capability) is a fundamentally better architecture than monolithic agent frameworks. If the project matures and builds community, it could become the performance tier of the orchestration landscape.
