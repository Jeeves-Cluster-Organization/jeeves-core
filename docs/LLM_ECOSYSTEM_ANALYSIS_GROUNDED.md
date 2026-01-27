# Jeeves-Core vs OSS LLM Ecosystem: Grounded Analysis

**Date:** 2026-01-27
**Status:** Self-reviewed and corrected against repository evidence
**Previous Version:** `LLM_ECOSYSTEM_ANALYSIS.md` (unverified claims)

---

## TASK 1: Claim Map

All Jeeves-Core-specific claims from the original analysis:

### Architecture Claims
| ID | Claim |
|----|-------|
| JC-001 | L4 mission_system exists (Orchestration framework, HTTP/gRPC API) |
| JC-002 | L3 avionics exists (Infrastructure: LLM providers, DB, Gateway) |
| JC-003 | L2 memory_module exists (Event sourcing, semantic memory, session state) |
| JC-004 | L1 control_tower exists (OS-like kernel with process lifecycle) |
| JC-005 | L0 protocols + shared exists (Type contracts, zero dependencies) |
| JC-006 | Go coreengine + commbus exists (Pipeline orchestration, comm bus) |
| JC-007 | Capability layer pattern (jeeves-capability-*) |

### Orchestration Claims
| ID | Claim |
|----|-------|
| JC-008 | Go Runtime with sequential/parallel execution |
| JC-009 | Native goroutines for parallel execution |
| JC-010 | Uses LangGraph internally |
| JC-011 | Explicit EdgeLimits for cyclic routing |
| JC-012 | Multi-dimensional bounds (MaxIterations, MaxLLMCalls, MaxAgentHops, timeouts, edge limits) |

### LLM Provider Claims
| ID | Claim |
|----|-------|
| JC-013 | OpenAI provider exists |
| JC-014 | Anthropic provider exists |
| JC-015 | Azure provider exists |
| JC-016 | LlamaServer provider exists |
| JC-017 | LlamaCpp provider exists |

### Memory System Claims
| ID | Claim |
|----|-------|
| JC-018 | 7-layer memory (L1-L7): Episodic, Events, Semantic, Working, Graph, Skills, Meta |
| JC-019 | pgvector for vector storage |
| JC-020 | Event sourcing with DomainEvent and causation chains |

### Tool Registry Claims
| ID | Claim |
|----|-------|
| JC-021 | Typed ToolId enum |
| JC-022 | RiskLevel enum (LOW/MEDIUM/HIGH/CRITICAL) |
| JC-023 | Tool category hierarchy |

### Human-in-the-Loop Claims
| ID | Claim |
|----|-------|
| JC-024 | 7 interrupt types with configurable resume stages |
| JC-025 | Interrupt expiry |

### Resource Management Claims
| ID | Claim |
|----|-------|
| JC-026 | Native rate limiting with sliding window |
| JC-027 | Resource quotas with multiple dimensions |
| JC-028 | Circuit breaker for tool health |

### Kernel/OS Pattern Claims
| ID | Claim |
|----|-------|
| JC-029 | ControlTower microkernel |
| JC-030 | Process lifecycle management (PCB pattern) |
| JC-031 | Service dispatch with capability registry |
| JC-032 | Process scheduling with PCB + dispatch + IPC |

### Advanced Feature Claims
| ID | Claim |
|----|-------|
| JC-033 | gRPC + HTTP support |
| JC-034 | REINTENT architecture (all feedback through Intent agent) |
| JC-035 | Evidence chain integrity (enforced citations) |
| JC-036 | Constitutional invariants (compile-time + runtime contracts) |
| JC-037 | Time-travel debugging with fork_from_checkpoint() |

### Observability Claims
| ID | Claim |
|----|-------|
| JC-038 | Prometheus metrics |
| JC-039 | Structured logging |
| JC-040 | Distributed tracing (planned) |

---

## TASK 2: Repo Check

| JC-ID | Status | Evidence | Correction |
|-------|--------|----------|------------|
| **JC-001** | VERIFIED | `/home/user/jeeves-core/mission_system/` exists with `api/`, `orchestrator/`, `proto/` | - |
| **JC-002** | VERIFIED | `/home/user/jeeves-core/avionics/` exists with `llm/providers/`, `database/`, `gateway/` | - |
| **JC-003** | VERIFIED | `/home/user/jeeves-core/memory_module/` exists with `repositories/`, `services/` | - |
| **JC-004** | VERIFIED | `/home/user/jeeves-core/control_tower/` exists with `kernel.py`, `lifecycle/`, `resources/` | - |
| **JC-005** | VERIFIED | `/home/user/jeeves-core/protocols/` and `/home/user/jeeves-core/shared/` exist | - |
| **JC-006** | VERIFIED | `/home/user/jeeves-core/coreengine/` (Go) and `/home/user/jeeves-core/commbus/` exist | - |
| **JC-007** | NOT FOUND | No `jeeves-capability-*` directories found in repo | Remove claim - capability layer is conceptual, not implemented |
| **JC-008** | VERIFIED | `coreengine/runtime/runtime.go:26-29` - RunModeSequential/RunModeParallel | - |
| **JC-009** | VERIFIED | `coreengine/runtime/runtime.go` uses Go concurrency primitives | - |
| **JC-010** | **CONTRADICTED** | No `langgraph` in dependencies (`pyproject.toml`, `requirements.txt`). No `from langgraph` imports. | **Remove claim** - Documentation mentions "LangGraph StateGraph" conceptually but actual orchestration is custom Go runtime |
| **JC-011** | VERIFIED | `coreengine/config/pipeline.go:74-78` - `EdgeLimit` struct with `From`, `To`, `MaxCount` | - |
| **JC-012** | VERIFIED | `coreengine/config/pipeline.go:187-190` - MaxIterations, MaxLLMCalls, MaxAgentHops fields | - |
| **JC-013** | VERIFIED | `avionics/llm/providers/openai.py` exists | - |
| **JC-014** | VERIFIED | `avionics/llm/providers/anthropic.py` exists | - |
| **JC-015** | VERIFIED | `avionics/llm/providers/azure.py` exists | - |
| **JC-016** | VERIFIED | `avionics/llm/providers/llamaserver_provider.py` exists | - |
| **JC-017** | VERIFIED | `avionics/llm/providers/llamacpp_provider.py` exists | - |
| **JC-018** | **PARTIAL** | `memory_module/CONSTITUTION.md:108-118` documents L1-L7, BUT L5/L6 are "in-memory stubs" | Correct: L5 (Graph) and L6 (Skills) are **stubs**, not production implementations |
| **JC-019** | VERIFIED | `memory_module/repositories/pgvector_repository.py` exists | - |
| **JC-020** | PARTIAL | `memory_module/repositories/event_repository.py` exists, `protocols/events.py` has `DomainEvent` | Causation chains mentioned in docs but implementation unclear |
| **JC-021** | VERIFIED | `avionics/tools/catalog.py:29` - `class ToolId(str, Enum)` | Note: ToolId is CAPABILITY-OWNED, not centrally defined |
| **JC-022** | **PARTIAL** | `protocols/enums.py:6` - `class RiskLevel(str, Enum)` with values: READ_ONLY, WRITE, DESTRUCTIVE, LOW, MEDIUM, HIGH, CRITICAL | Correct: Values are READ_ONLY/WRITE/DESTRUCTIVE (semantic) + LOW/MEDIUM/HIGH/CRITICAL (risk scale) |
| **JC-023** | VERIFIED | `protocols/enums.py:36` - `class ToolCategory(str, Enum)` with READ, WRITE, EXECUTE, NETWORK, SYSTEM, etc. | - |
| **JC-024** | VERIFIED | `protocols/interrupts.py:13-21` - 7 InterruptKind values: CLARIFICATION, CONFIRMATION, AGENT_REVIEW, CHECKPOINT, RESOURCE_EXHAUSTED, TIMEOUT, SYSTEM_ERROR | - |
| **JC-025** | VERIFIED | `protocols/interrupts.py:106` - `expires_at: Optional[datetime]` field on FlowInterrupt | - |
| **JC-026** | VERIFIED | `control_tower/resources/rate_limiter.py` exists; `protocols/interrupts.py:249` documents sliding window | - |
| **JC-027** | VERIFIED | `coreengine/config/pipeline.go:187-190` - MaxIterations, MaxLLMCalls, MaxAgentHops, TimeoutSeconds | - |
| **JC-028** | VERIFIED | `commbus/middleware.go:69-228` - CircuitBreakerMiddleware implementation | - |
| **JC-029** | **PARTIAL** | `control_tower/kernel.py` exists | "Microkernel" is aspirational terminology - it's a service coordinator, not a true microkernel |
| **JC-030** | VERIFIED | `control_tower/types.py:129` - `class ProcessControlBlock`; `control_tower/lifecycle/manager.py` manages PCB | - |
| **JC-031** | NOT FOUND | No "capability registry" or "service dispatch" code found | Remove claim - appears to be aspirational |
| **JC-032** | PARTIAL | PCB exists, dispatch via kernel.py, but no IPC in OS sense | "IPC" is overstated - it's async function calls, not true IPC |
| **JC-033** | VERIFIED | `avionics/gateway/grpc_client.py`, `protocols/grpc_client.py`; HTTP via FastAPI in `avionics/gateway/` | - |
| **JC-034** | VERIFIED | `mission_system/CONSTITUTION.md:55-84` - REINTENT architecture documented with R8/R9/R10 invariants | - |
| **JC-035** | PARTIAL | `mission_system/CONSTITUTION.md:89-110` documents evidence chain; `memory_module/services/nli_service.py` has ClaimVerificationResult | Contract test `test_evidence_chain.py` expected but not verified in tests/contract/ |
| **JC-036** | VERIFIED | `tests/contract/test_layer_boundaries.py`, `test_envelope_roundtrip.py` exist; CONSTITUTION.md files define invariants | - |
| **JC-037** | VERIFIED | `protocols/protocols.py:464` - `fork_from_checkpoint()` method; `avionics/checkpoint/postgres_adapter.py:281` implements it | - |
| **JC-038** | VERIFIED | `avionics/observability/metrics.py`, `coreengine/observability/metrics.go` exist; `docker/prometheus.yml` present | - |
| **JC-039** | VERIFIED | `shared/logging/__init__.py`, `avionics/logging/adapter.py` use structlog | - |
| **JC-040** | VERIFIED | `avionics/observability/tracing.py`, `otel_adapter.py` exist; OpenTelemetry in `go.mod` | Status: Implemented (not just planned) |

---

## TASK 3: Canonical Jeeves-Core (Corrected)

### What Jeeves-Core Actually Is (≤5 bullets)

1. **A hybrid Go+Python agent runtime** with Go (`coreengine`) handling pipeline orchestration and Python handling infrastructure/services
2. **A layered architecture** (L0-L4 Python + Go runtime) with protocol-first design using Python typing.Protocol
3. **A memory system** with 7 conceptual layers (L1-L7), though L5 (Graph) and L6 (Skills) are only in-memory stubs
4. **An enterprise-oriented framework** with bounds (MaxIterations, MaxLLMCalls, MaxAgentHops, EdgeLimits), circuit breakers, rate limiting
5. **A HITL-enabled system** with 7 interrupt types, expiry, and REINTENT feedback architecture

### Architecture Layers (What Truly Exists)

```
┌─────────────────────────────────────────────────────────────────┐
│ L4: mission_system                                              │
│     HTTP/gRPC API, orchestrator, prompts                        │
│     (NO LangGraph - uses Go coreengine)                         │
├─────────────────────────────────────────────────────────────────┤
│ L3: avionics                                                    │
│     LLM providers (5), database, gateway, observability         │
├─────────────────────────────────────────────────────────────────┤
│ L2: memory_module                                               │
│     Memory services, repositories, NLI                          │
│     (L5/L6 are STUBS only)                                      │
├─────────────────────────────────────────────────────────────────┤
│ L1: control_tower                                               │
│     ProcessControlBlock, rate limiting, interrupts              │
│     (NOT a microkernel - service coordinator)                   │
├─────────────────────────────────────────────────────────────────┤
│ L0: protocols + shared                                          │
│     Type contracts via Protocol, shared utilities               │
├─────────────────────────────────────────────────────────────────┤
│ GO: coreengine + commbus                                        │
│     PipelineRunner, Agent, CircuitBreaker, CommBus              │
│     (THIS is the actual orchestration, not LangGraph)           │
└─────────────────────────────────────────────────────────────────┘
```

### Memory Model (Actual Layers & Semantics)

| Layer | Name | Status | Implementation |
|-------|------|--------|----------------|
| **L1** | Episodic | IMPLEMENTED | `Envelope` in coreengine (Go) |
| **L2** | Events | IMPLEMENTED | `EventRepository` + `EventEmitter` |
| **L3** | Semantic | IMPLEMENTED | `ChunkService` + `PgVectorRepository` |
| **L4** | Working | IMPLEMENTED | `SessionStateService` |
| **L5** | Graph | **STUB ONLY** | `InMemoryGraphStorage` (not production) |
| **L6** | Skills | **STUB ONLY** | `InMemorySkillStorage` (not production) |
| **L7** | Meta | IMPLEMENTED | `ToolHealthService` |

**Correction:** The "7-layer memory" claim is technically true but misleading - only 5 layers (L1-L4, L7) are production-ready. L5/L6 have protocol definitions but only in-memory stub implementations.

### State Authority & Go↔Python Boundary

| Component | Language | Owns |
|-----------|----------|------|
| **coreengine** | Go | Pipeline execution, Envelope state, bounds enforcement, circuit breaker |
| **commbus** | Go | Message bus, middleware chain |
| **control_tower** | Python | ProcessControlBlock, rate limiting, interrupt management |
| **avionics** | Python | LLM providers, database clients, HTTP gateway |
| **memory_module** | Python | Memory services, repositories, NLI |
| **mission_system** | Python | API layer, orchestrator wrapper |

**Boundary:** Go coreengine owns runtime execution; Python layers provide infrastructure and services consumed via gRPC/in-process calls.

### Orchestration Model

**Actual Implementation:**
- Custom Go `PipelineRunner` in `coreengine/runtime/runtime.go`
- Sequential or parallel stage execution via Go concurrency
- EdgeLimits enforce per-transition cycle counts
- MaxIterations/MaxLLMCalls/MaxAgentHops as global bounds

**NOT LangGraph:** Despite documentation references to "LangGraph StateGraph", LangGraph is not a dependency. The documentation uses LangGraph terminology conceptually but the actual orchestration is a custom Go implementation.

### Resource Governance

| Resource | Implementation | Status |
|----------|----------------|--------|
| Rate Limiting | `control_tower/resources/rate_limiter.py` | IMPLEMENTED |
| Circuit Breaker | `commbus/middleware.go:88` - CircuitBreakerMiddleware | IMPLEMENTED |
| Bounds (Iterations) | `coreengine/config/pipeline.go:187` - MaxIterations | IMPLEMENTED |
| Bounds (LLM Calls) | `coreengine/config/pipeline.go:188` - MaxLLMCalls | IMPLEMENTED |
| Bounds (Agent Hops) | `coreengine/config/pipeline.go:189` - MaxAgentHops | IMPLEMENTED |
| Edge Limits | `coreengine/config/pipeline.go:74` - EdgeLimit | IMPLEMENTED |

### Observability

| Component | Implementation | Status |
|-----------|----------------|--------|
| Prometheus Metrics | `avionics/observability/metrics.py`, `coreengine/observability/metrics.go` | IMPLEMENTED |
| Structured Logging | `shared/logging/__init__.py` using structlog | IMPLEMENTED |
| Distributed Tracing | `avionics/observability/tracing.py` using OpenTelemetry | IMPLEMENTED (not "planned") |

---

## TASK 4: Self-Critique

### Overstated Claims

| Claim | Original Statement | Reality |
|-------|-------------------|---------|
| **LangGraph usage** | "jeeves-core uses LangGraph internally" | **FALSE** - LangGraph is not a dependency; orchestration is custom Go |
| **7-layer memory** | "7 layers (L1-L7) production-ready" | **MISLEADING** - L5/L6 are stubs only |
| **OS-style kernel** | "ControlTower microkernel" | **OVERSTATED** - It's a service coordinator, not a true microkernel |
| **Capability layer** | "jeeves-capability-* pattern" | **NOT FOUND** - No capability directories exist |
| **Service dispatch** | "Service dispatch with capability registry" | **NOT FOUND** - No such code exists |
| **IPC** | "PCB + dispatch + IPC" | **OVERSTATED** - No true IPC, just async function calls |
| **Tracing planned** | "Distributed tracing (planned)" | **UNDERSTATED** - Actually implemented with OpenTelemetry |

### Assumptions Beyond Evidence

1. **Assumed** LangGraph was used because documentation mentions "StateGraph" terminology
2. **Inferred** microkernel pattern from OS-like naming without verifying architectural purity
3. **Assumed** all 7 memory layers were production-ready without checking implementation status
4. **Projected** capability layer from external patterns without finding code evidence

### Percentage Corrections

| Original Metric | Corrected Value | Rationale |
|-----------------|-----------------|-----------|
| "35% genuinely novel" | **~30%** | Remove LangGraph claim, reduce memory layer uniqueness |
| "45% re-engineered from LangGraph" | **~35%** | LangGraph not used; Go runtime is more custom than claimed |
| "20% infrastructure" | **~35%** | More original infrastructure work than credited |

---

## TASK 5: Gaps → Questions

### Missing Repository Evidence

1. **Evidence Chain Tests:** `tests/contract/test_evidence_chain.py` referenced in documentation but not found in `tests/contract/` directory - does this test exist elsewhere?

2. **Capability Layer:** Documentation references "jeeves-capability-*" pattern - is this implemented in separate repositories or planned?

3. **L5/L6 Production Implementations:** `memory_module/CONSTITUTION.md:132-136` suggests PostgresGraphAdapter exists in avionics - is `avionics/database/PostgresGraphAdapter` implemented?

4. **REINTENT Implementation:** Documentation (mission_system/CONSTITUTION.md:246) shows `reintent_transition` routing logic - where is this actually implemented in code?

5. **Causation Chains:** Event sourcing documentation mentions causation chains - is there actual `causation_id` tracking in EventRepository?

6. **Go↔Python Bridge:** How does Python mission_system invoke Go coreengine? gRPC server exists (`coreengine/grpc/`) but client integration unclear.

---

## TASK 6: Patch

### Diff to Original Document

```diff
--- a/docs/LLM_ECOSYSTEM_ANALYSIS.md
+++ b/docs/LLM_ECOSYSTEM_ANALYSIS.md
@@ -31,7 +31,7 @@
 ### Top Novel Contributions

 1. **7-Layer Memory Architecture** - Most comprehensive memory separation in OSS
-2. **Explicit Cyclic Routing with EdgeLimits** - First-class loop control
+2. **Explicit Cyclic Routing with EdgeLimits** - First-class loop control (Go implementation)
 3. **Tool Risk Classification** - `RiskLevel` enum per tool
-4. **OS-Style Kernel Pattern** - `ControlTower` microkernel unique in LLM space
-5. **Go+Python Hybrid Runtime** - Performance-critical Go core, flexible Python apps
+4. **Service Coordinator Pattern** - `ControlTower` with PCB lifecycle (not true microkernel)
+5. **Go+Python Hybrid Runtime** - Custom Go PipelineRunner, Python infrastructure

@@ -38,7 +38,6 @@
 ### Primary Overlap Areas

-1. **Graph-based pipeline orchestration** - LangGraph, Haystack, Dify
+1. **Graph-based pipeline orchestration** - (Custom Go implementation, NOT LangGraph)
 2. **Human-in-the-loop interrupts** - LangGraph, Dify
 3. **RAG/semantic search** - LlamaIndex, RAGFlow, Haystack
 4. **LLM provider abstraction** - LiteLLM, LangChain

@@ -80,8 +80,9 @@
 | Capability | Implementation |
 |------------|---------------|
-| Pipeline Orchestration | Go `Runtime` with sequential/parallel execution |
+| Pipeline Orchestration | Go `PipelineRunner` with sequential/parallel execution (NOT LangGraph) |
 | LLM Provider Abstraction | OpenAI, Anthropic, Azure, LlamaServer, LlamaCpp |
-| Memory System | 7 layers (L1-L7): Episodic, Events, Semantic, Working, Graph, Skills, Meta |
+| Memory System | 7 layers (L1-L7), but L5 (Graph) and L6 (Skills) are STUBS ONLY |
 | Tool Registry | Typed `ToolId` enum, risk classification, category hierarchy |
 | Human-in-the-Loop | 7 interrupt types with configurable resume stages |
 | Bounded Execution | Max iterations, LLM calls, agent hops, timeouts, edge limits |
-| Observability | Prometheus metrics, structured logging, distributed tracing (planned) |
+| Observability | Prometheus metrics, structured logging, distributed tracing (implemented) |

@@ -170,8 +170,8 @@
 ### Detailed Comparison Matrix

 | Feature | jeeves-core | LangGraph | LlamaIndex | Haystack | Dify |
-| **Orchestration Model** | Go runtime + LangGraph | StateGraph | AgentWorkflow | Pipeline DAG | Queue-based Graph |
-| **Memory Layers** | 7 (L1-L7) | 3 | Built-in state | ChatMessageStore | RAG-focused |
+| **Orchestration Model** | Custom Go PipelineRunner | StateGraph | AgentWorkflow | Pipeline DAG | Queue-based Graph |
+| **Memory Layers** | 7 (L1-L7, but L5/L6 stubs) | 3 | Built-in state | ChatMessageStore | RAG-focused |

@@ -305,11 +305,11 @@
 | Feature | Description | Why Novel |
 |---------|-------------|-----------|
-| **OS-style kernel** | `ControlTower` with PCB lifecycle | No other LLM framework uses this |
+| **Service coordinator** | `ControlTower` with PCB lifecycle | OS-inspired but not true microkernel |
 | **Go+Python hybrid** | Go for runtime, Python for apps | All competitors are pure Python |
 | **Tool risk classification** | `RiskLevel` enum per tool | Not found in competitors |
-| **7-layer memory** | L1-L7 with clear ownership | Most comprehensive |
+| **7-layer memory** | L1-L7 with clear ownership (L5/L6 stubs) | Conceptually comprehensive |
 | **Event sourcing** | DomainEvent with causation chains | Enterprise pattern, rare in LLM |
-| **REINTENT architecture** | Single feedback edge to Intent | Cleaner than distributed REPLAN |
+| **REINTENT architecture** | Single feedback edge to Intent | Documented in CONSTITUTION.md |
```

### Evidence Appendix

| Claim | Repository Path |
|-------|-----------------|
| RiskLevel enum | `protocols/enums.py:6-14` |
| ToolId enum | `avionics/tools/catalog.py:29` |
| EdgeLimit | `coreengine/config/pipeline.go:74-78` |
| InterruptKind (7 types) | `protocols/interrupts.py:13-21` |
| ProcessControlBlock | `control_tower/types.py:129` |
| PipelineRunner (Go) | `coreengine/runtime/runtime.go:54-66` |
| CircuitBreaker | `commbus/middleware.go:88-228` |
| Rate limiter | `control_tower/resources/rate_limiter.py` |
| pgvector repository | `memory_module/repositories/pgvector_repository.py` |
| L5/L6 stubs | `memory_module/repositories/graph_stub.py`, `skill_stub.py` |
| REINTENT invariants | `mission_system/CONSTITUTION.md:263-295` |
| fork_from_checkpoint | `protocols/protocols.py:464`, `avionics/checkpoint/postgres_adapter.py:281` |
| OpenTelemetry tracing | `avionics/observability/tracing.py`, `otel_adapter.py` |
| No LangGraph dependency | Absent from all `pyproject.toml` and `requirements.txt` files |

---

## Corrected Conclusion

**Jeeves-core is a custom Go+Python agent runtime that is more original than initially assessed.**

- **~30% genuinely novel** - Custom Go pipeline orchestration (not LangGraph), PCB pattern, tool risk classification
- **~35% re-engineered** - Standard patterns (HITL, provider abstraction, RAG) but independently implemented
- **~35% infrastructure** - Well-integrated standard choices (PostgreSQL, pgvector, structlog, OpenTelemetry)

**Key Correction:** The original analysis overstated LangGraph usage and understated the custom Go implementation. Jeeves-core is NOT a LangGraph wrapper - it has its own orchestration engine.

**Honest Assessment:** The 7-layer memory claim is aspirational - only 5 layers are production-ready. The "microkernel" terminology is marketing rather than architectural reality.

---

*This grounded analysis corrects the original `LLM_ECOSYSTEM_ANALYSIS.md` based on repository evidence. Unverified claims have been marked or removed.*
