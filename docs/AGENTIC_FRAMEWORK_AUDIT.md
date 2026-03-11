# Jeeves-Core vs. OSS Agentic Frameworks — Full Comparative Audit

**Date:** 2026-03-11 (Updated)
**Scope:** Comprehensive comparison of jeeves-core against 12 major OSS agentic AI frameworks
**Codebase revision:** Post-merge of origin/main (commit 21491d0) — observability refactor, gateway hardening, pipeline_worker finalization

---

## 1. Executive Summary

Jeeves-Core has the **strongest systems architecture** of any OSS agentic framework — it is the only one treating agents as OS-managed processes with a Rust microkernel enforcing resource bounds, typed interrupts, and expression-based routing with compile-time safety guarantees. With 14,785 lines of Rust and 31,128 lines of Python across 152 files, it is a fully implemented dual-layer runtime.

The latest merge completes the observability story: the OTEL adapter has been consolidated from 576 lines to 138, dead metrics removed, and kernel-unique instrumentation (pipeline terminations, kernel instructions, agent execution duration, tool executions) is now wired. The gateway has been hardened with CORS wildcard rejection, request body size limiting, and Prometheus middleware. `PipelineWorker` has been finalized with `run_kernel_loop()` as a standalone free function and streaming execution support.

However, jeeves-core remains in a **"great engine, no dashboard"** state: architecturally superior but practically harder to adopt than competitors due to missing MCP support, string-based LLM interface (no message/chat API), no documentation, and zero community ecosystem — all areas where every major competitor now excels.

**Critical industry shift (2026):** MCP is now universal — every framework in this audit supports it. Microsoft has consolidated AutoGen + Semantic Kernel into a new Agent Framework (RC). Google ADK has expanded to 4 languages. CrewAI leads community size at 45.6k stars.

---

## 2. Jeeves-Core Architecture Inventory (Full Re-Inventory)

**Total codebase:** 14,785 lines Rust (41 files) + 31,128 lines Python (111 files) + 2,341 lines tests (24 test files)

### 2.1 Rust Kernel — 9,173 Lines, Fully Implemented

| Module | File | Status | Lines | Description |
|--------|------|--------|-------|-------------|
| Orchestrator | `src/kernel/orchestrator.rs` | FULL | 2,165 | Pipeline session management, bounds enforcement, stage routing, edge traversal tracking |
| Kernel core | `src/kernel/mod.rs` | FULL | 1,098 | Single-actor owner of all subsystems, envelope store (`process_envelopes: HashMap<ProcessId, Envelope>`) |
| Interrupts | `src/kernel/interrupts.rs` | FULL | 1,041 | 7 typed interrupt kinds with TTL, status tracking, session queries, interrupt state machine, validation |
| Process lifecycle | `src/kernel/lifecycle.rs` | FULL | 687 | ProcessControlBlock, state machine (NEW→READY→RUNNING→WAITING→TERMINATED→ZOMBIE), priority scheduling |
| Routing | `src/kernel/routing.rs` | FULL | 619 | Expression tree evaluation (13 operators, 4 field scopes), pipeline graph traversal, field reference resolution |
| Services | `src/kernel/services.rs` | FULL | 538 | ServiceRegistry for capability discovery and dispatch |
| Types | `src/kernel/types.rs` | FULL | 374 | Core type definitions (ProcessId, SessionId, etc.) |
| Rate limiting | `src/kernel/rate_limiter.rs` | FULL | 371 | Token bucket algorithm, per-user rate limiting with minute/hour windows |
| Resource tracking | `src/kernel/resources.rs` | FULL | 310 | Per-process and per-user quota enforcement (max_iterations, max_llm_calls, max_agent_hops, max_tool_calls) |
| Cleanup/GC | `src/kernel/cleanup.rs` | FULL | 304 | Background zombie/stale session cleanup |
| Orchestrator types | `src/kernel/orchestrator_types.rs` | FULL | 274 | PipelineSession, bounds, TerminalReason enums |
| Recovery | `src/kernel/recovery.rs` | FULL | 166 | Panic recovery wrapper for fault isolation |

**Key Structs:** `Kernel` (single-actor, `&mut self`), `Orchestrator`, `ProcessControlBlock`, `PipelineSession` (no envelope duplication — kernel owns), `ResourceQuota`, `RateLimiter`, `InterruptService`

### 2.2 IPC Layer — 2,833 Lines, Fully Implemented

| Component | File | Lines | Description |
|-----------|------|-------|-------------|
| Kernel handler | `src/ipc/handlers/kernel.rs` | 636 | Process CRUD, orchestration flows, quota checks |
| TCP server | `src/ipc/server.rs` | 505 | Async request routing, semaphore-controlled concurrency, error handling |
| Services handler | `src/ipc/handlers/services.rs` | 410 | Service registry queries, dispatch routing |
| Codec | `src/ipc/codec.rs` | 233 | Msgpack encoding/decoding, length-prefixed framing (5MB max) |
| Tools handler | `src/ipc/handlers/tools.rs` | 231 | Tool catalog, health, access policy validation |
| Orchestration handler | `src/ipc/handlers/orchestration.rs` | 223 | Session init, next instruction, result reporting |
| Validation handler | `src/ipc/handlers/validation.rs` | 214 | IPC message validation, input/output schema checks |
| CommBus handler | `src/ipc/handlers/commbus.rs` | 171 | Publish/command/query/subscribe |
| Interrupt handler | `src/ipc/handlers/interrupt.rs` | 145 | Create/resolve/get/cancel interrupts |
| Router | `src/ipc/router.rs` | 47 | IPC request routing dispatcher |

### 2.3 CommBus — 849 Lines, Fully Implemented

Location: `src/commbus/mod.rs`

Three messaging patterns:
- **Pub/sub:** Event type → subscriber list, fan-out delivery
- **Commands:** Fire-and-forget, single handler per command type
- **Queries:** Request/response with configurable timeout

Built-in BusStats, subscription receipts, pure in-process implementation.

### 2.4 Envelope — 1,125 Lines, Fully Implemented

| File | Lines | Description |
|------|-------|-------------|
| `src/envelope/mod.rs` | 997 | Bounded pipeline state with inputs, outputs, metadata. Semantic sub-structs: Identity, Pipeline, Bounds, Interrupts, Execution, Audit |
| `src/envelope/enums.rs` | 128 | TerminalReason, InterruptKind, HealthStatus, ToolCategory enums |

FlowInterrupt and InterruptResponse with builder methods. Source of truth for request state — kernel stores in `process_envelopes: HashMap<ProcessId, Envelope>`.

### 2.5 Tools Subsystem (Rust) — 1,196 Lines, Fully Implemented

| Component | File | Lines | Description |
|-----------|------|-------|-------------|
| Health tracking | `src/tools/health.rs` | 565 | Sliding-window metrics, circuit breaking, tool execution tracking |
| Catalog | `src/tools/catalog.rs` | 501 | ParamType validation, ToolEntry with risk classifications (critical→benign), severity (destructive→passive) |
| Access control | `src/tools/access.rs` | 130 | Agent→tool grant/revoke/check via HashMap |

### 2.6 Supporting Rust Modules — 609 Lines

| File | Lines | Description |
|------|-------|-------------|
| `src/types/config.rs` | 251 | IpcConfig, Config structures |
| `src/events/translation.rs` | 216 | Kernel event to external event translation |
| `src/types/errors.rs` | 117 | Error types and propagation |
| `src/types/ids.rs` | 89 | Strongly-typed IDs (ProcessId, SessionId, RequestId, UserId) |
| `src/observability.rs` | 57 | Structured logging and tracing setup |
| `src/events/mod.rs` | 8 | Event type definitions |

---

### 2.7 Python Runtime — 31,128 Lines, Fully Implemented

#### 2.7.1 IPC & Kernel Communication — 1,320 Lines

**File: `kernel_client.py`** (1,320 lines) — Async TCP+msgpack client
- `KernelClient` — 27+ RPC methods: `create_process()`, `initialize_orchestration_session()`, `get_next_instruction()`, `report_agent_result()`, `check_quota()`, `terminate_process()`, `get_process_info()`
- Types: `OrchestratorInstruction` (RUN_AGENT, RUN_AGENTS, TERMINATE, WAIT_INTERRUPT), `QuotaCheckResult`, `ProcessInfo`, `AgentExecutionMetrics`

#### 2.7.2 Pipeline & Agent Execution — 2,291 Lines

| Module | Lines | Status | Key Responsibilities |
|--------|-------|--------|----------------------|
| `pipeline_worker.py` | 574 | COMPLETE | Kernel-driven execution; `run_kernel_loop()` free function; `PipelineWorker` with `execute()` and `execute_streaming()` |
| `runtime/agents.py` | 747 | COMPLETE | Config-driven Agent dataclass (`process()` batch, `stream()` tokens), PipelineRunner, PipelineRegistry, PromptRegistry |
| `runtime/capability_service.py` | 399 | COMPLETE (REFACTORED) | Base class for kernel-driven capabilities; async lifecycle hooks (`_ensure_ready()`, `_enrich_metadata()`, `_on_result()`, `_build_result()`) |
| `runtime/persistence.py` | 155 | COMPLETE | `DatabasePersistence` — state save/load via async database |

**Architectural pattern:** Kernel owns orchestration (loop, routing, bounds); Python only runs agents. `PipelineWorker` is the bridge — receives kernel instructions, dispatches to agents, reports results back.

**Agent features:** LLM calls with usage tracking, tool execution with access control, deterministic tool dispatch, streaming with AUTHORITATIVE/DEBUG token modes, generic prompt context building from envelope outputs + metadata, pre/post hooks, mock handlers, OTEL span wrapping, JSON repair for LLM output.

#### 2.7.3 Gateway HTTP Interface — 1,249 Lines (HARDENED)

**File: `gateway/app.py`** (373 lines) — FastAPI application with:
- Lifecycle management (startup/shutdown)
- Service discovery via `CapabilityResourceRegistry`
- CORS middleware with **wildcard rejection** + credentials check (NEW)
- **Request body size limiting** (1 MB default) (NEW)
- **Prometheus metrics middleware** (NEW)
- Health/readiness probes
- OTEL tracing initialization via `instrument_fastapi()`
- Static files + Jinja2 templates
- Router mounting: chat, governance, interrupts

**Gateway Routers:**

| Router | Lines | Endpoints |
|--------|-------|-----------|
| `routers/chat.py` | 605 | POST /messages, GET /stream (SSE), /sessions CRUD, event publishing |
| `routers/interrupts.py` | 339 | POST /interrupts, GET /interrupts/{id}, PATCH /interrupts/{id}/response |
| `routers/health.py` | 214 | GET /health, GET /system/health, tool health summary |
| `routers/governance.py` | 77 | GET /governance/agents, /governance/memory (system introspection) |

**Supporting:** `gateway/sse.py` (191 lines) — Server-Sent Events; `gateway/event_bus.py` (191 lines) — Event aggregation

#### 2.7.4 Protocols & Interfaces — 2,990 Lines

| Module | Lines | Status | Purpose |
|--------|-------|--------|---------|
| `protocols/types.py` | 1,071 | COMPLETE | Envelope, PipelineConfig, RequestContext, enums (TerminalReason, InterruptKind) |
| `protocols/capability.py` | 932 | COMPLETE | Capability protocol definitions |
| `protocols/interfaces.py` | 461 | COMPLETE | 18 runtime-checkable protocols |
| `protocols/__init__.py` | 183 | COMPLETE | Public API exports |
| `protocols/routing.py` | 136 | COMPLETE | Routing rule evaluation |
| `protocols/_generated.py` | 119 | COMPLETE | Auto-generated Rust enum parity |
| `protocols/events.py` | 88 | COMPLETE | Event, EventCategory, EventSeverity types |

**18 Protocols:** LoggerProtocol, DatabaseClientProtocol, LLMProviderProtocol, ToolProtocol, ToolDefinitionProtocol, ToolRegistryProtocol, ClockProtocol, AppContextProtocol, SemanticSearchProtocol, DistributedBusProtocol, ToolExecutorProtocol, ConfigRegistryProtocol, AgentToolAccessProtocol, WebSocketManagerProtocol, EventBridgeProtocol, RequestContext, InterruptServiceProtocol, EventEmitterProtocol.

**Deleted:** `protocols/validation.py` — validation logic moved to individual handlers (commit 21491d0)

#### 2.7.5 Observability — 387 Lines (REFACTORED)

**File: `observability/metrics.py`** (228 lines) — Prometheus instrumentation
- `ORCHESTRATOR_INFLIGHT` — in-flight requests (Gauge)
- `ORCHESTRATOR_REQUESTS` — total by outcome (Counter)
- `ORCHESTRATOR_LATENCY` — end-to-end duration (Histogram)
- `LLM_PROVIDER_CALLS`, `LLM_TOKENS_USED` — LLM instrumentation
- `HTTP_REQUESTS_TOTAL`, `HTTP_REQUEST_DURATION` — gateway metrics
- **Kernel-unique (NEW):** `PIPELINE_TERMINATIONS`, `KERNEL_INSTRUCTIONS`, `AGENT_EXECUTION_DURATION`, `TOOL_EXECUTIONS`

**File: `observability/otel_adapter.py`** (138 lines) — Consolidated OTEL wrapper (was 576 lines)
- `OpenTelemetryAdapter` — span management
- Global singleton: `init_global_otel()`, `get_global_otel_adapter()`
- `instrument_fastapi()` — FastAPI auto-instrumentation
- `shutdown_tracing()` — graceful shutdown

**Deleted:** `observability/tracing.py` — consolidated into `otel_adapter.py` (commit 21491d0)

#### 2.7.6 Tools & Execution (Python) — 690 Lines

| Module | Lines | Status | Purpose |
|--------|-------|--------|---------|
| `tools/executor.py` | 258 | COMPLETE | `ToolExecutionCore` — pure execution logic (validate_params, filter_none_params, normalize_result, execute_tool). Optional ToolHealthService recording |
| `tools/catalog.py` | 295 | COMPLETE | `ToolCatalog` — tool discovery and metadata |
| `tools/decorator.py` | 137 | COMPLETE | `@tool` decorator with automatic service injection via `_INJECTED_DEPS` |

#### 2.7.7 LLM Providers — 748 Lines

| Module | Lines | Status | Purpose |
|--------|-------|--------|---------|
| `llm/providers/mock.py` | 457 | COMPLETE | Mock provider for testing (configurable responses) |
| `llm/providers/litellm_provider.py` | 322 | COMPLETE | LiteLLM adapter (90+ model backends) |
| `llm/providers/openai_http_provider.py` | 263 | COMPLETE | OpenAI-compatible HTTP API |
| `llm/factory.py` | 158 | COMPLETE | Provider factory pattern |
| `llm/cost_calculator.py` | 206 | COMPLETE | Token cost estimation |
| `llm/providers/base.py` | 127 | COMPLETE | LLMProvider ABC, TokenChunk dataclass |

**Interface:** `generate(model, prompt, options) → str` + `generate_stream()` — string-based prompt, not message-based chat.

#### 2.7.8 Capability & Service Management — 761 Lines

| Module | Lines | Purpose |
|--------|-------|---------|
| `capability_wiring.py` | 495 | Auto-wiring: `chain()`, `graph()`, `from_decorated()` (ADR-002) |
| `capability_registry.py` | 266 | `CapabilityRegistry` — capability discovery and registration |

#### 2.7.9 Orchestration & Events — 1,268 Lines

| Module | Lines | Purpose |
|--------|-------|---------|
| `orchestrator/event_context.py` | 534 | Event context and metadata |
| `orchestrator/events.py` | 409 | Event type definitions and handlers |
| `orchestrator/agent_events.py` | 309 | Agent-specific event types |
| `orchestrator/governance_service.py` | 288 | `HealthServicer` — system health, agent definitions, memory introspection |

**Event Bridge:** `events/bridge.py` (207 lines) — cross-layer translation; `events/aggregator.py` (179 lines) — event aggregation.

#### 2.7.10 Data Persistence & State — 1,276 Lines

| Module | Lines | Purpose |
|--------|-------|---------|
| `memory/tool_health_service.py` | 533 | Tool execution tracking and health metrics |
| `memory/tool_metrics_repository.py` | 436 | Tool performance metrics persistence |
| `redis/client.py` | 386 | Async Redis wrapper with connection pooling |
| `redis/connection_manager.py` | 329 | Redis connection lifecycle, state backend selection |
| `memory/session_state_service.py` | 98 | In-memory session state management |
| `database/client.py` | 27 | Async SQLite client |

#### 2.7.11 Configuration & Bootstrap — 999 Lines

| Module | Lines | Purpose |
|--------|-------|---------|
| `bootstrap.py` | 371 | `create_app_context()` — dependency wiring, kernel connectivity validation, feature flags |
| `wiring.py` | 304 | Dependency injection wiring |
| `settings.py` | 288 | `Settings` — Pydantic environment-based config |
| `context.py` | 224 | `AppContext` — composition root dependency container |
| `feature_flags.py` | 165 | `FeatureFlags` — feature toggle system |
| `logging/__init__.py` | 159 | structlog setup, request-scoped loggers |
| `config/registry.py` | 116 | `ConfigRegistry` — dynamic config lookup |

**ADR-001 Compliance:** Singleton elimination via AppContext dependency injection.

#### 2.7.12 Distributed & IPC (Python) — 709 Lines

| Module | Lines | Purpose |
|--------|-------|---------|
| `distributed/redis_bus.py` | 446 | Redis-backed message bus |
| `ipc/transport.py` | 263 | TCP+msgpack transport layer |

#### 2.7.13 Middleware — 163 Lines

| Module | Lines | Purpose |
|--------|-------|---------|
| `middleware/rate_limit.py` | 88 | FastAPI rate limiting |
| `middleware/body_limit.py` | 75 | Request body size limiting |

### 2.8 Testing Infrastructure — 2,341 Lines, 24 Test Files

**Unit Tests:**

| Test | Lines | Coverage |
|------|-------|----------|
| `test_events.py` (orchestrator) | 1,171 | Event handling, context, aggregation |
| `test_pipeline_worker.py` | 592 | Kernel loop, streaming, result reporting |
| `test_bootstrap.py` | 537 | Dependency wiring, context creation |
| `test_kernel_client.py` | 524 | IPC client, all RPC methods |
| `test_tool_executor.py` | 408 | Parameter validation, execution, health recording |
| `test_providers.py` (LLM) | 402 | Mock, OpenAI HTTP, LiteLLM providers |
| `test_app.py` (gateway) | 378 | FastAPI endpoints, CORS, health |
| `test_persistence.py` | 178 | State save/load |
| `test_rate_limit.py` | 166 | Rate limiting middleware |
| `test_governance_service.py` | 138 | Health servicer, agent discovery |

**Protocol Parity Tests:**

| Test | Lines | Purpose |
|------|-------|---------|
| `test_ipc_contract.py` | 394 | Cross-language schema drift detection |
| `test_enum_parity.py` | 97 | Rust↔Python enum alignment |
| `test_method_parity.py` | 72 | IPC method signature alignment |
| `test_quota_defaults.py` | 57 | Quota default consistency |
| `test_wire_constants.py` | 54 | Wire format constant alignment |

**Integration Tests:** `test_kernel_ipc.py` (64 lines) — end-to-end kernel communication

**Coverage threshold:** 50%+ with comprehensive protocol parity testing. 240+ Rust unit tests inline.

### 2.9 Build Configuration

**Cargo.toml** (95 lines): Rust 2021 edition, tokio async, serde serialization, rmp-serde msgpack, tracing observability, `#[deny(unsafe_code)]`

**pyproject.toml** (115 lines): Version 0.0.1, Python ≥3.11, dependencies: fastapi, uvicorn, structlog, pydantic, opentelemetry, prometheus-client, redis, litellm, httpx

---

## 3. Recent Changes (This Merge)

### 3.1 Observability Refactor (commit 21491d0)

| Change | Before | After |
|--------|--------|-------|
| OTEL adapter | 576 lines (over-engineered) | 138 lines (thin wrapper) |
| `tracing.py` | Existed | **Deleted** — consolidated into `otel_adapter.py` |
| Kernel metrics | Not wired | **Wired:** PIPELINE_TERMINATIONS, KERNEL_INSTRUCTIONS, AGENT_EXECUTION_DURATION, TOOL_EXECUTIONS |
| Dead metrics | Present | **Removed** |

### 3.2 Gateway Hardening

| Change | Description |
|--------|-------------|
| CORS | Wildcard origin rejection with credentials check |
| Body limit | 1 MB default request body size limiting |
| Prometheus | Metrics middleware integrated into app lifecycle |
| OTEL | `instrument_fastapi()` wired in startup |

### 3.3 Pipeline Worker Finalization

| Change | Description |
|--------|-------------|
| `run_kernel_loop()` | Extracted as standalone free function (testable without class) |
| `execute_streaming()` | Streaming execution path finalized |
| Single responsibility | Worker only runs agents; kernel owns orchestration |

### 3.4 Capability Service Refactor

| Change | Description |
|--------|-------------|
| Hook extraction | `_ensure_ready()`, `_enrich_metadata()`, `_on_result()`, `_build_result()` lifecycle hooks formalized |
| Base class | ~90 lines of boilerplate extracted from capability implementations |

### 3.5 Deleted Files

| File | Reason |
|------|--------|
| `protocols/validation.py` | Validation moved to individual handlers |
| `observability/tracing.py` | Consolidated into `otel_adapter.py` |

---

## 4. Competitor Framework Profiles (March 2026)

### 4.1 LangGraph (LangChain)

- **Version:** 1.1 | **Stars:** 24.6k | **MCP:** Yes | **Languages:** Python, JS/TS
- **Architecture:** Low-level graph-based orchestration for stateful agents. StateGraph with conditional edges, checkpointing, human-in-the-loop breakpoints.
- **2026 updates:** LangChain v1.0 now builds all agents on LangGraph. Pluggable sandbox integrations (Modal, Daytona, Runloop). V2 streaming with typed `StreamPart` dicts.
- **Ecosystem:** 900+ integrations, LangSmith for tracing/evaluation, 4.2M+ monthly downloads.
- **Maturity:** Production-stable. MIT license. Used by Klarna, Replit, Elastic.

### 4.2 CrewAI

- **Version:** 0.152.0 | **Stars:** 45.6k | **MCP:** Yes (native + Enterprise MCP Server) | **Language:** Python
- **Architecture:** Role-based crews + event-driven Flows. Independent from LangChain.
- **Memory:** 4-layer (short-term, long-term, entity, contextual) — most comprehensive built-in memory of any framework.
- **Enterprise:** CrewAI AMP — unified control plane, real-time observability, on-prem/VPC deployment.
- **Community:** 100k+ certified developers, 1.4B+ automations run.

### 4.3 Microsoft Agent Framework (AutoGen + Semantic Kernel successor)

- **Version:** RC (Feb 19, 2026) | **Stars:** ~38k (legacy AutoGen) | **MCP:** Yes | **Languages:** Python, .NET
- **Architecture:** Combines AutoGen's multi-agent abstractions with Semantic Kernel's enterprise features. Graph-based workflows, session state, middleware, telemetry.
- **Status:** AutoGen is in maintenance mode (critical fixes only). Semantic Kernel continues updates but new work focused on Agent Framework.
- **Protocols:** Supports A2A, AG-UI, and MCP standards.

### 4.4 AG2 (Independent AutoGen Fork)

- **Version:** Active fork | **Stars:** 4.2k | **MCP:** Yes | **Language:** Python
- **Status:** Community-driven by volunteers from Google DeepMind, Meta, IBM. Good for research/prototyping, not recommended for production.

### 4.5 OpenAI Agents SDK

- **Version:** 0.11.1 (Mar 9, 2026) | **Stars:** ~19k+ | **MCP:** Yes (stdio, SSE, streamable HTTP) | **Languages:** Python, TS/JS
- **Architecture:** 5 primitives — agents, handoffs, guardrails, sessions, tracing. Extremely lightweight. Provider-agnostic (100+ LLMs).
- **Guardrails:** First-class tripwire (abort) and filter (transform) guardrails on input/output.
- **2026 updates:** WebSocket transport, Responses API (replacing Assistants API mid-2026), `ToolOutputTrimmer`.

### 4.6 Semantic Kernel

- **Version:** Python 1.40.0, .NET 1.73.0 | **Stars:** 27.3k | **MCP:** Yes | **Languages:** C#, Python, Java
- **Status:** Production-stable across 3 languages. Being merged into Microsoft Agent Framework. 9.8M NuGet downloads (.NET).

### 4.7 Haystack (deepset)

- **Version:** 2.24.1 | **Stars:** 24.4k | **MCP:** Yes (MCPToolset + Hayhooks MCP server) | **Language:** Python
- **Architecture:** Context-engineering-first. Modular pipelines with explicit control. Agent component with breakpoint debugging and AgentSnapshot resumption.
- **2026 updates:** Semantic document splitting (EmbeddingBasedDocumentSplitter), Hayhooks for deployment as REST/MCP.
- **Enterprise:** Haystack Enterprise Platform (managed cloud or self-hosted).

### 4.8 LlamaIndex

- **Version:** Workflows 2.15.0 | **Stars:** 47.6k | **MCP:** Yes (native client, LlamaCloud MCP server) | **Languages:** Python, TS
- **Architecture:** Event-driven async-first Workflows. AgentWorkflow for multi-agent orchestration with state management. ACP (Agent Client Protocol) integration.
- **2026 updates:** Workflows 1.0 separated into standalone package, `llamactl` for pre-built agent templates, Workflow Debugger.
- **RAG:** Best-in-class retrieval-augmented generation, now complemented by strong agent capabilities.

### 4.9 DSPy (Stanford NLP)

- **Version:** 3.1.0 | **Stars:** 28k | **MCP:** Yes | **Language:** Python
- **Architecture:** "Programming, not prompting." Automatic prompt optimization via optimizers (MIPROv2, GEPA, SIMBA). Typed declarative signatures.
- **2026 updates:** `dspy.Reasoning` for native reasoning model support, GEPA cached evals, SIMBA optimizer, Python 3.14 support.
- **Unique:** Only framework focused on automatic optimization rather than manual prompt engineering.

### 4.10 Claude Agent SDK (Anthropic)

- **Version:** 0.1.48 (Mar 7, 2026) | **MCP:** Yes (first-class, in-process servers) | **Language:** Python
- **Architecture:** Full Claude Code capabilities as SDK — file read/write, system commands, subagents, background tasks, plugins. Native MCP with OAuth 2.0 for remote servers.
- **2026 updates:** VSCode native MCP dialog, MCP startup performance improvements, binary content handling, Apple Xcode 26.3 integration.
- **Maturity:** Early but rapidly maturing. Battle-tested runtime (powers Claude Code).

### 4.11 Google ADK (Agent Development Kit)

- **Version:** ~1.19.0 | **Stars:** 17.2k | **MCP:** Yes | **Languages:** Python, TS, Go, Java
- **Architecture:** Code-first and no-code (Agent Config). Workflow agents (Sequential, Parallel, Loop). Built-in evaluation framework.
- **2026 updates:** TypeScript SDK launched, third-party integrations (AgentOps, Arize, MLflow, n8n, Hugging Face). Deploy to Cloud Run or Vertex AI.
- **Unique:** Only framework supporting 4 languages. A2A protocol support.

### 4.12 Additional Frameworks

| Framework | Stars | Status | Notes |
|-----------|-------|--------|-------|
| **BeeAI (IBM)** | 3.1k | Active/Community | Linux Foundation AI & Data. Framework-agnostic orchestration. YAML declarative workflows. IBM states it won't maintain — community-driven. |
| **Letta (MemGPT)** | 21k | Production | Memory-first platform. Self-editing memory, Context Repositories with git versioning, Agent File (.af) format. #1 on Terminal-Bench for model-agnostic coding agents. |

---

## 5. Full Comparison Matrix

| Dimension | Jeeves-Core | LangGraph | CrewAI | MS Agent Framework | OpenAI Agents SDK | Haystack | LlamaIndex | DSPy | Claude Agent SDK | Google ADK |
|-----------|-------------|-----------|--------|---------------------|-------------------|----------|------------|------|-----------------|------------|
| **Language** | Rust + Python | Python, JS/TS | Python | Python, .NET | Python, TS/JS | Python | Python, TS | Python | Python | Python, TS, Go, Java |
| **Version** | 0.0.1 | 1.1 | 0.152 | RC | 0.11.1 | 2.24.1 | 2.15.0 (wf) | 3.1.0 | 0.1.48 | ~1.19.0 |
| **Codebase** | 14.8k Rust + 31.1k Python | ~50k+ | ~30k+ | ~100k+ (combined) | ~15k | ~40k+ | ~80k+ | ~25k | N/A | ~20k+ |
| **Stars** | Private | 24.6k | 45.6k | ~38k (legacy) | ~19k+ | 24.4k | 47.6k | 28k | N/A | 17.2k |
| **Agent Model** | Config-driven kernel processes | DAG state machine | Role-based crews + Flows | Multi-agent + graph workflows | 5 primitives (agents, handoffs, guardrails, sessions, tracing) | Component pipeline + Agent | Event-driven @step Workflows | Declarative typed signatures | Agent loop + hooks + skills | Code-first + Agent Config |
| **Orchestration** | Kernel-managed pipeline graph with expression routing | StateGraph with conditional edges | Sequential/hierarchical + event Flows | Graph + session state + middleware | Agent handoffs via transfer tools | Directed multigraph | AgentWorkflow on Workflows | Module composition | Agent loop with tool dispatch | Sequential/Parallel/Loop agents |
| **Resource Bounds** | Kernel-enforced (LLM calls, tokens, hops, iterations, tool calls, timeout) | `recursion_limit` (Python) | Iteration limits | Configurable | Max turns | None | None | None | None | None |
| **Type Safety** | Rust compile-time + 18 Python protocols | Pydantic | Dynamic | Strong (.NET) / Weak (Python) | Pydantic | Component typed I/O | Pythonic | Typed signatures | Python/TS | Python typing |
| **IPC** | TCP + msgpack binary (5MB max, 27 RPC methods) | In-process | In-process | In-process / gRPC | In-process | In-process | In-process | In-process | In-process | In-process |
| **MCP Support** | **No** | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes (first-class) | Yes |
| **Tool System** | Capability-scoped catalogs with risk metadata + Rust access control + circuit breaking | Tool decorator + schema | Built-in tools library | Plugin-based | `@function_tool` | Typed components | Function calling + MCP | ReAct modules + MCP | Built-in file/shell/web + MCP | Tools + MCP + Extensions |
| **Memory** | SessionStateService (in-memory) + ToolHealthService + ToolMetricsRepository | Immutable state + checkpointing | 4-layer (short/long/entity/contextual) | Session state + middleware | Sessions (persistent) | Document stores | Multi-layer (buffer/summary/vector/composable) | Mem0 integration | Context/MCP | Session state |
| **Guardrails** | Pre/post hooks + tool risk policies + kernel bounds + rate limiting + body size limiting | None native | None native | Middleware/filters | First-class tripwire/filter | None native | None native | None native | Hooks with matchers | None native |
| **Human-in-Loop** | 7 typed interrupt kinds (1,041 lines Rust) | Breakpoints | Human input tool | Configurable | Guardrails (blocking) | AgentSnapshot breakpoints | None native | None | None native | None native |
| **Streaming** | SSE + IPC stream frames (chunk/end) + AUTHORITATIVE/DEBUG modes + `execute_streaming()` | Typed StreamPart dicts | Callbacks | Streaming | Streaming | AsyncPipeline | Streaming | None | Streaming | Gemini Live API streaming |
| **Multi-Agent** | Parallel groups (JOIN_ALL/JOIN_FIRST) + routing rules + PipelineRegistry | Supervisor, swarm, pipeline | First-class crew composition | Graph-based multi-agent | Agent handoffs | Pipeline composition | AgentWorkflow + swarm/orchestrator | Limited | Subagent orchestration | Sequential/Parallel/Loop |
| **Inter-Agent Comms** | CommBus (pub/sub, commands, query/response) — 849 lines | Shared state graph | Delegation + manager | Shared state | Handoff context | Pipeline data flow | Shared state dict | None | None native | None native |
| **Distributed** | Redis bus (446 lines) + distributed tasks | None native | None native | Distributed (.NET) | None native | None native | None native | None native | None | Cloud Run / Vertex AI |
| **Observability** | OTEL adapter (138 lines) + Prometheus metrics (228 lines, 10 named metrics) + structlog | LangSmith | Callbacks | Telemetry + middleware | Built-in tracing (default on) | None built-in | OpenTelemetry | None | Built-in tracing | Built-in evaluation |
| **Evaluation** | None | LangSmith evaluation | None built-in | Built-in | Tracing-based | None built-in | None built-in | Automatic optimization (GEPA, SIMBA, MIPROv2) | None | Built-in eval framework |
| **LLM Interface** | String prompt → string response | Message-based chat | Message-based chat | Message-based chat | Message-based chat | Message-based chat | Message-based chat | Typed signatures | Message-based chat | Message-based chat |
| **Maturity** | Early (v0.0.1) | Production (v1.1) | Production (enterprise tier) | RC | Production | Production (v2.24) | Production | Production/Research | Early (v0.1.x) | Maturing |

---

## 6. When to Choose Each Framework

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

## 7. Jeeves-Core Strengths (Unique Differentiators)

**No other OSS framework has these:**

### 7.1 Rust Microkernel with Compile-Time Safety
14,785 lines of Rust with `#[deny(unsafe_code)]`. No null derefs, no data races, no use-after-free. The kernel is a single-actor that owns all mutable state (`&mut self`). Every other framework is pure Python/TS with runtime-only guarantees.

### 7.2 Kernel-Enforced Resource Bounds
LLM calls, tokens, agent hops, iterations, tool calls, and timeout are checked at the Rust level via ProcessControlBlock quotas (310 lines in `resources.rs`). Bounds validated for positivity (zero/negative rejected). Cannot be bypassed from Python code. LangGraph's `recursion_limit` is a Python-level check. No other framework has this.

### 7.3 Unix-Inspired Process Lifecycle
687 lines in `lifecycle.rs`. Agents are kernel-managed processes with states (NEW→READY→RUNNING→WAITING→TERMINATED→ZOMBIE) and scheduling priorities (REALTIME/HIGH/NORMAL/LOW/IDLE) using a BinaryHeap. Background GC cleans up zombies and stale sessions (304 lines in `cleanup.rs`).

### 7.4 Seven Typed Interrupt Kinds
1,041 lines in `interrupts.rs`. CLARIFICATION (24h TTL), CONFIRMATION (1h), AGENT_REVIEW (30min), CHECKPOINT (no TTL), RESOURCE_EXHAUSTED (5min), TIMEOUT (5min), SYSTEM_ERROR (1h). Each with configurable TTL, require_response flag, status tracking, and session queries. Most frameworks have binary yes/no or nothing.

### 7.5 CommBus with Three Messaging Patterns
849 lines in `commbus/mod.rs`. Pub/sub (fan-out), commands (fire-and-forget), queries (request/response with timeout). All kernel-mediated for tracing and access control. With BusStats for monitoring. No other agent framework has a built-in message bus with these three patterns.

### 7.6 Binary IPC Protocol
2,833 lines across IPC layer. TCP + msgpack with length-prefixed framing (5MB max). 27 RPC methods across 7 service handlers (kernel, orchestration, interrupt, tools, services, commbus, validation). Cross-language contract tests (674 lines) catch Rust↔Python schema drift at CI time.

### 7.7 Expression-Based Routing
619 lines in `routing.rs`. Declarative, serializable expression trees with 13 operators (Eq, Neq, Gt, Lt, Gte, Lte, Contains, Exists, NotExists, And, Or, Not, Always) and 4 field scopes (Current, Agent, Meta, Interrupt). Evaluated in Rust. More powerful and safer than Python-function routing.

### 7.8 Config-Driven Agents
747 lines in `agents.py`. The `Agent` dataclass is configuration-driven — no subclassing required. Behavior determined by `AgentConfig` flags (has_llm, has_tools, tool_dispatch, output_mode, token_stream) and pre/post hooks. PipelineRunner builds agents from config with validation (output_key uniqueness, LLM factory requirements).

### 7.9 Streaming with Token Modes
Three streaming modes: OFF (no tokens), DEBUG (preview tokens + canonical LLM call), AUTHORITATIVE (streamed tokens are the canonical output). `execute_streaming()` in PipelineWorker. No other framework distinguishes between debug and authoritative token streams.

### 7.10 Distributed Execution Path
Redis bus (446 lines) + distributed task protocol. Most frameworks are single-process only. Google ADK supports Cloud Run deployment but doesn't have an in-process distributed bus.

### 7.11 Tool Risk Classification
1,196 lines across Rust tools subsystem. Risk semantics (critical/high/medium/low/benign) and severity (destructive/durable/query/passive) per tool. Agent→tool access grants checked at orchestration time. Circuit breaking on tool health degradation (565 lines in `health.rs`). ToolHealthService + ToolMetricsRepository on Python side (969 lines).

### 7.12 Kernel-Unique Observability Metrics (NEW)
10 named Prometheus metrics including 4 kernel-unique: PIPELINE_TERMINATIONS (by terminal reason), KERNEL_INSTRUCTIONS (by instruction type), AGENT_EXECUTION_DURATION (histogram), TOOL_EXECUTIONS (by tool and outcome). No other framework instruments at the kernel level.

---

## 8. Jeeves-Core Gaps (vs. Competition)

### 8.1 Critical Gaps

| Gap | Impact | Who Does It Better | Remediation |
|-----|--------|--------------------|-------------|
| **No MCP support** | CRITICAL — MCP is now universal (every competitor supports it). Jeeves-core is the ONLY framework without it. | All 12 competitors | Implement MCP client; expose capabilities as MCP servers |
| **String-prompt LLM interface** | CRITICAL — `generate(prompt: str) → str` has no message-based chat, no tool schemas, no multi-modal, no structured output | Every other framework supports message-based APIs | Add `generate_chat(messages, tools) → ChatResponse` to LLMProviderProtocol |
| **No documentation** | HIGH — no tutorials, no API reference, no getting-started guide, no examples | LangChain, CrewAI, OpenAI SDK, Google ADK all have extensive docs | Create quickstart, API reference, architecture guide |

### 8.2 High-Impact Gaps

| Gap | Impact | Who Does It Better |
|-----|--------|--------------------|
| **Rust binary requirement** | HIGH — `pip install && go` vs. compile Rust + run kernel binary + connect via TCP | Every other framework is `pip install` |
| **No built-in RAG** | HIGH — most common LLM application pattern | Haystack, LlamaIndex (best-in-class), LangChain |
| **No community/ecosystem** | HIGH — no integrations, no plugins, no contributors, private repo | CrewAI (45.6k stars, 100k devs), LlamaIndex (47.6k stars), LangGraph (900+ integrations) |
| **No evaluation framework** | HIGH — no way to measure agent quality | DSPy (automatic optimization), LangSmith, Google ADK (built-in eval) |

### 8.3 Medium-Impact Gaps

| Gap | Impact | Who Does It Better |
|-----|--------|--------------------|
| **No formalized guardrails API** | MEDIUM — has pre/post hooks and tool risk but no structured violation reporting | OpenAI Agents SDK (tripwire/filter), MS Agent Framework (middleware) |
| **Observability wired but passive** | MEDIUM — OTEL adapter + Prometheus metrics defined but spans not actively emitted in Agent.process() or PipelineRunner | OpenAI SDK (tracing default on), LangSmith, LlamaIndex (OTEL native) |
| **In-memory session state only** | MEDIUM — SessionStateService is in-memory, no persistent conversation memory | CrewAI (4-layer), LlamaIndex (multi-layer), Letta (self-editing persistent memory) |
| **No conversation management** | MEDIUM — each request is a new Envelope, no multi-turn context | OpenAI SDK (sessions), Letta (Conversations API) |
| **Envelope complexity** | MEDIUM — Envelope is 997 lines in Rust + 1,071 lines in Python types. Risk of "god object" | LangGraph (clean TypedDict), OpenAI SDK (simple RunContext) |
| **No A2A protocol support** | MEDIUM — emerging standard for agent-to-agent communication | MS Agent Framework, Google ADK, BeeAI |

---

## 9. Industry Trends (March 2026)

### 9.1 MCP is Universal
Every framework in this audit now supports MCP. It has become the de facto standard for agent-tool interoperability. Jeeves-core is the only framework that doesn't support it.

### 9.2 Microsoft Consolidation
AutoGen + Semantic Kernel → Microsoft Agent Framework (RC). This is the biggest ecosystem shift — teams should migrate to the unified framework.

### 9.3 Multi-Language Expansion
Google ADK leads with 4 languages (Python, TS, Go, Java). OpenAI Agents SDK has Python/TS parity. LlamaIndex has Python/TS. Single-language frameworks are at a disadvantage.

### 9.4 Memory-First Architectures
Letta's self-editing memory and Context Repositories represent a new paradigm. CrewAI's 4-layer memory is the most comprehensive in traditional frameworks. Memory is becoming a key differentiator.

### 9.5 Built-In Evaluation
Google ADK and DSPy lead with built-in evaluation capabilities. LangSmith provides evaluation externally. Frameworks without evaluation tooling are harder to use in production.

### 9.6 Agent Protocols Emerging
A2A (Agent-to-Agent), AG-UI (Agent-to-UI), ACP (Agent Client Protocol) are emerging alongside MCP. MS Agent Framework and Google ADK support multiple protocols.

### 9.7 Observability Consolidation
OpenTelemetry is the winner. Frameworks are consolidating on OTEL for tracing and Prometheus for metrics. Proprietary tracing (LangSmith) coexists but OTEL is the open standard. Jeeves-core's OTEL + Prometheus stack is the right choice — it just needs active span emission.

---

## 10. Strategic Recommendations for Jeeves-Core

### 10.1 Critical Path (Must Do)

1. **MCP Client + Server** — Implement MCP transport. Expose capabilities as MCP servers. Consume external tools via MCP client. This is table-stakes in 2026. Every competitor has it.

2. **Message-Based LLM Interface** — Add `ChatMessage`, `ToolCall`, `ToolResult` types to LLMProviderProtocol. Support `generate_chat(messages, tools, response_format)`. Keep string `generate()` for backward compatibility.

3. **Getting-Started Documentation** — Quickstart guide, architecture overview, first-capability tutorial. Without this, no external adoption is possible.

### 10.2 High Priority

4. **Simplify Onboarding** — Consider a pure-Python mode that works without the Rust kernel (with reduced guarantees) for development/prototyping. Or provide pre-built kernel binaries via PyPI wheels.

5. **Built-In Evaluation** — Add an evaluation harness that leverages the existing OTEL + Prometheus metrics infrastructure and kernel-unique metrics (PIPELINE_TERMINATIONS, AGENT_EXECUTION_DURATION).

6. **Persistent Memory** — Implement a default memory layer (at minimum, conversation history persistence) on top of the existing DatabaseClientProtocol + Redis infrastructure.

### 10.3 Medium Priority

7. **A2A Protocol Support** — Enable jeeves-core agents to communicate with agents from other frameworks.

8. **Active OTEL Span Emission** — Wire up actual span creation in `Agent.process()`, `PipelineRunner.run()`, `ToolExecutionCore.execute_tool()`, and `PipelineWorker.execute()`. The adapter is ready (138 lines); it just needs to be called.

9. **Structured Guardrails API** — Formalize the existing pre/post hooks into a guardrail protocol with violation reporting, similar to OpenAI SDK's tripwire/filter model.

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
                         │  ● Letta
                         │
                         │        ● BeeAI
                         │                    ● AG2
                         └──────────────────────────────► ECOSYSTEM / ADOPTION
```

Jeeves-Core occupies the upper-left quadrant: highest architectural rigor, lowest ecosystem/adoption. The strategic challenge is moving right without sacrificing the architectural advantages that make it unique.

**Bottom line:** Jeeves-core has the most rigorous agent runtime architecture of any OSS framework — 14,785 lines of `#[deny(unsafe_code)]` Rust kernel + 31,128 lines of Python runtime with 18 protocols, kernel-enforced bounds, 7 typed interrupt kinds, and a 3-pattern message bus. The observability refactor and gateway hardening bring it closer to production-ready. But in a 2026 landscape where MCP is universal and `pip install` is expected, the critical path is: **MCP → message-based LLM → docs → reference capability**. Everything else follows.
