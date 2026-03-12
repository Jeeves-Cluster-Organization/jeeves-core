# Jeeves-Core: Comprehensive Architectural Audit & Comparative Analysis

**Date**: 2026-03-12 (Iteration 8 — graph primitives, Fork/Gate, state merge)
**Scope**: Full codebase audit of jeeves-core (Rust kernel + Python infrastructure) with comparative analysis against 17+ OSS agentic frameworks.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Architecture Deep Dive](#2-architecture-deep-dive)
3. [Comparative Analysis](#3-comparative-analysis)
4. [Strengths](#4-strengths)
5. [Weaknesses](#5-weaknesses)
6. [Documentation Assessment](#6-documentation-assessment)
7. [Improvement Recommendations](#7-improvement-recommendations)
8. [Maturity Assessment](#8-maturity-assessment)
9. [Trajectory Analysis](#9-trajectory-analysis)

---

## 1. Executive Summary

Jeeves-Core is a **Rust micro-kernel + Python infrastructure** framework for AI agent orchestration. The kernel (~15,500 lines Rust, `#[deny(unsafe_code)]`) provides process lifecycle, IPC, interrupt handling, resource quotas, and graph-based pipeline orchestration. The Python layer (~5,200 lines) provides LLM providers, pipeline execution, gateway, and bootstrap.

**Architectural thesis**: A Rust kernel managing Python agents provides enforcement guarantees that in-process Python frameworks cannot match — resource quotas, output schema validation, tool ACLs, and fault isolation are kernel-mediated, not convention-dependent.

**Overall maturity: 3.7/5** (up from 3.6) — graph primitives (Gate/Fork nodes, state merge, Break) close the last orchestration expressiveness gap.

**What changed in iteration 8** (6 commits):
- **Graph primitives**: `NodeKind` enum (Agent/Gate/Fork), state merge strategies (Replace/Append/MergeDict), `Break` instruction, `step_limit`
- **Gate nodes**: Pure routing nodes — evaluate routing without running an agent
- **Fork nodes**: Parallel fan-out — evaluate ALL matching rules, dispatch branches concurrently, with WaitAll/WaitFirst join
- **State schema**: Typed state fields with merge strategies, kernel-managed state dict
- **Dead code deletion**: `InMemoryConversationHistory` deleted (44 lines), stale tests removed
- **MockKernelClient rewrite**: ~500 lines, full-fidelity Gate/Fork dispatch, entry-guard max_visits, WaitFirst join, state merge
- **hooks-as-lists**: `pre_process`/`post_process`/`context_loaders` normalized from single callable to list
- **PipelineWorker consolidation**: ~787 lines, break_loop support, parallel fan-out in kernel loop
- **A2A server expansion**: 254→421 lines
- **Routing evaluator**: Added `State` scope for state-based routing
- **Protocol cleanup**: `ConversationHistoryProtocol` removed (dead after InMemoryConversationHistory deletion)

---

## 2. Architecture Deep Dive

### 2.1 Layer Architecture

```
┌─────────────────────────────────────────────────────┐
│              Capability Layer (external)             │
│  Agents, Prompts, Tools, Domain Logic, Pipelines    │
├─────────────────────────────────────────────────────┤
│          Python Infrastructure (jeeves_core)         │
│  Gateway │ LLM Providers │ Bootstrap │ Orchestrator  │
│  IPC Client │ Memory │ Retrieval │ MCP │ A2A        │
├─────────────────────────────────────────────────────┤
│              IPC (TCP + msgpack)                     │
│  4B length prefix │ 1B type │ msgpack payload        │
│  MAX_FRAME_SIZE = 5MB                                │
├─────────────────────────────────────────────────────┤
│              Rust Micro-Kernel                       │
│  Process Lifecycle │ Resource Quotas │ Interrupts    │
│  Pipeline Orchestrator │ CommBus │ Rate Limiter      │
│  JSON Schema Validation │ Tool ACLs                  │
│  Graph Primitives (Gate/Fork/State)                  │
└─────────────────────────────────────────────────────┘
```

### 2.2 Rust Kernel

#### Process Lifecycle
Unix-inspired state machine: `New → Ready → Running → Waiting/Blocked → Terminated → Zombie`

#### Pipeline Orchestrator (~2,665 lines, 60+ tests)
```
error_next → routing_rules (first match) → default_next → terminate
```

**Graph primitives** (NEW):
- `NodeKind::Agent` — standard agent execution node
- `NodeKind::Gate` — pure routing node, evaluates routing without running an agent
- `NodeKind::Fork` — parallel fan-out, evaluates ALL matching rules, dispatches branches concurrently

**State management** (NEW):
- `StateField { key, merge: MergeStrategy }` — typed state fields
- `MergeStrategy::Replace` — overwrite existing value
- `MergeStrategy::Append` — append to array
- `MergeStrategy::MergeDict` — shallow-merge object keys
- `state_schema` on `PipelineConfig` — declares state fields and merge strategies
- Kernel manages state dict, merges agent outputs per schema

**Control flow** (NEW):
- `Break` instruction — agents can set `_break: true` in output to terminate pipeline early
- `step_limit` on `PipelineConfig` — optional hard cap on total orchestration steps

**Routing Expression Language** (631 lines, 18 tests): 13 operators across 5 field scopes (Current, Agent, Meta, Interrupt, State — State is NEW).

**Parallel execution**: Fork nodes with `JoinStrategy::WaitAll` / `JoinStrategy::WaitFirst`. Per-edge traversal counters prevent infinite loops.

**Instruction types**: `RunAgent`, `RunAgents`, `WaitParallel`, `WaitInterrupt`, `Terminate`.

#### Kernel-Enforced Contracts
- JSON Schema output validation (`jsonschema` crate)
- Tool ACL enforcement (defense-in-depth)
- Resource quotas: `max_llm_calls`, `max_tool_calls`, `max_agent_hops`, `max_iterations`, `max_output_tokens`, timeout
- `max_visits` per-stage entry guard
- Per-user sliding-window rate limiting

#### IPC Protocol (2,247 lines, 7 handlers)
Binary wire format (4B len + 1B type + msgpack), 27 methods across 4 services.

#### CommBus (849 lines, 12 tests)
Pub/sub events, point-to-point commands, request/response queries with timeout.

#### Interrupts (1,041 lines, 16 tests)
7 typed kinds with TTL.

### 2.3 Python Infrastructure

#### Protocol-first DI — 13 `@runtime_checkable` Protocols (trimmed from 14)
- `LLMProviderProtocol`: message-based `chat()`, `chat_with_usage()`, `chat_stream()`, `health_check()`
- `ContextRetrieverProtocol`, `EmbeddingProviderProtocol`
- `ToolRegistryProtocol`, `ToolExecutorProtocol`, `ConfigRegistryProtocol`
- `DatabaseClientProtocol`, `LoggerProtocol`, `AppContextProtocol`
- `ClockProtocol`, `DistributedBusProtocol`

Removed in this iteration: `ConversationHistoryProtocol` (dead after InMemoryConversationHistory deletion).

#### Typed LLM Boundary
All frozen dataclasses: `LLMResult`, `LLMToolCall`, `LLMUsage`.

#### Agent Execution (~769 lines)
- `Agent.process(AgentContext) → Tuple[Dict, Dict]`
- `StreamingAgent(Agent)` — SRP subclass
- hooks-as-lists: `pre_process`, `post_process`, `context_loaders` accept single callable or list, normalized to list
- `_break` output key triggers kernel Break instruction

#### Pipeline Execution (~787 lines)
- `run_kernel_loop()` — free function, handles both RUN_AGENT and RUN_AGENTS (parallel fan-out)
- `PipelineWorker` — capacity-controlled via semaphore, scheduler queue, service registry
- `execute_streaming()` — asyncio.Queue + sentinel pattern
- `break_loop` support in kernel loop

#### Pipeline Configuration
- `AgentConfig` gained `node_kind` field ("Agent", "Gate", "Fork")
- `PipelineConfig` gained `state_schema` and `step_limit`
- `PipelineConfig.graph()` validates edge conflicts (explicit routing_rules + edge-derived rules)
- 13 routing expression operators across 5 field scopes (Current, Agent, Meta, Interrupt, State)

#### Testing Infrastructure
- `MockKernelClient` (~502 lines) — full-fidelity kernel mock: Gate dispatch, Fork fan-out with WaitAll/WaitFirst, entry-guard max_visits, state merge, Break support, bounds extraction from pipeline_config
- `TestPipeline` + `EvaluationRunner`
- `routing_eval.py` (149 lines) — Python routing evaluator with State scope

#### Interoperability
- MCP client+server (528 lines)
- A2A client+server (421+169 = 590 lines, up from 437)

#### Observability
- Active OTEL spans at 5 sites, 14 Prometheus metrics (6 kernel-unique)

---

## 3. Comparative Analysis — 17 Frameworks

### 3.1 Unique Differentiators (vs. ALL 17)

1. **Kernel-enforced contracts** — No other Python-ecosystem framework validates output schemas, enforces tool ACLs, or caps resource consumption at a kernel level.

2. **Graph primitives (Gate/Fork/State)** — LangGraph has conditional edges and parallel branches. Jeeves now has Gate (pure routing without agent execution), Fork (parallel fan-out with WaitAll/WaitFirst), and typed state merge (Replace/Append/MergeDict). This matches LangGraph's expressiveness with kernel-level enforcement.

3. **Rust kernel + Python agents** — Memory safety and fault isolation pure-Python frameworks structurally cannot provide.

4. **Two-phase instruction enrichment** — Orchestrator determines *what*; kernel enriches *how*. Unique separation.

5. **7 typed interrupt kinds** — Most have no HITL or basic breakpoints only.

6. **Both MCP client+server AND A2A client+server** — No other framework has full bidirectional support for both.

7. **TestPipeline + MockKernelClient** — Full-fidelity kernel mock replicating Gate/Fork/state routing. LLM-free pipeline testing. Architecturally unique.

### 3.2 Where Jeeves Trails

| Gap | Leading Framework | Jeeves Status |
|-----|-------------------|---------------|
| Community/ecosystem | AutoGen (40k), CrewAI (25k) | ~0 stars |
| Multi-language SDKs | Semantic Kernel (C#/Py/Java) | Rust+Python only |
| Visual builder | Mastra, Prefect | No GUI |
| Built-in memory | LangGraph (checkpointing) | No persistent memory (InMemoryConversationHistory deleted) |
| RAG pipeline | Haystack, LangChain | Basic retriever/classifier |
| Managed cloud | Prefect Cloud, Temporal Cloud | Self-hosted only |

---

## 4. Strengths

**S1: Contract Enforcement (5.0/5)** — Kernel JSON Schema validation, tool ACLs, resource quotas, max_visits entry guard, step_limit.

**S2: Architecture (4.9/5)** — Three-layer IPC boundary. Unix process lifecycle. 13 `@runtime_checkable` protocols. `#[deny(unsafe_code)]`.

**S3: Graph Expressiveness (4.5/5)** (NEW) — Gate/Fork/Agent node kinds, state merge strategies, Break instruction, step_limit. Matches LangGraph's graph expressiveness with kernel enforcement.

**S4: Type Safety (4.5/5)** — Frozen dataclasses, typed LLM boundary, typed routing with 5 field scopes.

**S5: Testing & Evaluation (4.5/5)** — MockKernelClient with full Gate/Fork/state fidelity. TestPipeline, EvaluationRunner.

**S6: Observability (4.0/5)** — 5 OTEL sites, 14 Prometheus metrics, CommBus events.

**S7: Interoperability (4.0/5)** — MCP + A2A client+server (590 lines A2A, up from 437).

---

## 5. Weaknesses

**W1: Community & Ecosystem (1.0/5)** — Zero adoption.

**W2: Multi-language Support (2.0/5)** — Rust + Python only.

**W3: Managed Experience (2.0/5)** — Self-hosted only.

**W4: Memory & Persistence (2.0/5)** (WORSENED) — `InMemoryConversationHistory` was deleted. No persistent memory at all. State is ephemeral within pipeline execution only.

**W5: RAG Pipeline Depth (2.5/5)** — Basic retriever/classifier.

**W6: Operational Complexity (2.5/5)** — Rust binary required.

---

## 6. Documentation Assessment

8 documents, 1,387 lines. Score: 3.5/5. (Unchanged — no new docs in this iteration.)

---

## 7. Recommendations

### Immediate (High Impact)

1. **Persistent state/memory** — The deletion of InMemoryConversationHistory without replacement makes memory the #1 gap. Need Redis/SQL-backed state persistence.

2. **TypeScript SDK** — Highest-leverage ecosystem expansion.

3. **Tutorials/cookbook** — Gate/Fork patterns need documentation.

### Medium-term

4. **Checkpoint/replay** — Persist pipeline state for crash recovery.
5. **Vector DB integration** — Production RAG.
6. **State visualization** — Graph primitives are now complex enough to warrant visual tooling.

---

## 8. Maturity Assessment

| Dimension | Score | Rationale |
|-----------|-------|-----------|
| Architecture | 4.9/5 | Rust kernel + Python infra, IPC boundary, 3-layer separation, protocol-first DI |
| Contract enforcement | 5.0/5 | Kernel JSON Schema + tool ACLs + quotas + max_visits + step_limit |
| Graph expressiveness | 4.5/5 | Gate/Fork/Agent nodes, state merge, Break, step_limit. Matches LangGraph |
| Type safety | 4.5/5 | Frozen dataclasses, typed LLM boundary, `#[deny(unsafe_code)]` |
| Testing & eval | 4.5/5 | MockKernelClient with full Gate/Fork fidelity, TestPipeline, EvaluationRunner |
| Observability | 4.0/5 | 5 OTEL sites, 14 Prometheus metrics |
| Interoperability | 4.0/5 | MCP + A2A client+server, bidirectional |
| Documentation | 3.5/5 | 8 docs, 1,387 lines. Missing tutorials for Gate/Fork patterns |
| RAG depth | 2.5/5 | Basic retriever + classifier |
| Operational complexity | 2.5/5 | Rust compilation required |
| Multi-language | 2.0/5 | Rust + Python only |
| Managed experience | 2.0/5 | Self-hosted only |
| Memory/persistence | 2.0/5 | No persistent memory (InMemoryConversationHistory deleted) |
| Community/ecosystem | 1.0/5 | Zero adoption |
| **Overall** | **3.7/5** | |

---

## 9. Trajectory Analysis

### Gap Closure (7 Iterations)

| Gap | Status |
|-----|--------|
| String-prompt LLM interface | **CLOSED** — message-based chat() |
| No output schema validation | **CLOSED** — kernel JSON Schema |
| No tool ACLs | **CLOSED** — kernel-enforced |
| No MCP support | **CLOSED** — client+server |
| No A2A support | **CLOSED** — client+server (590 lines) |
| Envelope complexity | **CLOSED** — frozen AgentContext |
| No testing framework | **CLOSED** — MockKernelClient + TestPipeline + EvaluationRunner |
| Dead protocols | **CLOSED** — 18 → 13 |
| No StreamingAgent separation | **CLOSED** — StreamingAgent(Agent) SRP |
| No OTEL spans | **CLOSED** — 5 sites |
| No documentation | **CLOSED** — 8 docs, 1,387 lines |
| No graph primitives | **CLOSED** — Gate/Fork/State/Break/step_limit |
| No state management | **CLOSED** — state_schema + MergeStrategy |
| Hooks-as-lists | **CLOSED** — normalized callable handling |
| Persistent memory | **OPEN** — InMemoryConversationHistory deleted, no replacement |

**14/15 gaps closed.** Sole remaining: persistent memory.

### Code Trajectory

| Metric | Direction | Detail |
|--------|-----------|--------|
| Rust orchestrator | 2,165 → 2,665 lines | +500 lines for Gate/Fork/State/Break |
| Rust orchestrator_types | 274 → 312 lines | NodeKind, MergeStrategy, StateField, step_limit |
| MockKernelClient | ~192 → 502 lines | Full-fidelity Gate/Fork/WaitFirst/state mock |
| PipelineWorker | ~539 → 787 lines | break_loop, parallel fan-out, consolidated |
| A2A server | 254 → 421 lines | Expanded server implementation |
| Routing evaluator | Added State scope | 5th field scope |
| Deleted | InMemoryConversationHistory | 44 lines dead code removed |
| Protocols | 14 → 13 | ConversationHistoryProtocol removed |

---

*Audit conducted across 7 iterations on branch `claude/audit-agentic-frameworks-B6fqd`, 2026-03-11 to 2026-03-12.*
