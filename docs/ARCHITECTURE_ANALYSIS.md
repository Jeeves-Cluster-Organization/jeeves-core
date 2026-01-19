# Jeeves-Core Architecture Analysis

**Date:** 2026-01-19  
**Scan:** Full codebase review

---

## 1. Architecture Summary (15 Lines)

Jeeves-Core is a layered agentic runtime combining Python application logic (L0-L4) with a Go orchestration engine. The core abstractions are **GenericEnvelope** (state carrier), **PipelineConfig** (agent DAG), and **Runtime** (execution engine). L0 (`jeeves_protocols`/`jeeves_shared`) defines zero-dependency type contracts. L1 (`jeeves_control_tower`) provides an OS-like kernel with process lifecycle, resource quotas, IPC via CommBus, and interrupt handling. L2 (`jeeves_memory_module`) implements event sourcing, semantic search (pgvector), and session state. L3 (`jeeves_avionics`) houses infrastructure: LLM providers (OpenAI/Anthropic/Azure/LlamaCpp), database clients, observability, and the Go bridge. L4 (`jeeves_mission_system`) delivers orchestration APIs, gRPC/HTTP services, and capability registration. The Go `coreengine` is authoritative for pipeline execution (sequential/parallel), bounds enforcement, and envelope operations. A `commbus` package in Go provides pub/sub, query/response, and command patterns. Python and Go interoperate via gRPC or subprocess-based bridge. Interrupts (clarification, confirmation, agent review) pause execution and resume on user response. Cyclic routing is permitted with edge limits and global iteration caps. All layers follow strict import boundaries enforced by constitution documents. The design is protocol-first: components depend on protocols, not implementations.

---

## 2. Truth Table: Claimed Layers vs Actual Code Modules

| Layer | Claimed Responsibility | Actual Modules Present | Implementation Status |
|-------|----------------------|----------------------|----------------------|
| **L0: jeeves_protocols** | Zero-dep type contracts, protocols | `core.py`, `config.py`, `envelope.py`, `agents.py`, `protocols.py`, `capability.py`, `memory.py`, `events.py`, `interrupts.py`, `utils.py`, `grpc_client.py` | ✅ **COMPLETE** - All protocols defined, dataclasses for config/envelope/memory, capability registry implemented |
| **L0: jeeves_shared** | Shared utilities (UUID, logging, serialization) | `id_generator.py`, `uuid_utils.py`, `serialization.py`, `fuzzy_matcher.py`, `testing.py`, `logging/` | ✅ **COMPLETE** - Basic utilities present |
| **L1: jeeves_control_tower** | OS-like kernel (process lifecycle, resources, IPC) | `kernel.py`, `lifecycle/manager.py`, `resources/tracker.py`, `ipc/commbus.py`, `ipc/coordinator.py`, `events/aggregator.py`, `services/interrupt_service.py` | ✅ **COMPLETE** - ControlTower kernel, LifecycleManager, ResourceTracker, CommBusCoordinator, EventAggregator, InterruptService all implemented |
| **L2: jeeves_memory_module** | Event sourcing, semantic memory, session state | `repositories/` (event, session_state, chunk, pgvector, trace), `services/` (embedding, session_state, tool_health, trace_recorder), `handlers.py`, `intent_classifier.py` | ⚠️ **PARTIAL** - Event/session repos present, pgvector repo exists but semantic search integration incomplete |
| **L3: jeeves_avionics** | Infrastructure (LLM, DB, Gateway, Observability) | `llm/` (5 providers), `database/` (postgres, redis, factory), `gateway/` (FastAPI routers, WebSocket, SSE), `observability/` (OTEL, metrics), `interop/go_bridge.py`, `checkpoint/`, `tools/` | ✅ **COMPLETE** - LLM providers (OpenAI, Anthropic, Azure, LlamaServer, Mock), database factory, gateway with routers, observability adapters |
| **L4: jeeves_mission_system** | Orchestration API, capability registration | `api/` (server, health, chat), `orchestrator/` (flow_service, event_context), `adapters.py`, `bootstrap.py`, `verticals/`, `config/`, `prompts/` | ⚠️ **PARTIAL** - API layer present, flow service implemented, but vertical registry is mostly stub |
| **GO: coreengine** | Pipeline orchestration (sequential + parallel) | `runtime/runtime.go`, `envelope/generic.go`, `agents/unified.go`, `config/pipeline.go`, `grpc/server.go`, `tools/executor.go` | ✅ **COMPLETE** - Runtime with Execute/Run/RunParallel/Resume, GenericEnvelope with bounds, UnifiedAgent |
| **GO: commbus** | Communication bus (pub/sub, query, command) | `bus.go`, `protocols.go`, `messages.go`, `middleware.go`, `errors.go` | ✅ **COMPLETE** - InMemoryCommBus with Publish/Send/QuerySync, middleware chain, subscriber management |

### Detailed Assessment

| Component | Claimed | Code Exists | Tests Exist | Integration Verified |
|-----------|---------|-------------|-------------|---------------------|
| GenericEnvelope (Go) | ✓ | ✅ `envelope/generic.go` (983 lines) | ✅ `generic_test.go` | ✅ |
| GenericEnvelope (Python) | ✓ | ✅ `jeeves_protocols/envelope.py` | ✅ `test_envelope.py` | ⚠️ Round-trip tests missing |
| Runtime (Go) | ✓ | ✅ `runtime/runtime.go` | ✅ `runtime_test.go` | ✅ |
| UnifiedAgent (Go) | ✓ | ✅ `agents/unified.go` | ✅ `contracts_test.go` | ✅ |
| UnifiedAgent (Python) | ✓ | ✅ `jeeves_protocols/agents.py` | ⚠️ No dedicated tests | ⚠️ |
| ControlTower kernel | ✓ | ✅ `kernel.py` | ✅ `test_kernel_integration.py` | ✅ |
| InterruptService | ✓ | ✅ `services/interrupt_service.py` | ✅ `test_interrupt_service.py` | ✅ |
| ResourceTracker | ✓ | ✅ `resources/tracker.py` | ✅ `test_resource_tracker.py` | ✅ |
| LLM Providers | ✓ | ✅ 5 providers in `llm/providers/` | ✅ `test_llm_providers.py` | ⚠️ Mock-only in tests |
| Semantic Search | ✓ | ⚠️ `pgvector_repository.py` exists | ⚠️ No dedicated tests | ❌ Not wired end-to-end |
| Entity Graph (L5) | ✓ | ⚠️ `graph_stub.py` (stub only) | ❌ | ❌ |
| Skill Storage (L6) | ✓ | ⚠️ `skill_stub.py` (stub only) | ❌ | ❌ |
| gRPC Go Server | ✓ | ✅ `grpc/server.go` | ⚠️ Minimal tests | ⚠️ Proto stubs incomplete |
| gRPC Python Client | ✓ | ✅ `grpc_client.py` | ⚠️ No tests | ❌ Stubs not generated |
| Go Binary Bridge | ✓ | ✅ `interop/go_bridge.py` | ⚠️ No tests | ⚠️ Binary not built |

---

## 3. Top 5 Missing/Implicit Components

The design documents and protocols reference components that code doesn't fully provide:

### 1. **Envelope Round-Trip Contract Tests (Go ↔ Python)**
- **Claimed:** CONTRACTS.md Contract 11 specifies "Go → Python → Go must be lossless"
- **Missing:** No actual round-trip tests exist in `tests/contracts/`
- **Impact:** FlowInterrupt serialization drift risk (Go has typed struct, Python uses `Dict[str, Any]`)
- **Location needed:** `tests/contract/test_envelope_roundtrip.py`

### 2. **Generated gRPC Stubs for Python**
- **Claimed:** `grpc_client.py` imports from `jeeves_protocols.grpc_stub`
- **Missing:** No `grpc_stub` module exists; proto compilation step not automated
- **Impact:** Python cannot communicate with Go gRPC server without manual proto compilation
- **Location needed:** `jeeves_protocols/grpc_stub.py` (generated from `coreengine/proto/jeeves_core.proto`)

### 3. **Built Go Binaries (go-envelope, gRPC server)**
- **Claimed:** `GoEnvelopeBridge` requires `go-envelope` binary in PATH; gRPC client requires running server
- **Missing:** No build scripts, no CI step to compile Go binaries
- **Impact:** Python-Go interop fails at runtime with `GoNotAvailableError`
- **Location needed:** `scripts/build_go.sh`, CI job for Go compilation

### 4. **Semantic Search End-to-End Wiring**
- **Claimed:** L4 Memory claims "semantic search" with pgvector
- **Present:** `pgvector_repository.py` with basic CRUD
- **Missing:** No embedding pipeline integration, no search service wired to CommBus handlers
- **Impact:** `SemanticSearchProtocol` is defined but not callable at runtime
- **Location needed:** `jeeves_memory_module/services/semantic_search_service.py` wired via `handlers.py`

### 5. **Memory Layers L5-L6 (Entity Graph, Skill Storage)**
- **Claimed:** HANDOFF.md references 4-layer memory (L1-L4) but protocols define `GraphStorageProtocol`, `SkillStorageProtocol` (L5-L6)
- **Present:** Only stubs (`graph_stub.py`, `skill_stub.py`) with `raise NotImplementedError`
- **Missing:** Actual implementations for entity relationship graphs and learned skill persistence
- **Impact:** Advanced memory features (entity linking, skill reuse) are non-functional
- **Location needed:** `jeeves_memory_module/repositories/graph_repository.py`, `skill_repository.py`

---

## Summary Scores

| Dimension | Score | Notes |
|-----------|-------|-------|
| **Protocol Coverage** | 9/10 | Comprehensive protocols; minor gaps in gRPC stubs |
| **Go Engine Completeness** | 9/10 | Runtime, envelope, agents all implemented; missing binary distribution |
| **Python Layer Completeness** | 7/10 | L0-L1 solid; L2-L4 have gaps in semantic search and verticals |
| **Interop Readiness** | 4/10 | Bridge code exists but binaries not built, stubs not generated |
| **Test Coverage** | 6/10 | Unit tests good; contract/integration tests incomplete |

---

*Generated by architecture scan on 2026-01-19*
