# Jeeves-Core vs. OSS Agentic Frameworks — Full Comparative Audit

**Date:** 2026-03-11 (Full re-audit)
**Scope:** Comprehensive comparison of jeeves-core against 15 major OSS agentic AI frameworks
**Codebase revision:** Branch `claude/audit-agentic-frameworks-B6fqd` — includes MCP, A2A, PromptRegistry, CLI scaffold, observability refactor, gateway hardening

---

## 1. Executive Summary

Jeeves-Core has the **strongest systems architecture** of any OSS agentic framework — it is the only one treating agents as OS-managed processes with a Rust microkernel enforcing resource bounds, typed interrupts, and expression-based routing with compile-time safety guarantees. With **15,748 lines of Rust** and **32,138 lines of Python** (21,528 runtime + 10,610 tests) across 194 files, it is a fully implemented dual-layer runtime.

**What changed since last audit:**
- **MCP support added** (528 lines) — MCPClientAdapter + MCP server implementation. Jeeves-core is no longer the only framework without MCP.
- **A2A protocol support added** (437 lines) — Agent-to-agent client/server via HTTP with `/.well-known/agent.json` discovery. Only MS Agent Framework and Google ADK also support A2A among competitors.
- **PromptRegistry concrete implementation** — satisfies the AgentPromptRegistry protocol. Previous gap closed.
- **CLI scaffold module** (242 lines) — project generation and scaffolding for new capabilities.
- **Rust kernel grew +963 lines** (+6.5%) to 15,748 lines with 289 inline tests.

**Critical remaining gap:** The string-based LLM interface (`generate(prompt: str) → str`) is now the **only critical gap**. Every competitor uses message-based chat APIs. MCP and A2A — previously the #1 and #6 gaps — are now resolved.

**Industry landscape (March 2026):** MCP is universal. MS Agent Framework reached RC (GA targeting end of Q1 2026). Google ADK expanded to 4 languages. OpenAI Agents SDK added WebSocket transport and sessions. LlamaIndex Workflows shipped as standalone package. Letta introduced V1 architecture with Context Repositories. New entrants: PydanticAI, Agno (formerly Phidata), Mastra.

---

## 2. Jeeves-Core Architecture Inventory (Full Re-Inventory)

**Total codebase:** 15,748 lines Rust (40 files) + 21,528 lines Python (101 files) + 10,610 lines tests (53 test files) = **47,886 lines across 194 files**

### 2.1 Rust Kernel — 15,748 Lines, 289 Inline Tests

| Module | Key Files | Lines | Tests | Description |
|--------|-----------|-------|-------|-------------|
| **Kernel Core** | `mod.rs`, `orchestrator.rs`, `lifecycle.rs`, `interrupts.rs`, `routing.rs`, `services.rs`, `resources.rs`, `cleanup.rs`, `recovery.rs`, `orchestrator_types.rs`, `types.rs` | 9,044 | 162 | Process lifecycle, pipeline orchestration (2,165 lines, 60 tests), 7 typed interrupt kinds (1,041 lines, 16 tests), expression routing (619 lines, 18 tests), rate limiting, resource tracking, zombie GC |
| **IPC Layer** | `server.rs`, `router.rs`, `codec.rs`, 7 handler files | 2,247 | 25 | TCP+msgpack server, 27+ RPC methods across kernel/orchestration/commbus/interrupt/tools/services/validation handlers |
| **Tools** | `catalog.rs`, `health.rs`, `access.rs` | 1,597 | 30 | Tool registry with risk classification, sliding-window health tracking + circuit breaking, agent→tool access control |
| **Envelope** | `mod.rs`, `enums.rs` | 1,125 | 25 | Bounded pipeline state with semantic sub-structs (Identity, Pipeline, Bounds, Interrupts, Execution, Audit) |
| **CommBus** | `mod.rs` | 849 | 12 | Pub/sub, fire-and-forget commands, request/response queries with timeout |
| **Events** | `translation.rs`, `mod.rs` | 224 | 7 | Kernel event → external event translation |
| **Types** | `config.rs`, `errors.rs`, `ids.rs` | 471 | — | Runtime configuration, error types, strongly-typed IDs |
| **Other** | `lib.rs`, `main.rs`, `observability.rs` | 127 | 1 | Entry points, tracing setup |

**Key properties:** `#[deny(unsafe_code)]`, single-actor kernel (`&mut self`), panic recovery via `with_recovery()` wrapper.

### 2.2 Python Runtime — 21,528 Lines, 101 Files

#### 2.2.1 Protocols & Interfaces — 2,997 Lines

| Module | Lines | Key Types |
|--------|-------|-----------|
| `protocols/types.py` | 1,071 | Envelope, PipelineConfig, AgentConfig, GenerationParams, `stage()` shorthand, `chain()`, `graph()` builders |
| `protocols/capability.py` | 932 | CapabilityResourceRegistry (11 registration methods), CapabilityToolCatalog |
| `protocols/interfaces.py` | 462 | 18 runtime-checkable protocols |
| `protocols/routing.py` | 136 | Route building and matching |
| `protocols/_generated.py` | 119 | Auto-generated Rust enum parity |
| `protocols/events.py` | 88 | Event, EventCategory, EventSeverity |

#### 2.2.2 Pipeline & Agent Execution — 2,291 Lines

| Module | Lines | Key Responsibilities |
|--------|-------|----------------------|
| `runtime/agents.py` | 764 | Config-driven Agent dataclass, PipelineRunner, PipelineRegistry, **PromptRegistry** (NEW) |
| `pipeline_worker.py` | 574 | Kernel-driven execution; `run_kernel_loop()` free function; streaming support |
| `runtime/capability_service.py` | 399 | Base class for kernel-driven capabilities with lifecycle hooks |
| `runtime/persistence.py` | 155 | DatabasePersistence — state save/load |

#### 2.2.3 Gateway HTTP Interface — 2,198 Lines (Hardened)

| Component | Lines | Endpoints/Features |
|-----------|-------|-------------------|
| `gateway/app.py` | 388 | FastAPI with CORS wildcard rejection, body size limiting (1MB), Prometheus middleware, OTEL instrumentation |
| `gateway/routers/chat.py` | 605 | POST /messages, GET /stream (SSE), /sessions CRUD |
| `gateway/routers/interrupts.py` | 339 | Interrupt CRUD + response workflow |
| `gateway/routers/health.py` | 214 | Health/readiness probes, tool health summary |
| `gateway/sse.py` | 191 | Server-Sent Events streaming |
| `gateway/event_bus.py` | 191 | Event aggregation and fan-out |

#### 2.2.4 MCP Module (NEW) — 528 Lines

| Component | Lines | Description |
|-----------|-------|-------------|
| `mcp/client.py` | 319 | MCPClientAdapter — wrap MCP servers as ToolProtocol sources |
| `mcp/server.py` | 199 | MCP protocol server — expose capabilities as MCP resources |

#### 2.2.5 A2A Module (NEW) — 437 Lines

| Component | Lines | Description |
|-----------|-------|-------------|
| `a2a/client.py` | 169 | Remote agent invocation via HTTP |
| `a2a/server.py` | 254 | Expose local agent via HTTP + `/.well-known/agent.json` agent card |

#### 2.2.6 CLI Module (NEW) — 242 Lines

| Component | Lines | Description |
|-----------|-------|-------------|
| `cli/scaffold.py` | 236 | Project scaffolding and code generation |

#### 2.2.7 LLM Providers — 1,539 Lines

| Provider | Lines | Description |
|----------|-------|-------------|
| `llm/providers/mock.py` | 460 | Configurable mock for testing |
| `llm/providers/litellm_provider.py` | 299 | LiteLLM adapter (90+ model backends) |
| `llm/providers/openai_http_provider.py` | 294 | OpenAI-compatible HTTP API |
| `llm/factory.py` | 158 | Provider factory pattern |
| `llm/cost_calculator.py` | 206 | Token cost estimation |
| `llm/providers/base.py` | 127 | LLMProvider ABC, TokenChunk |

**Interface:** `generate(model, prompt, options) → str` + `generate_stream()` — string-based, not message-based chat.

#### 2.2.8 Kernel Client — 1,320 Lines

`kernel_client.py`: Async TCP+msgpack client with 27+ RPC methods mapping to all kernel IPC services.

#### 2.2.9 Orchestrator & Events — 1,540 Lines

Events subsystem with `EventContext`, `PipelineEvent`, `StageEvent`, agent-specific event emission, and `GovernanceService` for risk assessment.

#### 2.2.10 Tools — 690 Lines

`@tool` decorator, `ToolExecutionCore` (validate, execute, record health), `ToolCatalog` for discovery and metadata.

#### 2.2.11 Memory & State — 1,067 Lines

SessionStateService (in-memory), ToolHealthService (sliding-window metrics), ToolMetricsRepository (persistence).

#### 2.2.12 Observability — 387 Lines (Refactored)

OTEL adapter (138 lines, consolidated from 576), Prometheus metrics (228 lines, 10 named metrics including 4 kernel-unique).

#### 2.2.13 Other Modules

| Module | Lines | Purpose |
|--------|-------|---------|
| Distributed (Redis bus) | 455 | Redis pub/sub for distributed events |
| Configuration & Bootstrap | 999 | AppContext, Settings, wiring, feature flags |
| Logging | 255 | structlog setup, context binding |
| Middleware | 163 | Rate limiting, body size limiting |
| Database & Redis | 742 | DB client, Redis async wrapper + connection manager |
| IPC (Python) | 335 | Protocol constants, msgpack framing |
| Utils | 879 | JSON repair, serialization, datetime, strings, testing helpers |
| Testing helpers | 531 | Mock kernel, test pipeline, routing eval |

### 2.3 Test Suite — 10,610 Lines, 53 Files

| Category | Files | Lines | Coverage |
|----------|-------|-------|----------|
| Unit tests | 29 | 8,061 | Agent execution, bootstrap, kernel client, pipeline worker, tool executor, protocols, gateway, LLM providers, Redis bus |
| Protocol parity | 5 | 674 | Rust↔Python enum, schema, method, quota, wire constant alignment |
| Integration | 3 | 106 | Kernel IPC round-trips |
| Config & fixtures | 17 | 1,769 | Test configuration, markers, mock infrastructure |

---

## 3. Recent Changes Since Last Audit

| Change | Before | After |
|--------|--------|-------|
| MCP support | **None** (critical gap) | Client + server (528 lines) |
| A2A support | **None** (medium gap) | Client + server (437 lines) |
| PromptRegistry | Protocol only, no implementation | **Concrete implementation** in `runtime/agents.py` |
| CLI tooling | None | Scaffold module (242 lines) |
| Rust kernel | 14,785 lines | **15,748 lines** (+963, +6.5%) |
| Inline Rust tests | ~280 | **289** (+9) |
| Python runtime | ~31,128 lines (incl. tests) | **32,138 lines** (21,528 + 10,610 tests) |
| OTEL adapter | 576 lines (over-engineered) | 138 lines (thin wrapper) |
| `tracing.py` | Existed | **Deleted** — consolidated into `otel_adapter.py` |
| `protocols/validation.py` | Existed | **Deleted** — validation moved to handlers |
| Gateway security | Basic | CORS wildcard rejection, body size limiting, Prometheus middleware |
| Pipeline worker | Class-based | `run_kernel_loop()` free function + `execute_streaming()` |
| Kernel metrics | Not wired | 4 kernel-unique metrics wired |

---

## 4. Competitor Framework Profiles (March 2026)

### 4.1 LangGraph (LangChain)

- **Version:** 1.1 | **Stars:** ~24.6k | **MCP:** Yes | **Languages:** Python, JS/TS, Java (LangGraph4j v1.8.7)
- **Architecture:** Low-level graph-based orchestration. StateGraph with conditional edges, checkpointing, human-in-the-loop breakpoints.
- **2026 updates:** v2 streaming with typed `StreamPart` dicts and `GraphOutput` objects. v1 default maintained for backward compatibility. Pluggable sandbox integrations (Modal, Daytona, Runloop). Used by Uber, LinkedIn, Klarna.
- **Ecosystem:** 900+ integrations, LangSmith for tracing/evaluation.
- **Maturity:** Production-stable. MIT license.

### 4.2 CrewAI

- **Version:** ~0.152 | **Stars:** 45.6k | **MCP:** Yes (native + Enterprise MCP Server) | **Language:** Python
- **Architecture:** Role-based crews + event-driven Flows. Fully independent from LangChain.
- **Memory:** 4-layer (short-term, long-term, entity, contextual) — most comprehensive built-in memory.
- **Enterprise:** CrewAI AMP — unified control plane, real-time observability, on-prem/VPC.
- **Community:** 100k+ certified developers. Latest release Feb 26, 2026.

### 4.3 Microsoft Agent Framework

- **Version:** RC (Feb 19, 2026, targeting 1.0 GA end of Q1 2026) | **Stars:** ~38k (legacy) | **MCP:** Yes | **Languages:** Python, .NET
- **Architecture:** Merges AutoGen's multi-agent abstractions with Semantic Kernel's enterprise features. Graph-based workflows (sequential, concurrent, handoff, group chat) with streaming, checkpointing, and human-in-the-loop.
- **Protocols:** A2A, AG-UI, and MCP. Multi-provider (Azure OpenAI, Anthropic, Bedrock, Ollama).
- **Status:** AutoGen in maintenance mode. Semantic Kernel continues but new features target Agent Framework.

### 4.4 OpenAI Agents SDK

- **Version:** 0.11.1 (Mar 9, 2026) | **Stars:** ~19k+ | **MCP:** Yes (stdio, SSE, streamable HTTP) | **Languages:** Python, TS/JS
- **Architecture:** 5 primitives — agents, handoffs, guardrails, sessions, tracing. Provider-agnostic (100+ LLMs).
- **2026 updates:** WebSocket transport (opt-in), sessions with Redis persistence, RealtimeAgent for voice, Python 3.14 compatibility, openai v2.x required.
- **Guardrails:** First-class tripwire (abort) and filter (transform) on input/output.

### 4.5 Haystack (deepset)

- **Version:** 2.25.2 (Mar 5, 2026) | **Stars:** ~24.4k | **MCP:** Yes (MCPToolset + Hayhooks MCP server) | **Language:** Python
- **Architecture:** Context-engineering-first. Modular pipelines with explicit control. Agent with breakpoint debugging and AgentSnapshot resumption.
- **2026 updates:** EmbeddingBasedDocumentSplitter (semantic splitting), multi-query retrieval expansion, Python 3.10+ required.

### 4.6 LlamaIndex

- **Version:** Workflows 1.0 (standalone package) | **Stars:** ~47.6k | **MCP:** Yes | **Languages:** Python, TS
- **Architecture:** Event-driven async-first Workflows. AgentWorkflow for multi-agent orchestration.
- **2026 updates:** Workflows 1.0 separated into standalone package (`llama-index-workflows`). Typed Workflow State (Python + TS). Resource injection. Agent Client Protocol (ACP) integration. Workflow Debugger.

### 4.7 DSPy (Stanford NLP)

- **Version:** 3.1.3 (Feb 5, 2026) | **Stars:** ~28k | **MCP:** Yes | **Language:** Python
- **Architecture:** "Programming, not prompting." Automatic prompt optimization via optimizers.
- **2026 updates:** GEPA optimizer (reflective prompt evolution) with MLFlow integration, SIMBA optimizer, Python 3.14 support, reasoning LM support via `dspy.Reasoning`.

### 4.8 Claude Agent SDK (Anthropic)

- **Version:** 0.1.48 (Mar 7, 2026) | **MCP:** Yes (first-class, in-process servers) | **Languages:** Python, TS
- **Architecture:** Full Claude Code capabilities as SDK. Native MCP with OAuth 2.0.
- **2026 updates:** Renamed from Claude Code SDK. CLI auto-bundled. Fine-grained tool streaming fixes. VSCode, Xcode integrations. Voice STT support.
- **Status:** Alpha but rapidly maturing. Battle-tested runtime (powers Claude Code).

### 4.9 Google ADK

- **Version:** ~1.19 (Python), 0.6.0 (Java) | **Stars:** ~17.2k | **MCP:** Yes | **Languages:** Python, TS, Go, Java
- **Architecture:** Code-first + Agent Config. Workflow agents (Sequential, Parallel, Loop). Built-in evaluation.
- **2026 updates:** TypeScript and Go SDKs launched. Integrations ecosystem (AgentOps, Arize, MLflow, n8n, Hugging Face). Interactions API for reasoning chains. Service registry. Session rewind. Code execution sandbox.

### 4.10 Letta (MemGPT)

- **Version:** 0.16.4 (Jan 29, 2026) | **Stars:** ~21k | **Language:** Python
- **Architecture:** Memory-first platform. LLM-as-OS paradigm. Self-editing memory tools.
- **2026 updates:** V1 architecture (`letta_v1_agent`) — heartbeats deprecated, native reasoning only. Context Repositories with git-based versioning. Conversations API for shared memory across parallel user experiences. Remote Environments for Letta Code. #1 on Terminal-Bench for model-agnostic coding agents.

### 4.11 New Entrants (2025-2026)

| Framework | Focus | Key Differentiator |
|-----------|-------|-------------------|
| **PydanticAI** | Type-safe agents | Pydantic philosophy for AI — strong typing, validation, no magic. Deep Python ecosystem integration. |
| **Agno** (formerly Phidata) | Performance | ~2μs agent instantiation (~10,000× faster than LangGraph), ~3.75KiB memory. Native multimodality. Three multi-agent modes. |
| **Mastra** | TypeScript-first | From Gatsby team. Deploy to Vercel/Cloudflare/Netlify in one command. Built for frontend developers. Native OpenTelemetry. |

---

## 5. Full Comparison Matrix

| Dimension | Jeeves-Core | LangGraph | CrewAI | MS Agent Fw | OpenAI SDK | Haystack | LlamaIndex | DSPy | Claude SDK | Google ADK |
|-----------|-------------|-----------|--------|-------------|------------|----------|------------|------|------------|------------|
| **Language** | Rust + Python | Py, JS/TS, Java | Python | Python, .NET | Python, TS | Python | Python, TS | Python | Python, TS | Py, TS, Go, Java |
| **Version** | 0.0.1 | 1.1 | ~0.152 | RC→GA Q1 | 0.11.1 | 2.25.2 | Wf 1.0 | 3.1.3 | 0.1.48 | ~1.19 |
| **Stars** | Private | ~24.6k | 45.6k | ~38k | ~19k+ | ~24.4k | ~47.6k | ~28k | N/A | ~17.2k |
| **Agent Model** | Config-driven kernel processes | DAG state machine | Role-based crews | Multi-agent + graph | 5 primitives | Component pipeline | Event-driven Wf | Typed signatures | Agent loop | Code-first |
| **Resource Bounds** | Kernel-enforced | `recursion_limit` | Iteration limits | Configurable | Max turns | None | None | None | None | None |
| **MCP** | **Yes** (client+server) | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes (1st class) | Yes |
| **A2A** | **Yes** (client+server) | No | No | Yes | No | No | No | No | No | Yes |
| **IPC** | TCP+msgpack binary | In-process | In-process | In-process/gRPC | In-process | In-process | In-process | In-process | In-process | In-process |
| **Tool System** | Capability-scoped + risk + circuit break + MCP | Decorator | Built-in library | Plugin | `@function_tool` | Typed components | Fn calling + MCP | ReAct + MCP | Built-in + MCP | Tools + MCP |
| **Memory** | SessionState + ToolHealth | State + checkpoint | 4-layer | Session + middleware | Sessions (Redis) | Document stores | Multi-layer | Mem0 | Context/MCP | Session state |
| **Guardrails** | Hooks + risk + kernel bounds | None | None | Middleware | **1st class** tripwire/filter | None | None | None | Hooks | None |
| **Human-in-Loop** | **7 typed interrupts** | Breakpoints | Human input | Configurable | Guardrails | AgentSnapshot | None | None | None | None |
| **Streaming** | SSE + AUTH/DEBUG modes | Typed StreamPart | Callbacks | Streaming | WebSocket + HTTP | Async | Streaming | None | Streaming | Gemini Live |
| **CommBus** | **3-pattern** (pub/sub, cmd, query) | Shared state | Delegation | Shared state | Handoff context | Pipeline | Shared state | None | None | None |
| **Distributed** | Redis bus + A2A | None | None | .NET distributed | Redis sessions | None | None | None | None | Cloud Run |
| **Observability** | OTEL + Prometheus (10 metrics) | LangSmith | Callbacks | Telemetry | Tracing (default on) | None | OTEL | MLFlow | Tracing | Built-in eval |
| **LLM Interface** | **String prompt** | Message chat | Message chat | Message chat | Message chat | Message chat | Message chat | Typed sigs | Message chat | Message chat |
| **Maturity** | Early (0.0.1) | Production | Production | RC→GA | Production | Production | Production | Production | Alpha | Maturing |

---

## 6. When to Choose Each Framework

| Choose... | When you need... |
|-----------|-----------------|
| **Jeeves-Core** | Production-grade process isolation, kernel-enforced resource bounds, Rust safety guarantees, formal interrupt system, distributed execution via Redis + A2A federation. You're building a platform where runaway agents are unacceptable and you need OS-level controls. |
| **LangGraph** | Complex stateful graph workflows with the largest integration ecosystem (900+). Production-proven at Uber, LinkedIn, Klarna. Best for graph-shaped agent workflows with checkpointing. |
| **CrewAI** | Role-based multi-agent collaboration with minimal boilerplate. Fastest path from idea to working multi-agent system. Best built-in memory (4 layers). Largest community (45.6k stars). |
| **MS Agent Framework** | Enterprise .NET/Azure environments. Migration from AutoGen or Semantic Kernel. Multi-protocol (A2A + AG-UI + MCP). Targeting 1.0 GA by end of Q1 2026. |
| **OpenAI Agents SDK** | Rapid prototyping with any LLM (100+). Simplest API with best DX. Built-in tracing and first-class guardrails. WebSocket transport. Dual Python/TS. |
| **Haystack** | RAG and document processing with explicit control. Context-engineering-first. Semantic document splitting. Agent debugging with snapshot resumption. |
| **LlamaIndex** | Knowledge-intensive agents. Best-in-class RAG + growing agent capabilities via standalone Workflows. Typed state. Largest community (47.6k stars). |
| **DSPy** | Automatic prompt optimization (GEPA, SIMBA, MIPROv2). You want the system to find the best prompts. MLFlow integration for experiment tracking. |
| **Claude Agent SDK** | Claude-powered agents with native file/system access. First-class MCP with OAuth 2.0. Battle-tested runtime (powers Claude Code). |
| **Google ADK** | Multi-language team (4 languages). Google Cloud deployment. Built-in evaluation. Interactions API for reasoning chains. A2A protocol support. |
| **Letta** | Memory-first agents that persist and self-improve. Self-editing memory. Context Repositories with git versioning. #1 on Terminal-Bench. |
| **PydanticAI** | Type-safe agent development that feels like regular software engineering. No magic abstractions. Deep Python ecosystem integration. |
| **Agno** | Raw performance. ~2μs agent instantiation. Native multimodality. Lean and fast. |
| **Mastra** | TypeScript-first. Frontend developers building agents. One-command deploy to Vercel/Cloudflare/Netlify. |

---

## 7. Jeeves-Core Strengths (Unique Differentiators)

**No other OSS framework has these:**

### 7.1 Rust Microkernel with Compile-Time Safety
15,748 lines of Rust with `#[deny(unsafe_code)]`. No null derefs, no data races, no use-after-free. Single-actor kernel owns all mutable state. Every other framework is pure Python/TS with runtime-only guarantees.

### 7.2 Kernel-Enforced Resource Bounds
LLM calls, tokens, agent hops, iterations, tool calls, and timeout are checked at the Rust level. Bounds validated for positivity (zero/negative rejected). Cannot be bypassed from Python. No other framework has this.

### 7.3 Unix-Inspired Process Lifecycle
687 lines. Agents are kernel-managed processes with states (NEW→READY→RUNNING→WAITING→TERMINATED→ZOMBIE) and scheduling priorities (REALTIME/HIGH/NORMAL/LOW/IDLE). Background GC cleans up zombies (304 lines).

### 7.4 Seven Typed Interrupt Kinds
1,041 lines. CLARIFICATION (24h TTL), CONFIRMATION (1h), AGENT_REVIEW (30min), CHECKPOINT (no TTL), RESOURCE_EXHAUSTED (5min), TIMEOUT (5min), SYSTEM_ERROR (1h). Most frameworks have binary yes/no or nothing.

### 7.5 CommBus with Three Messaging Patterns
849 lines. Pub/sub (fan-out), commands (fire-and-forget), queries (request/response with timeout). All kernel-mediated. No other agent framework has a built-in 3-pattern message bus.

### 7.6 Binary IPC Protocol
2,247 lines across IPC layer. TCP + msgpack with length-prefixed framing (5MB max). 27+ RPC methods across 7 handlers. Cross-language contract tests (674 lines) catch schema drift at CI.

### 7.7 Expression-Based Routing
619 lines. 13 operators (Eq, Neq, Gt, Lt, Gte, Lte, Contains, Exists, NotExists, And, Or, Not, Always) with 4 field scopes (Current, Agent, Meta, Interrupt). Evaluated in Rust.

### 7.8 Config-Driven Agents
764 lines. No subclassing. Behavior via `AgentConfig` flags + hooks. Three streaming modes (OFF/DEBUG/AUTHORITATIVE).

### 7.9 Tool Risk Classification + Circuit Breaking
1,597 lines. Risk semantics (critical→benign), severity (destructive→passive), per-agent access grants, sliding-window health tracking, automatic circuit breaking.

### 7.10 MCP + A2A Protocol Support (NEW)
965 lines combined. Both client and server implementations. Jeeves-Core can consume MCP tools, expose capabilities as MCP resources, invoke remote A2A agents, and expose agents via A2A. Among OSS frameworks, only MS Agent Framework and Google ADK also support both MCP + A2A.

### 7.11 Kernel-Unique Observability Metrics
10 named Prometheus metrics including 4 kernel-unique: PIPELINE_TERMINATIONS (by terminal reason), KERNEL_INSTRUCTIONS (by instruction type), AGENT_EXECUTION_DURATION (histogram), TOOL_EXECUTIONS (by tool and outcome).

---

## 8. Jeeves-Core Gaps (vs. Competition)

### 8.1 Critical Gap (1 remaining)

| Gap | Impact | Who Does It Better | Remediation |
|-----|--------|--------------------|-------------|
| **String-prompt LLM interface** | CRITICAL — `generate(prompt: str) → str` has no message-based chat, no tool schemas, no multi-modal, no structured output | Every other framework supports message-based APIs | Add `generate_chat(messages, tools) → ChatResponse` to LLMProviderProtocol |

### 8.2 Previously Critical Gaps — Now Resolved

| Gap | Previous Status | Current Status |
|-----|----------------|----------------|
| **No MCP support** | CRITICAL — only framework without it | **RESOLVED** — client + server (528 lines) |
| **No A2A support** | MEDIUM — emerging standard | **RESOLVED** — client + server (437 lines) |
| **No PromptRegistry** | MEDIUM — protocol without implementation | **RESOLVED** — concrete implementation added |

### 8.3 High-Impact Gaps

| Gap | Impact | Who Does It Better |
|-----|--------|--------------------|
| **No documentation** | HIGH — no tutorials, no API reference, no getting-started | LangChain, CrewAI, OpenAI SDK, Google ADK |
| **Rust binary requirement** | HIGH — `pip install && go` vs. compile Rust + run kernel binary | Every other framework is `pip install` |
| **No built-in RAG** | HIGH — most common LLM application pattern | Haystack, LlamaIndex, LangChain |
| **No community/ecosystem** | HIGH — no integrations, no plugins, no contributors | CrewAI (45.6k), LlamaIndex (47.6k), LangGraph (900+ integrations) |
| **No evaluation framework** | HIGH — no way to measure agent quality | DSPy (automatic optimization), LangSmith, Google ADK |

### 8.4 Medium-Impact Gaps

| Gap | Impact | Who Does It Better |
|-----|--------|--------------------|
| **No formalized guardrails API** | MEDIUM — has hooks/risk but no structured violation reporting | OpenAI SDK (tripwire/filter), MS Agent Framework (middleware) |
| **No AG-UI protocol support** | MEDIUM — emerging agent-to-UI standard | MS Agent Framework |
| **Observability wired but passive** | MEDIUM — spans not actively emitted in agent code paths | OpenAI SDK (tracing default on), LangSmith |
| **In-memory session state only** | MEDIUM — no persistent conversation memory | CrewAI (4-layer), LlamaIndex (multi-layer), Letta (self-editing) |
| **No conversation management** | MEDIUM — each request is new Envelope | OpenAI SDK (sessions), Letta (Conversations API) |
| **Envelope complexity** | MEDIUM — 997 lines Rust + 1,071 lines Python | LangGraph (clean TypedDict), OpenAI SDK (simple RunContext) |

---

## 9. Industry Trends (March 2026)

### 9.1 MCP is Universal
Every framework supports MCP. Jeeves-core now does too. This is table stakes, no longer a differentiator.

### 9.2 Microsoft Consolidation
AutoGen + Semantic Kernel → Microsoft Agent Framework (RC, targeting GA end of Q1 2026). Teams should migrate to the unified framework.

### 9.3 Multi-Language Expansion
Google ADK leads with 4 languages (Python, TS, Go, Java). OpenAI SDK has Python/TS parity. LangGraph has Python/TS/Java. Single-language frameworks are disadvantaged.

### 9.4 Memory-First Architectures
Letta's self-editing memory, Context Repositories, and V1 architecture represent the cutting edge. CrewAI's 4-layer memory is the best in traditional frameworks. Memory is a key differentiator.

### 9.5 Built-In Evaluation
Google ADK and DSPy lead with built-in evaluation. LangSmith provides evaluation externally. Frameworks without evaluation tooling are harder to use in production.

### 9.6 Agent Protocols Consolidating
A2A (Agent-to-Agent), AG-UI (Agent-to-UI), ACP (Agent Client Protocol) are maturing alongside MCP. MS Agent Framework and Google ADK support multiple protocols. Jeeves-core now supports MCP + A2A.

### 9.7 Performance Emerging as Differentiator
Agno's ~2μs agent instantiation (~10,000× faster than LangGraph) shows that performance matters for high-throughput agent systems. Jeeves-Core's binary IPC is a performance advantage that could be benchmarked.

### 9.8 TypeScript-First Frameworks
Mastra (from Gatsby team) represents a TS-first approach targeting frontend developers. This is a new market segment.

### 9.9 Observability Consolidation
OpenTelemetry is the standard. Jeeves-core's OTEL + Prometheus stack is the right choice — it just needs active span emission throughout agent code paths.

---

## 10. Strategic Recommendations for Jeeves-Core

### 10.1 Critical Path (Must Do)

1. **Message-Based LLM Interface** — The last critical gap. Add `ChatMessage`, `ToolCall`, `ToolResult` types. Support `generate_chat(messages, tools, response_format)`. Keep string `generate()` for backward compatibility.

2. **Getting-Started Documentation** — Quickstart guide, architecture overview, first-capability tutorial. Without this, no external adoption is possible.

### 10.2 High Priority

3. **Simplify Onboarding** — Consider pre-built kernel binaries via PyPI wheels, or a pure-Python development mode.

4. **Built-In Evaluation** — Leverage existing OTEL + Prometheus + kernel-unique metrics to build an evaluation harness.

5. **Persistent Memory** — Implement conversation history on top of existing DatabaseClientProtocol + Redis infrastructure.

6. **Active OTEL Span Emission** — Wire up spans in `Agent.process()`, `PipelineRunner.run()`, `ToolExecutionCore.execute_tool()`, `PipelineWorker.execute()`.

### 10.3 Medium Priority

7. **AG-UI Protocol Support** — Complete the protocol trifecta (MCP + A2A + AG-UI).

8. **Structured Guardrails API** — Formalize existing hooks into guardrail protocol with violation reporting.

9. **Reference Capability** — Build a demonstrable capability (e.g., code analysis, research assistant) showcasing the platform's strengths.

---

## 11. Competitive Position Summary

```
                    ARCHITECTURAL RIGOR
                         ▲
                         │
            Jeeves-Core ●│
                         │
                         │          ● MS Agent Framework
                         │
                         │    ● LangGraph
                         │                    ● Google ADK
                         │  ● Haystack
                         │        ● Claude SDK
                         │  ● LlamaIndex
                         │    ● CrewAI     ● OpenAI SDK
                         │
                         │            ● DSPy
                         │  ● Letta         ● Agno
                         │       ● PydanticAI
                         │        ● BeeAI     ● Mastra
                         │                    ● AG2
                         └──────────────────────────────► ECOSYSTEM / ADOPTION
```

Jeeves-Core occupies the upper-left quadrant: highest architectural rigor, lowest ecosystem/adoption. With MCP + A2A, it has moved slightly rightward — it can now interoperate with the broader agent ecosystem.

**Bottom line:** Jeeves-core has the most rigorous agent runtime architecture of any OSS framework — 15,748 lines of `#[deny(unsafe_code)]` Rust kernel + 32,138 lines of Python with 18 protocols, kernel-enforced bounds, 7 typed interrupt kinds, 3-pattern message bus, MCP client+server, and A2A client+server. Three previously critical gaps (MCP, A2A, PromptRegistry) are now closed. **One critical gap remains: the string-prompt LLM interface.** After that, the path is: **docs → reference capability → community**.

---

*Audit conducted against branch `claude/audit-agentic-frameworks-B6fqd`, 2026-03-11. Full re-audit with comprehensive codebase re-inventory and competitor research.*
