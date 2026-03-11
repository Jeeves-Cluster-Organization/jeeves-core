# Jeeves-Core vs. OSS Agentic Frameworks — Full Comparative Audit

**Date:** 2026-03-11
**Scope:** Comprehensive comparison of jeeves-core against 12 major OSS agentic AI frameworks
**Codebase revision:** Post-merge of `runtime/agents.py` and `runtime/__init__.py`

---

## 1. Executive Summary

Jeeves-Core has the **strongest systems architecture** of any OSS agentic framework — it is the only one treating agents as OS-managed processes with a Rust microkernel enforcing resource bounds, typed interrupts, and expression-based routing at compile-time safety guarantees. The new `runtime/agents.py` module completes the Python-side agent execution model with config-driven Agents, PipelineRunner orchestration, PromptRegistry, streaming, and pre/post hooks.

However, jeeves-core remains in a **"great engine, no dashboard"** state: architecturally superior but practically harder to adopt than competitors due to missing MCP support, string-based LLM interface (no message/chat API), no documentation or getting-started guides, and zero community ecosystem — all areas where every major competitor now excels.

**Critical industry shift (2026):** MCP is now universal — every framework in this audit supports it. Microsoft has consolidated AutoGen + Semantic Kernel into a new Agent Framework (RC). Google ADK has expanded to 4 languages. CrewAI leads community size at 45.6k stars.

---

## 2. Jeeves-Core Architecture Inventory (Post-Merge)

### 2.1 Rust Kernel — Fully Implemented

| Module | File | Status | Lines | Description |
|--------|------|--------|-------|-------------|
| Kernel core | `src/kernel/mod.rs` | FULL | ~1,100 | Single-actor owner of all subsystems |
| Process lifecycle | `src/kernel/lifecycle.rs` | FULL | ~300 | Unix-like state machine (NEW→READY→RUNNING→WAITING→TERMINATED→ZOMBIE) with priority scheduling |
| Resource tracking | `src/kernel/resources.rs` | FULL | ~100 | Per-process and per-user quota enforcement |
| Rate limiting | `src/kernel/rate_limiter.rs` | FULL | — | Sliding-window with burst support |
| Interrupts | `src/kernel/interrupts.rs` | FULL | ~200 | 7 typed interrupt kinds with TTL, status tracking, session queries |
| Orchestrator | `src/kernel/orchestrator.rs` | FULL | ~600+ | Pipeline session management, instruction generation, bounds enforcement |
| Routing | `src/kernel/routing.rs` | FULL | ~300 | Expression tree evaluation (13 operators, 4 field scopes) |
| Services | `src/kernel/services.rs` | FULL | — | Service registry with load/health tracking |
| Cleanup/GC | `src/kernel/cleanup.rs` | FULL | — | Background zombie/stale session cleanup |
| Recovery | `src/kernel/recovery.rs` | FULL | — | Panic recovery wrapper for fault isolation |

### 2.2 IPC Layer — Fully Implemented

| Component | File | Methods | Description |
|-----------|------|---------|-------------|
| TCP server | `src/ipc/server.rs` | — | Semaphore-controlled concurrent connections |
| Codec | `src/ipc/codec.rs` | — | Length-prefixed msgpack framing (5MB max) |
| Router | `src/ipc/router.rs` | — | Service-based dispatch |
| Kernel handler | `src/ipc/handlers/kernel.rs` | 14 | Process CRUD, quota, scheduling |
| Orchestration handler | `src/ipc/handlers/orchestration.rs` | 4 | Session init, next instruction, result reporting |
| Interrupt handler | `src/ipc/handlers/interrupt.rs` | 4 | Create/resolve/get/cancel interrupts |
| Tools handler | `src/ipc/handlers/tools.rs` | — | Tool catalog and access control RPC |
| Services handler | `src/ipc/handlers/services.rs` | — | Service registry RPC |
| CommBus handler | `src/ipc/handlers/commbus.rs` | 4 | Publish/command/query/subscribe |

### 2.3 CommBus — Fully Implemented

Location: `src/commbus/mod.rs`

Three messaging patterns:
- **Pub/sub:** Event type → subscriber list, fan-out delivery
- **Commands:** Fire-and-forget, single handler per command type
- **Queries:** Request/response with configurable timeout

Built-in statistics (BusStats), subscription receipts, pure in-process implementation.

### 2.4 Envelope — Fully Implemented

Location: `src/envelope/mod.rs` (~1,000+ lines)

Semantic sub-structs: Identity, Pipeline, Bounds, Interrupts, Execution, Audit. FlowInterrupt and InterruptResponse with builder methods. Source of truth for request state — kernel stores in `process_envelopes: HashMap<ProcessId, Envelope>`.

### 2.5 Tools Subsystem — Fully Implemented

| Component | Location | Description |
|-----------|----------|-------------|
| Catalog (Rust) | `src/tools/catalog.rs` | ParamType validation, ToolEntry with risk classifications |
| Access control (Rust) | `src/tools/access.rs` | Agent→tool grant/revoke/check via HashMap |
| Health (Rust) | `src/tools/health.rs` | Sliding-window metrics, circuit breaking |
| Catalog (Python) | `python/jeeves_core/tools/catalog.py` | ToolCatalogEntry, ToolDefinition, ToolCatalog registry |
| Decorator (Python) | `python/jeeves_core/tools/decorator.py` | `@tool` decorator with service injection |
| Executor (Python) | `python/jeeves_core/tools/executor.py` | Async execution with retry, error handling |

### 2.6 Python Runtime — Fully Implemented (NEW)

Location: `python/jeeves_core/runtime/agents.py` (~750 lines)

| Component | Description |
|-----------|-------------|
| **Agent** | Config-driven dataclass. `process()` for batch, `stream()` for token emission. Pre/post hooks, mock handlers. OTEL span wrapping. JSON repair for LLM output. |
| **PipelineRunner** | Builds agents from PipelineConfig. Validates output_key uniqueness and LLM factory requirements. Event context propagation. |
| **PipelineRegistry** | Multi-pipeline dispatch. Scoped agent lookup (pipeline_name + agent_name). |
| **PromptRegistry** | Template storage with `str.format_map` rendering. Override `_render()` for Jinja2. Protocol-compliant. |
| **CapabilityService** | Base class for kernel-driven services with lifecycle hooks (`_ensure_ready`, `_enrich_metadata`, `_on_result`, `_build_result`). |
| **create_envelope()** | Factory with RequestContext validation, UUID generation. |
| **create_pipeline_runner()** | Factory wiring config → runner with all dependencies. |

Agent features: LLM calls with usage tracking, tool execution with access control, deterministic tool dispatch, streaming with AUTHORITATIVE/DEBUG token modes, generic prompt context building from envelope outputs + metadata.

### 2.7 Python Protocols — 18 Runtime-Checkable Protocols

Location: `python/jeeves_core/protocols/interfaces.py`

LoggerProtocol, DatabaseClientProtocol, LLMProviderProtocol, ToolProtocol, ToolDefinitionProtocol, ToolRegistryProtocol, ClockProtocol, AppContextProtocol, SemanticSearchProtocol, DistributedBusProtocol, ToolExecutorProtocol, ConfigRegistryProtocol, AgentToolAccessProtocol, WebSocketManagerProtocol, EventBridgeProtocol, RequestContext, InterruptServiceProtocol, EventEmitterProtocol.

### 2.8 LLM Providers

| Provider | File | Description |
|----------|------|-------------|
| MockProvider | `llm/providers/mock.py` | Testing with configurable responses |
| OpenAIHTTPProvider | `llm/providers/openai_http_provider.py` | OpenAI-compatible HTTP API |
| LiteLLMProvider | `llm/providers/litellm_provider.py` | 50+ model backends |

**Interface:** `generate(model, prompt, options) → str` — string-based prompt, not message-based chat.

### 2.9 Other Subsystems

| Area | Status | Location |
|------|--------|----------|
| Gateway (FastAPI) | FULL | `python/jeeves_core/gateway/` — REST, SSE, WebSocket |
| Observability | PARTIAL | `python/jeeves_core/observability/` — OTEL adapter + Prometheus scaffolding, no active instrumentation |
| Memory | PARTIAL | `python/jeeves_core/memory/` — SessionStateService (in-memory), ToolHealthService |
| Persistence | FULL | `python/jeeves_core/runtime/persistence.py` — Envelope serialization |
| Database | FULL | `python/jeeves_core/database/client.py` — Async SQLite |
| Distributed | FULL | `python/jeeves_core/distributed/redis_bus.py` — Redis queue |
| Event Bridge | FULL | `python/jeeves_core/events/bridge.py` — Kernel→frontend translation |
| Tests | FULL | 240+ Rust unit tests, 50+ Python tests, cross-language contract tests |

---

## 3. Competitor Framework Profiles (March 2026)

### 3.1 LangGraph (LangChain)

- **Version:** 1.1 | **Stars:** 24.6k | **MCP:** Yes | **Languages:** Python, JS/TS
- **Architecture:** Low-level graph-based orchestration for stateful agents. StateGraph with conditional edges, checkpointing, human-in-the-loop breakpoints.
- **2026 updates:** LangChain v1.0 now builds all agents on LangGraph. Pluggable sandbox integrations (Modal, Daytona, Runloop). V2 streaming with typed `StreamPart` dicts.
- **Ecosystem:** 900+ integrations, LangSmith for tracing/evaluation, 4.2M+ monthly downloads.
- **Maturity:** Production-stable. MIT license. Used by Klarna, Replit, Elastic.

### 3.2 CrewAI

- **Version:** 0.152.0 | **Stars:** 45.6k | **MCP:** Yes (native + Enterprise MCP Server) | **Language:** Python
- **Architecture:** Role-based crews + event-driven Flows. Independent from LangChain.
- **Memory:** 4-layer (short-term, long-term, entity, contextual) — most comprehensive built-in memory of any framework.
- **Enterprise:** CrewAI AMP — unified control plane, real-time observability, on-prem/VPC deployment.
- **Community:** 100k+ certified developers, 1.4B+ automations run.

### 3.3 Microsoft Agent Framework (AutoGen + Semantic Kernel successor)

- **Version:** RC (Feb 19, 2026) | **Stars:** ~38k (legacy AutoGen) | **MCP:** Yes | **Languages:** Python, .NET
- **Architecture:** Combines AutoGen's multi-agent abstractions with Semantic Kernel's enterprise features. Graph-based workflows, session state, middleware, telemetry.
- **Status:** AutoGen is in maintenance mode (critical fixes only). Semantic Kernel continues updates but new work focused on Agent Framework.
- **Protocols:** Supports A2A, AG-UI, and MCP standards.

### 3.4 AG2 (Independent AutoGen Fork)

- **Version:** Active fork | **Stars:** 4.2k | **MCP:** Yes | **Language:** Python
- **Status:** Community-driven by volunteers from Google DeepMind, Meta, IBM. Good for research/prototyping, not recommended for production.

### 3.5 OpenAI Agents SDK

- **Version:** 0.11.1 (Mar 9, 2026) | **Stars:** ~19k+ | **MCP:** Yes (stdio, SSE, streamable HTTP) | **Languages:** Python, TS/JS
- **Architecture:** 5 primitives — agents, handoffs, guardrails, sessions, tracing. Extremely lightweight. Provider-agnostic (100+ LLMs).
- **Guardrails:** First-class tripwire (abort) and filter (transform) guardrails on input/output.
- **2026 updates:** WebSocket transport, Responses API (replacing Assistants API mid-2026), `ToolOutputTrimmer`.

### 3.6 Semantic Kernel

- **Version:** Python 1.40.0, .NET 1.73.0 | **Stars:** 27.3k | **MCP:** Yes | **Languages:** C#, Python, Java
- **Status:** Production-stable across 3 languages. Being merged into Microsoft Agent Framework. 9.8M NuGet downloads (.NET).

### 3.7 Haystack (deepset)

- **Version:** 2.24.1 | **Stars:** 24.4k | **MCP:** Yes (MCPToolset + Hayhooks MCP server) | **Language:** Python
- **Architecture:** Context-engineering-first. Modular pipelines with explicit control. Agent component with breakpoint debugging and AgentSnapshot resumption.
- **2026 updates:** Semantic document splitting (EmbeddingBasedDocumentSplitter), Hayhooks for deployment as REST/MCP.
- **Enterprise:** Haystack Enterprise Platform (managed cloud or self-hosted).

### 3.8 LlamaIndex

- **Version:** Workflows 2.15.0 | **Stars:** 47.6k | **MCP:** Yes (native client, LlamaCloud MCP server) | **Languages:** Python, TS
- **Architecture:** Event-driven async-first Workflows. AgentWorkflow for multi-agent orchestration with state management. ACP (Agent Client Protocol) integration.
- **2026 updates:** Workflows 1.0 separated into standalone package, `llamactl` for pre-built agent templates, Workflow Debugger.
- **RAG:** Best-in-class retrieval-augmented generation, now complemented by strong agent capabilities.

### 3.9 DSPy (Stanford NLP)

- **Version:** 3.1.0 | **Stars:** 28k | **MCP:** Yes | **Language:** Python
- **Architecture:** "Programming, not prompting." Automatic prompt optimization via optimizers (MIPROv2, GEPA, SIMBA). Typed declarative signatures.
- **2026 updates:** `dspy.Reasoning` for native reasoning model support, GEPA cached evals, SIMBA optimizer, Python 3.14 support.
- **Unique:** Only framework focused on automatic optimization rather than manual prompt engineering.

### 3.10 Claude Agent SDK (Anthropic)

- **Version:** 0.1.48 (Mar 7, 2026) | **MCP:** Yes (first-class, in-process servers) | **Language:** Python
- **Architecture:** Full Claude Code capabilities as SDK — file read/write, system commands, subagents, background tasks, plugins. Native MCP with OAuth 2.0 for remote servers.
- **2026 updates:** VSCode native MCP dialog, MCP startup performance improvements, binary content handling, Apple Xcode 26.3 integration.
- **Maturity:** Early but rapidly maturing. Battle-tested runtime (powers Claude Code).

### 3.11 Google ADK (Agent Development Kit)

- **Version:** ~1.19.0 | **Stars:** 17.2k | **MCP:** Yes | **Languages:** Python, TS, Go, Java
- **Architecture:** Code-first and no-code (Agent Config). Workflow agents (Sequential, Parallel, Loop). Built-in evaluation framework.
- **2026 updates:** TypeScript SDK launched, third-party integrations (AgentOps, Arize, MLflow, n8n, Hugging Face). Deploy to Cloud Run or Vertex AI.
- **Unique:** Only framework supporting 4 languages. A2A protocol support.

### 3.12 Additional Frameworks

| Framework | Stars | Status | Notes |
|-----------|-------|--------|-------|
| **BeeAI (IBM)** | 3.1k | Active/Community | Linux Foundation AI & Data. Framework-agnostic orchestration. YAML declarative workflows. IBM states it won't maintain — community-driven. |
| **Letta (MemGPT)** | 21k | Production | Memory-first platform. Self-editing memory, Context Repositories with git versioning, Agent File (.af) format. #1 on Terminal-Bench for model-agnostic coding agents. |

---

## 4. Full Comparison Matrix

| Dimension | Jeeves-Core | LangGraph | CrewAI | MS Agent Framework | OpenAI Agents SDK | Haystack | LlamaIndex | DSPy | Claude Agent SDK | Google ADK |
|-----------|-------------|-----------|--------|---------------------|-------------------|----------|------------|------|-----------------|------------|
| **Language** | Rust + Python | Python, JS/TS | Python | Python, .NET | Python, TS/JS | Python | Python, TS | Python | Python | Python, TS, Go, Java |
| **Version** | 0.0.1 | 1.1 | 0.152 | RC | 0.11.1 | 2.24.1 | 2.15.0 (wf) | 3.1.0 | 0.1.48 | ~1.19.0 |
| **Stars** | Private | 24.6k | 45.6k | ~38k (legacy) | ~19k+ | 24.4k | 47.6k | 28k | N/A | 17.2k |
| **Agent Model** | Config-driven kernel processes | DAG state machine | Role-based crews + Flows | Multi-agent + graph workflows | 5 primitives (agents, handoffs, guardrails, sessions, tracing) | Component pipeline + Agent | Event-driven @step Workflows | Declarative typed signatures | Agent loop + hooks + skills | Code-first + Agent Config |
| **Orchestration** | Kernel-managed pipeline graph with expression routing | StateGraph with conditional edges | Sequential/hierarchical + event Flows | Graph + session state + middleware | Agent handoffs via transfer tools | Directed multigraph | AgentWorkflow on Workflows | Module composition | Agent loop with tool dispatch | Sequential/Parallel/Loop agents |
| **Resource Bounds** | Kernel-enforced (LLM calls, tokens, hops, iterations, timeout) | `recursion_limit` (Python) | Iteration limits | Configurable | Max turns | None | None | None | None | None |
| **Type Safety** | Rust compile-time + Python protocols | Pydantic | Dynamic | Strong (.NET) / Weak (Python) | Pydantic | Component typed I/O | Pythonic | Typed signatures | Python/TS | Python typing |
| **IPC** | TCP + msgpack binary (5MB max) | In-process | In-process | In-process / gRPC | In-process | In-process | In-process | In-process | In-process | In-process |
| **MCP Support** | **No** | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes (first-class) | Yes |
| **Tool System** | Capability-scoped catalogs with risk metadata + Rust access control | Tool decorator + schema | Built-in tools library | Plugin-based | `@function_tool` | Typed components | Function calling + MCP | ReAct modules + MCP | Built-in file/shell/web + MCP | Tools + MCP + Extensions |
| **Memory** | SessionStateService (in-memory) + protocol-defined persistence | Immutable state + checkpointing | 4-layer (short/long/entity/contextual) | Session state + middleware | Sessions (persistent) | Document stores | Multi-layer (buffer/summary/vector/composable) | Mem0 integration | Context/MCP | Session state |
| **Guardrails** | Pre/post hooks + tool risk policies + kernel bounds + rate limiting | None native | None native | Middleware/filters | First-class tripwire/filter | None native | None native | None native | Hooks with matchers | None native |
| **Human-in-Loop** | 7 typed interrupt kinds | Breakpoints | Human input tool | Configurable | Guardrails (blocking) | AgentSnapshot breakpoints | None native | None | None native | None native |
| **Streaming** | SSE + IPC stream frames (chunk/end) + AUTHORITATIVE/DEBUG modes | Typed StreamPart dicts | Callbacks | Streaming | Streaming | AsyncPipeline | Streaming | None | Streaming | Gemini Live API streaming |
| **Multi-Agent** | Parallel groups (JOIN_ALL/JOIN_FIRST) + routing rules | Supervisor, swarm, pipeline | First-class crew composition | Graph-based multi-agent | Agent handoffs | Pipeline composition | AgentWorkflow + swarm/orchestrator | Limited | Subagent orchestration | Sequential/Parallel/Loop |
| **Inter-Agent Comms** | CommBus (pub/sub, commands, query/response) | Shared state graph | Delegation + manager | Shared state | Handoff context | Pipeline data flow | Shared state dict | None | None native | None native |
| **Distributed** | Redis bus + distributed tasks | None native | None native | Distributed (.NET) | None native | None native | None native | None native | None | Cloud Run / Vertex AI |
| **Observability** | OTEL adapter + structlog + Prometheus (partial) | LangSmith | Callbacks | Telemetry + middleware | Built-in tracing (default on) | None built-in | OpenTelemetry | None | Built-in tracing | Built-in evaluation |
| **Evaluation** | None | LangSmith evaluation | None built-in | Built-in | Tracing-based | None built-in | None built-in | Automatic optimization (GEPA, SIMBA, MIPROv2) | None | Built-in eval framework |
| **LLM Interface** | String prompt → string response | Message-based chat | Message-based chat | Message-based chat | Message-based chat | Message-based chat | Message-based chat | Typed signatures | Message-based chat | Message-based chat |
| **Maturity** | Early (v0.0.1) | Production (v1.1) | Production (enterprise tier) | RC | Production | Production (v2.24) | Production | Production/Research | Early (v0.1.x) | Maturing |

---

## 5. When to Choose Each Framework

| Choose... | When you need... |
|-----------|-----------------|
| **Jeeves-Core** | Production-grade process isolation, kernel-enforced resource bounds, Rust safety guarantees, formal interrupt system, distributed execution. You're building a platform where runaway agents are unacceptable and you need OS-level controls over agent lifecycle. You accept the cost of Rust compilation, no MCP, and string-based LLM interface. |
| **LangGraph** | Complex stateful workflows with conditional branching, checkpointing, and human-in-the-loop. You want the largest integration ecosystem (900+) and mature tracing (LangSmith). Best for graph-shaped agent workflows. |
| **CrewAI** | Role-based multi-agent collaboration with minimal boilerplate. Fastest path from idea to working multi-agent system. Best built-in memory (4 layers). Enterprise tier available. Largest community (45.6k stars). |
| **MS Agent Framework** | Enterprise .NET/Azure environments. You need Microsoft ecosystem integration and are migrating from AutoGen or Semantic Kernel. Multi-provider (Azure OpenAI, Anthropic, Bedrock, Ollama). Supports A2A + MCP. |
| **OpenAI Agents SDK** | Rapid prototyping with any LLM (100+ providers). You want the simplest API with built-in tracing and first-class guardrails. Best developer experience of any framework. Dual Python/TS with feature parity. |
| **Haystack** | RAG and document processing pipelines with explicit control. Best for retrieval-augmented generation with typed component composition. Agent debugging with breakpoints and snapshot resumption. |
| **LlamaIndex** | Knowledge-intensive agents. Best-in-class RAG + growing agent capabilities via Workflows. Multi-layer memory. Clean event-driven async API. Largest community (47.6k stars). |
| **DSPy** | Automatic LLM pipeline optimization rather than manual prompt engineering. Unique optimizer-first approach (GEPA, SIMBA, MIPROv2). Research-backed. You want the system to find the best prompts for you. |
| **Claude Agent SDK** | Building agents powered by Claude models with native file/system access. First-class MCP with OAuth 2.0. Battle-tested runtime (powers Claude Code). IDE integrations (VSCode, Xcode). |
| **Google ADK** | Multi-language team (Python, TS, Go, Java). Google Cloud deployment (Cloud Run, Vertex AI). Built-in evaluation framework. Gemini-optimized but model-agnostic. A2A protocol support. |
| **Letta** | Memory-first agents that persist and self-improve across sessions. Self-editing memory tools. Context Repositories with git versioning. Model-agnostic stateful agent platform. |
| **BeeAI** | Framework-agnostic orchestration (run LangChain, CrewAI agents on one platform). YAML declarative workflows. Linux Foundation governance. |

---

## 6. Jeeves-Core Strengths (Unique Differentiators)

**No other OSS framework has these:**

### 6.1 Rust Microkernel with Compile-Time Safety
No null derefs, no data races, no use-after-free. The kernel is a single-actor that owns all mutable state (`&mut self`). Every other framework is pure Python/TS with runtime-only guarantees. `#[deny(unsafe_code)]` in Cargo.toml.

### 6.2 Kernel-Enforced Resource Bounds
LLM calls, tokens, agent hops, iterations, and timeout are checked at the Rust level via ProcessControlBlock quotas. Cannot be bypassed from Python code. LangGraph's `recursion_limit` is a Python-level check. No other framework has this.

### 6.3 Unix-Inspired Process Lifecycle
Agents are kernel-managed processes with states (NEW→READY→RUNNING→WAITING→TERMINATED→ZOMBIE) and scheduling priorities (REALTIME/HIGH/NORMAL/LOW/IDLE) using a BinaryHeap. Background GC cleans up zombies and stale sessions.

### 6.4 Seven Typed Interrupt Kinds
CLARIFICATION (24h TTL), CONFIRMATION (1h), AGENT_REVIEW (30min), CHECKPOINT (no TTL), RESOURCE_EXHAUSTED (5min), TIMEOUT (5min), SYSTEM_ERROR (1h). Each with configurable TTL, require_response flag, status tracking, and session queries. Most frameworks have binary yes/no or nothing.

### 6.5 CommBus with Three Messaging Patterns
Pub/sub (fan-out), commands (fire-and-forget), queries (request/response with timeout). All kernel-mediated for tracing and access control. With BusStats for monitoring. No other agent framework has a built-in message bus with these three patterns.

### 6.6 Binary IPC Protocol
TCP + msgpack with length-prefixed framing (5MB max). 27 RPC methods across 6 service handlers. Significantly faster than HTTP/JSON for high-frequency orchestration. Cross-language contract tests catch Rust↔Python schema drift at CI time.

### 6.7 Expression-Based Routing
Declarative, serializable expression trees with 13 operators (Eq, Neq, Gt, Lt, Gte, Lte, Contains, Exists, NotExists, And, Or, Not, Always) and 4 field scopes (Current, Agent, Meta, Interrupt). Evaluated in Rust. More powerful and safer than Python-function routing.

### 6.8 Config-Driven Agents (NEW)
The new `Agent` dataclass is configuration-driven — no subclassing required. Behavior determined by `AgentConfig` flags (has_llm, has_tools, tool_dispatch, output_mode, token_stream) and pre/post hooks. PipelineRunner builds agents from config with validation (output_key uniqueness, LLM factory requirements).

### 6.9 Streaming with Token Modes (NEW)
Three streaming modes: OFF (no tokens), DEBUG (preview tokens + canonical LLM call), AUTHORITATIVE (streamed tokens are the canonical output). No other framework distinguishes between debug and authoritative token streams.

### 6.10 Distributed Execution Path
Redis bus + distributed task protocol. Most frameworks are single-process only. Google ADK supports Cloud Run deployment but doesn't have an in-process distributed bus.

### 6.11 Tool Risk Classification
Rust-enforced risk semantics (critical/high/medium/low/benign) and severity (destructive/durable/query/passive) per tool. Agent→tool access grants checked at orchestration time. Circuit breaking on tool health degradation.

---

## 7. Jeeves-Core Gaps (vs. Competition)

### 7.1 Critical Gaps

| Gap | Impact | Who Does It Better | Remediation |
|-----|--------|--------------------|-------------|
| **No MCP support** | CRITICAL — MCP is now universal (every competitor supports it) | All 12 competitors | Implement MCP client; expose capabilities as MCP servers |
| **String-prompt LLM interface** | CRITICAL — `generate(prompt: str) → str` has no message-based chat, no tool schemas, no multi-modal, no structured output | Every other framework supports message-based APIs | Add `generate_chat(messages, tools) → ChatResponse` to LLMProviderProtocol |
| **No documentation** | HIGH — no tutorials, no API reference, no getting-started guide, no examples | LangChain, CrewAI, OpenAI SDK, Google ADK all have extensive docs | Create quickstart, API reference, architecture guide |

### 7.2 High-Impact Gaps

| Gap | Impact | Who Does It Better |
|-----|--------|--------------------|
| **Rust binary requirement** | HIGH — `pip install && go` vs. compile Rust + run kernel binary + connect via TCP | Every other framework is `pip install` |
| **No built-in RAG** | HIGH — most common LLM application pattern | Haystack, LlamaIndex (best-in-class), LangChain |
| **No community/ecosystem** | HIGH — no integrations, no plugins, no contributors, private repo | CrewAI (45.6k stars, 100k devs), LlamaIndex (47.6k stars), LangGraph (900+ integrations) |
| **No evaluation framework** | HIGH — no way to measure agent quality | DSPy (automatic optimization), LangSmith, Google ADK (built-in eval) |

### 7.3 Medium-Impact Gaps

| Gap | Impact | Who Does It Better |
|-----|--------|--------------------|
| **No formalized guardrails API** | MEDIUM — has pre/post hooks and tool risk but no structured violation reporting | OpenAI Agents SDK (tripwire/filter), MS Agent Framework (middleware) |
| **Partial observability** | MEDIUM — OTEL adapter + Prometheus scaffolding but no active span instrumentation | OpenAI SDK (tracing default on), LangSmith, LlamaIndex (OTEL native) |
| **In-memory session state only** | MEDIUM — SessionStateService is in-memory, no persistent conversation memory | CrewAI (4-layer), LlamaIndex (multi-layer), Letta (self-editing persistent memory) |
| **No conversation management** | MEDIUM — each request is a new Envelope, no multi-turn context | OpenAI SDK (sessions), Letta (Conversations API) |
| **Envelope complexity** | MEDIUM — ~40+ fields across semantic sub-structs, risk of "god object" | LangGraph (clean TypedDict), OpenAI SDK (simple RunContext) |
| **No A2A protocol support** | MEDIUM — emerging standard for agent-to-agent communication | MS Agent Framework, Google ADK, BeeAI |

---

## 8. Industry Trends (March 2026)

### 8.1 MCP is Universal
Every framework in this audit now supports MCP. It has become the de facto standard for agent-tool interoperability. Jeeves-core is the only framework that doesn't support it.

### 8.2 Microsoft Consolidation
AutoGen + Semantic Kernel → Microsoft Agent Framework (RC). This is the biggest ecosystem shift — teams should migrate to the unified framework.

### 8.3 Multi-Language Expansion
Google ADK leads with 4 languages (Python, TS, Go, Java). OpenAI Agents SDK has Python/TS parity. LlamaIndex has Python/TS. Single-language frameworks are at a disadvantage.

### 8.4 Memory-First Architectures
Letta's self-editing memory and Context Repositories represent a new paradigm. CrewAI's 4-layer memory is the most comprehensive in traditional frameworks. Memory is becoming a key differentiator.

### 8.5 Built-In Evaluation
Google ADK and DSPy lead with built-in evaluation capabilities. LangSmith provides evaluation externally. Frameworks without evaluation tooling are harder to use in production.

### 8.6 Agent Protocols Emerging
A2A (Agent-to-Agent), AG-UI (Agent-to-UI), ACP (Agent Client Protocol) are emerging alongside MCP. MS Agent Framework and Google ADK support multiple protocols.

---

## 9. Strategic Recommendations for Jeeves-Core

### 9.1 Critical Path (Must Do)

1. **MCP Client + Server** — Implement MCP transport. Expose capabilities as MCP servers. Consume external tools via MCP client. This is table-stakes in 2026.

2. **Message-Based LLM Interface** — Add `ChatMessage`, `ToolCall`, `ToolResult` types to LLMProviderProtocol. Support `generate_chat(messages, tools, response_format)`. Keep string `generate()` for backward compatibility.

3. **Getting-Started Documentation** — Quickstart guide, architecture overview, first-capability tutorial. Without this, no external adoption is possible.

### 9.2 High Priority

4. **Simplify Onboarding** — Consider a pure-Python mode that works without the Rust kernel (with reduced guarantees) for development/prototyping. Or provide pre-built kernel binaries.

5. **Built-In Evaluation** — Add an evaluation harness that leverages the existing OTEL + metrics infrastructure.

6. **Persistent Memory** — Implement a default memory layer (at minimum, conversation history persistence) on top of the existing DatabaseClientProtocol.

### 9.3 Medium Priority

7. **A2A Protocol Support** — Enable jeeves-core agents to communicate with agents from other frameworks.

8. **Active OTEL Instrumentation** — Wire up actual span emission in Agent.process(), PipelineRunner, and tool execution paths.

9. **Structured Guardrails API** — Formalize the existing pre/post hooks into a guardrail protocol with violation reporting, similar to OpenAI SDK's tripwire/filter model.

---

## 10. Competitive Position Summary

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
                         │  ● Letta
                         │
                         │        ● BeeAI
                         │                    ● AG2
                         └──────────────────────────────► ECOSYSTEM / ADOPTION
```

Jeeves-Core occupies the upper-left quadrant: highest architectural rigor, lowest ecosystem/adoption. The strategic challenge is moving right without sacrificing the architectural advantages that make it unique.

**Bottom line:** Jeeves-core has the most rigorous agent runtime architecture of any OSS framework. The new `runtime/agents.py` completes the Python execution model. But in a 2026 landscape where MCP is universal and `pip install` is expected, the critical path is: **MCP → message-based LLM → docs → reference capability**. Everything else follows.
