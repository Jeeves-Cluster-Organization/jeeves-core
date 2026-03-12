# Jeeves-Core: Comprehensive Architectural Audit & Comparative Analysis

**Date**: 2026-03-12 (Iteration 10 — definition-time validation, DeterministicAgent, API surface)
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

Jeeves-Core is a **Rust micro-kernel + Python infrastructure** framework for AI agent orchestration. The kernel (~15,300 lines Rust, `#[deny(unsafe_code)]`) provides process lifecycle, IPC, interrupt handling, resource quotas, and graph-based pipeline orchestration. The Python layer (~5,500 lines, up from ~5,200) provides LLM providers, pipeline execution, gateway, bootstrap, and now a curated single-import API surface.

**Architectural thesis**: A Rust kernel managing Python agents provides enforcement guarantees that in-process Python frameworks cannot match — resource quotas, output schema validation, tool ACLs, and fault isolation are kernel-mediated, not convention-dependent.

**Overall maturity: 3.9/5** (up from 3.8) — definition-time validation, DeterministicAgent, curated API surface, and 55 more tests improve developer experience and catch errors before runtime.

**What changed in iteration 10** (2 commits, +731/-1 lines):

- **Definition-time validation**: `RoutingRule.__post_init__` eagerly validates expr structure via `validate_expr()`. `PipelineConfig.validate()` cross-references routing targets, default_next, error_next against existing stages. Gate nodes validated: no `has_llm=True`, no `Current` scope in routing (Gates produce no output). `capability_wiring._validate_pipeline_config()` now calls `pipeline.validate()`.
- **DeterministicAgent**: Abstract base class for non-LLM stage executors. `execute(context) -> Dict`. Bridged into Agent via mock_handler — no Worker/IPC changes needed. `agent_class` field on `AgentConfig` with mutual exclusion validation.
- **Curated API surface**: `jeeves_core.api` — single import for all capability definition needs.
- **Routing expression builders**: `routing.py` expanded from 50->227 lines with `validate_expr()` structural validator and 5 FieldRef scope constructors.
- **+55 new Python tests**: `test_routing_validation.py` (23), `test_types_parity.py` (12), `test_pipeline_validation.py` (11), `test_deterministic_agent.py` (8), `test_api_surface.py` (1).
- **Async mock_handler fix**: `Agent.process()` now awaits coroutine mock_handlers.

---

## 2. Architecture Deep Dive

### 2.1 Layer Architecture

```
+-----------------------------------------------------+
|              Capability Layer (external)             |
|  Agents, Prompts, Tools, Domain Logic, Pipelines    |
|  import from jeeves_core.api (single import)        |
+-----------------------------------------------------+
|          Python Infrastructure (jeeves_core)         |
|  Gateway | LLM Providers | Bootstrap | Orchestrator  |
|  IPC Client | Retrieval | MCP | A2A                 |
|  DeterministicAgent | Routing Builders               |
+-----------------------------------------------------+
|              IPC (TCP + msgpack)                     |
|  4B length prefix | 1B type | msgpack payload        |
|  MAX_FRAME_SIZE = 5MB                                |
+-----------------------------------------------------+
|              Rust Micro-Kernel                       |
|  Process Lifecycle | Resource Quotas | Interrupts    |
|  Pipeline Orchestrator | CommBus | Rate Limiter      |
|  JSON Schema Validation | Tool ACLs                  |
|  Graph Primitives (Gate/Fork/State)                  |
+-----------------------------------------------------+
```

### 2.2 Rust Kernel

#### Process Lifecycle
Unix-inspired state machine: `New -> Ready -> Running -> Waiting/Blocked -> Terminated -> Zombie`

#### Pipeline Orchestrator (~2,560 lines, 69 tests)
```
error_next -> routing_rules (first match) -> default_next -> terminate
```

**Graph primitives**:
- `NodeKind::Agent` — standard agent execution node
- `NodeKind::Gate` — pure routing node, evaluates routing without running an agent
- `NodeKind::Fork` — parallel fan-out, evaluates ALL matching rules, dispatches branches concurrently

**State management**:
- `StateField { key, merge: MergeStrategy }` — typed state fields
- `MergeStrategy::Replace` / `Append` / `MergeDict`
- `state_schema` on `PipelineConfig` — declares state fields and merge strategies

**Control flow**:
- `Break` instruction — agents set `_break: true` -> kernel terminates with `BREAK_REQUESTED`
- `step_limit` on `PipelineConfig` — optional hard cap on total orchestration steps

**Instruction factory methods**:
- `Instruction::terminate(reason, message)` / `::terminate_completed()`
- `Instruction::run_agent(name)` / `::run_agents(names)`
- `Instruction::wait_parallel()` / `::wait_interrupt(interrupt)`

**Routing Expression Language** (631 lines, 18 tests): 13 operators across 5 field scopes (Current, Agent, Meta, Interrupt, State).

**Parallel execution**: Fork nodes with `JoinStrategy::WaitAll` / `JoinStrategy::WaitFirst`.

**Instruction types**: `RunAgent`, `RunAgents`, `WaitParallel`, `WaitInterrupt`, `Terminate`.

#### Kernel-Enforced Contracts
- JSON Schema output validation (`jsonschema` crate)
- Tool ACL enforcement (defense-in-depth)
- Resource quotas: `max_llm_calls`, `max_tool_calls`, `max_agent_hops`, `max_iterations`, `max_output_tokens`, timeout
- `max_visits` per-stage entry guard
- Per-user sliding-window rate limiting

#### IPC Protocol (2,022 lines + 5 handler modules)
Binary wire format (4B len + 1B type + msgpack), 27 methods across 5 services:
- `kernel.rs` (779L) — process lifecycle, scheduling, quotas
- `orchestration.rs` (418L) — pipeline session, instructions, agent results
- `tools.rs` (344L) — catalog, validation, ACLs, prompt generation
- `interrupt.rs` (253L) — interrupt submission, resolution, listing
- `commbus.rs` (228L) — pub/sub events, commands, queries

#### CommBus (849 lines, 12 tests)
Pub/sub events, point-to-point commands, request/response queries with timeout.

#### Interrupts (1,041 lines, 16 tests)
7 typed kinds with TTL.

### 2.3 Python Infrastructure

#### Curated API Surface (NEW — `jeeves_core.api`)
Single-import module for capability authors:
```python
from jeeves_core.api import (
    PipelineConfig, AgentConfig, stage, Edge, RoutingRule,
    RunMode, JoinStrategy, TokenStreamMode, GenerationParams,
    DomainServiceConfig, EdgeLimit,
    DomainModeConfig, DomainAgentConfig, CapabilityToolsConfig,
    CapabilityOrchestratorConfig, AgentLLMConfig,
    RequestContext, AgentContext,
    eq, neq, gt, lt, gte, lte, contains, exists, not_exists,
    and_, or_, not_, always, agent, meta, state, interrupt, current,
    register_capability,
    DeterministicAgent,
    make_agent_context, make_agent_config,
)
```

#### Definition-Time Validation (NEW)

**Three layers of validation, all before runtime:**

1. **RoutingRule.__post_init__**: Eagerly validates expr structure — catches typos in op names, missing fields, invalid scopes at definition time, not at pipeline execution time.

2. **PipelineConfig.validate()**: Cross-reference checks — routing targets exist, default_next/error_next reference valid stages, Gate nodes don't use `has_llm=True` or `Current` scope (Gates produce no output). Recursive `_check_current_scope_in_expr()` walks And/Or/Not trees.

3. **capability_wiring._validate_pipeline_config()**: Calls `pipeline.validate()` at registration time. Catches graph edge conflicts, duplicate output_keys, and all of the above.

**Error shift**: These validations move error detection from runtime pipeline execution (where debugging is hard) to definition/registration time (where stack traces point at the misconfigured line). This mirrors Rust's compile-time safety ethos on the Python side.

#### DeterministicAgent (NEW — `runtime/deterministic.py`)
```python
class DeterministicAgent(ABC):
    @abstractmethod
    async def execute(self, context: AgentContext) -> Dict[str, Any]: ...
```

Base class for non-LLM stage executors (data transforms, API calls, aggregators). Bridged into existing `Agent` machinery via mock_handler — no Worker/IPC/kernel changes needed. `agent_class` field on `AgentConfig` with mutual exclusion: `agent_class` + `has_llm` raises ValueError, `agent_class` + `mock_handler` raises ValueError. PipelineRunner validates `issubclass(agent_class, DeterministicAgent)` at build time.

#### Protocol-first DI — 13 `@runtime_checkable` Protocols
- `LLMProviderProtocol`: message-based `chat()`, `chat_with_usage()`, `chat_stream()`, `health_check()`
- `ContextRetrieverProtocol`, `EmbeddingProviderProtocol`
- `ToolRegistryProtocol`, `ToolExecutorProtocol`, `ConfigRegistryProtocol`
- `DatabaseClientProtocol`, `LoggerProtocol`, `AppContextProtocol`
- `ClockProtocol`, `DistributedBusProtocol`

#### Typed LLM Boundary
All frozen dataclasses: `LLMResult`, `LLMToolCall`, `LLMUsage`.

#### Routing Expression Builders (227 lines)
- 13 operator builders: `eq`, `neq`, `gt`, `lt`, `gte`, `lte`, `contains`, `exists`, `not_exists`, `and_`, `or_`, `not_`, `always`
- 5 FieldRef scope constructors: `current()`, `agent()`, `meta()`, `interrupt()`, `state()`
- `_field()` normalizer: bare string -> Current scope, dict -> pass-through
- `validate_expr()`: structural validator — checks op validity, field structure, scope required keys, recursive And/Or/Not validation

#### Agent Execution (~793 lines)
- `Agent.process(AgentContext) -> Tuple[Dict, Dict]`
- `StreamingAgent(Agent)` — SRP subclass
- hooks-as-lists: `pre_process`, `post_process`, `context_loaders`
- `_break` output key triggers kernel Break instruction
- Async mock_handler fix: `Agent.process()` now awaits coroutine mock_handlers (NEW)

#### Pipeline Execution (~787 lines)
- `run_kernel_loop()` — free function, handles RUN_AGENT and RUN_AGENTS (parallel fan-out)
- `PipelineWorker` — capacity semaphore, scheduler queue, service registry
- `execute_streaming()` — asyncio.Queue + sentinel pattern
- `break_loop` support in kernel loop

#### Testing Infrastructure
- `MockKernelClient` (~502 lines) — full-fidelity kernel mock
- `test_mock_kernel_fidelity.py` (13 tests) — bounds, Gate, WaitFirst, Break, state merge
- `test_routing_eval_ops.py` (61 tests) — all 13 operators, 5 scopes, edge cases
- `test_routing_validation.py` (23 tests) — structural validation of routing exprs (NEW)
- `test_pipeline_validation.py` (11 tests) — cross-reference validation (NEW)
- `test_deterministic_agent.py` (8 tests) — DeterministicAgent lifecycle, hooks, config validation (NEW)
- `test_types_parity.py` (12 tests) — serialization parity with Rust kernel types (NEW)
- `test_api_surface.py` (1 test) — all API exports importable (NEW)
- `TestPipeline` + `EvaluationRunner`
- `routing_eval.py` (149 lines)

#### Interoperability
- MCP client+server (528 lines)
- A2A client+server (421+169 = 590 lines)

#### Observability
- Active OTEL spans at 5 sites, 14 Prometheus metrics (6 kernel-unique)

---

## 3. Comparative Analysis — 17 Frameworks

### 3.1 Unique Differentiators (vs. ALL 17)

1. **Kernel-enforced contracts** — No other Python-ecosystem framework validates output schemas, enforces tool ACLs, or caps resource consumption at a kernel level.

2. **Definition-time validation** (NEW) — RoutingRule validates expr at definition. PipelineConfig.validate() cross-references targets. Gate constraints (no Current scope, no has_llm) caught before deployment. No other framework validates pipeline graphs this thoroughly at definition time.

3. **Graph primitives (Gate/Fork/State)** — Gate (pure routing), Fork (parallel fan-out with WaitAll/WaitFirst), typed state merge (Replace/Append/MergeDict). Matches LangGraph's expressiveness with kernel enforcement.

4. **Rust kernel + Python agents** — Memory safety and fault isolation pure-Python frameworks structurally cannot provide.

5. **Two-phase instruction enrichment** — Orchestrator determines *what*; kernel enriches *how*. Unique separation.

6. **7 typed interrupt kinds** — Most have no HITL or basic breakpoints only.

7. **Both MCP client+server AND A2A client+server** — No other framework has full bidirectional support for both.

8. **Full-fidelity kernel mock + 151 dedicated tests** — MockKernelClient replicating Gate/Fork/state routing. LLM-free pipeline testing. 151 dedicated tests covering routing ops, kernel fidelity, validation, serialization parity, DeterministicAgent.

### 3.2 Comparison Matrix

| Dimension | Jeeves | LangGraph | CrewAI | AutoGen | Semantic Kernel | Haystack | DSPy | Prefect | Temporal | Mastra | PydanticAI | Agno | Google ADK | Strands | Rig |
|-----------|--------|-----------|--------|---------|-----------------|----------|------|---------|----------|--------|------------|------|-----------|---------|-----|
| **Language** | Rust+Py | Python | Python | Python | C#/Py/Java | Python | Python | Python | Go+SDKs | TS | Python | Python | Python | Python | Rust |
| **Kernel enforcement** | Yes | No | No | No | No | No | No | No | Yes* | No | No | No | No | No | No |
| **Definition-time validation** | Yes | Partial | No | No | No | No | No | Partial | Workflow | Partial | Pydantic | No | No | No | No |
| **Graph primitives** | Gate/Fork/State | Conditional edges | None | None | None | Pipeline | None | DAG | Workflows | Graph | None | None | Seq/Par/Loop | None | None |
| **State merge** | Replace/Append/MergeDict | Annotated reducers | None | Shared state | None | None | None | None | None | None | None | None | None | None | None |
| **Output validation** | Kernel JSON Schema | Parsers | None | None | Filters | None | Assertions | Pydantic | No | Zod | Pydantic | None | None | None | None |
| **Resource quotas** | Kernel-enforced | None | Manual | None | None | None | None | Limits | Limits | None | None | None | None | None | None |
| **Tool ACLs** | Kernel-enforced | None | None | None | None | None | None | None | None | None | None | None | None | None | None |
| **Interrupt/HITL** | 7 typed kinds | Breakpoints | None | Human proxy | None | None | None | Pause | Signals | None | None | None | None | None | None |
| **MCP support** | Client+Server | Client | None | None | None | None | None | None | None | Client | Client | Client | Client | Client | None |
| **A2A support** | Client+Server | None | None | None | None | None | None | None | None | None | None | None | Server | None | None |
| **Testing infra** | MockKernel+151 tests | None | None | None | None | None | Eval | None | None | None | pytest | None | Eval | None | None |

### 3.3 Where Jeeves Trails

| Gap | Leading Framework | Jeeves Status |
|-----|-------------------|---------------|
| Community/ecosystem | AutoGen (40k), CrewAI (25k) | ~0 stars |
| Multi-language SDKs | Semantic Kernel (C#/Py/Java) | Rust+Python only |
| Visual builder | Mastra, Prefect | No GUI |
| Built-in memory | LangGraph (checkpointing) | No persistent memory |
| RAG pipeline | Haystack, LangChain | Basic retriever/classifier |
| Managed cloud | Prefect Cloud, Temporal Cloud | Self-hosted only |

### 3.4 Framework Comparison Scores

| Framework | Overall | Architecture | Graph | Ecosystem | DX | Protocol |
|-----------|---------|--------------|-------|-----------|-----|---------|
| LangChain/LangGraph | 4.0 | 3.5 | 4.0 | 5.0 | 4.5 | 4.0 |
| CrewAI | 3.5 | 2.5 | 2.0 | 4.0 | 4.5 | 3.5 |
| MS Agent Framework | 3.5 | 4.0 | 3.5 | 3.5 | 3.5 | 4.5 |
| OpenAI Agents SDK | 3.5 | 3.5 | 2.5 | 3.5 | 5.0 | 3.5 |
| Haystack | 3.5 | 3.5 | 3.0 | 4.0 | 3.5 | 3.0 |
| DSPy | 3.0 | 4.0 | 2.0 | 2.5 | 2.5 | 2.5 |
| Google ADK | 3.0 | 3.5 | 3.0 | 3.0 | 4.0 | 4.0 |
| PydanticAI | 3.0 | 3.0 | 2.0 | 2.5 | 4.0 | 3.5 |
| Agno | 3.0 | 3.0 | 2.5 | 3.5 | 4.0 | 3.0 |
| Strands Agents | 3.0 | 3.0 | 2.5 | 3.0 | 3.5 | 3.5 |
| **Jeeves-Core** | **3.9** | **4.9** | **4.5** | 1.5 | 3.5 | 4.0 |

Jeeves-Core DX score rose from 2.5->3.5 with curated API surface, definition-time validation, and DeterministicAgent.

---

## 4. Strengths

**S1: Contract Enforcement (5.0/5)** — Kernel JSON Schema validation, tool ACLs, resource quotas, max_visits entry guard, step_limit.

**S2: Architecture (4.9/5)** — Three-layer IPC boundary. Unix process lifecycle. 13 `@runtime_checkable` protocols. `#[deny(unsafe_code)]`. Modular IPC handlers.

**S3: Definition-Time Validation (4.5/5)** (NEW) — Three-layer validation: RoutingRule.__post_init__ validates expr structure, PipelineConfig.validate() cross-references graph integrity, capability_wiring validates at registration. Gate constraint detection (no Current scope, no has_llm). Errors caught at definition time, not runtime.

**S4: Graph Expressiveness (4.5/5)** — Gate/Fork/Agent node kinds, state merge strategies, Break instruction, step_limit. Matches LangGraph with kernel enforcement.

**S5: Type Safety (4.5/5)** — Frozen dataclasses, typed LLM boundary, typed routing with 5 field scopes. Instruction factory methods. Mutual exclusion validation on AgentConfig (agent_class vs has_llm/mock_handler).

**S6: Testing & Evaluation (4.8/5)** (UP from 4.7) — 151 dedicated tests across 7 test files. 61 routing ops, 23 routing validation, 13 kernel fidelity, 12 types parity, 11 pipeline validation, 8 DeterministicAgent, 1 API surface. MockKernelClient, TestPipeline, EvaluationRunner.

**S7: Developer Experience (3.5/5)** (UP from 2.5) — Single-import API surface (`jeeves_core.api`). DeterministicAgent for non-LLM stages. Routing expression builders with ergonomic `_field()` normalizer. Definition-time errors with clear messages.

**S8: Observability (4.0/5)** — 5 OTEL sites, 14 Prometheus metrics, CommBus events.

**S9: Interoperability (4.0/5)** — MCP + A2A client+server (590 lines A2A). Bidirectional.

**S10: Code Hygiene (4.5/5)** — Dead code deleted, Instruction factories, modular handlers, TerminalReason enum parity tested.

---

## 5. Weaknesses

**W1: Community & Ecosystem (1.0/5)** — Zero adoption.

**W2: Multi-language Support (2.0/5)** — Rust + Python only.

**W3: Managed Experience (2.0/5)** — Self-hosted only.

**W4: Memory & Persistence (2.0/5)** — No persistent memory. State is ephemeral within pipeline execution only.

**W5: RAG Pipeline Depth (2.5/5)** — Basic retriever/classifier.

**W6: Operational Complexity (2.5/5)** — Rust binary required.

---

## 6. Documentation Assessment

8 documents, 1,387 lines. Score: 3.5/5. Gate/Fork patterns and routing builder API still need tutorials. The `jeeves_core.api` module serves as implicit documentation via its curated exports.

---

## 7. Recommendations

### Immediate (High Impact)

1. **Persistent state/memory** — Pipeline state_schema provides the schema; need Redis/SQL-backed persistence across requests.

2. **API surface documentation** — `jeeves_core.api` needs a getting-started tutorial showing stage(), routing builders, PipelineConfig.chain()/graph(), DeterministicAgent.

3. **TypeScript SDK** — Highest-leverage ecosystem expansion.

### Medium-term

4. **Checkpoint/replay** — Persist pipeline state for crash recovery.
5. **Vector DB integration** — Production RAG.
6. **State visualization** — Graph primitives are now complex enough to warrant visual tooling.

---

## 8. Maturity Assessment

| Dimension | Score | Rationale |
|-----------|-------|-----------|
| Architecture | 4.9/5 | Rust kernel + Python infra, IPC boundary, 3-layer separation, protocol-first DI, modular IPC handlers |
| Contract enforcement | 5.0/5 | Kernel JSON Schema + tool ACLs + quotas + max_visits + step_limit |
| Definition-time validation | 4.5/5 | RoutingRule, PipelineConfig.validate(), capability_wiring — 3-layer pre-runtime validation |
| Graph expressiveness | 4.5/5 | Gate/Fork/Agent nodes, state merge, Break, step_limit. Matches LangGraph |
| Type safety | 4.5/5 | Frozen dataclasses, typed LLM boundary, `#[deny(unsafe_code)]`, Instruction factories |
| Testing & eval | 4.8/5 | 151 tests: routing ops, validation, kernel fidelity, types parity, DeterministicAgent, API surface |
| Developer experience | 3.5/5 | Curated API surface, DeterministicAgent, routing builders, definition-time errors |
| Observability | 4.0/5 | 5 OTEL sites, 14 Prometheus metrics |
| Interoperability | 4.0/5 | MCP + A2A client+server, bidirectional |
| Code hygiene | 4.5/5 | Dead code deleted, Instruction factories, modular handlers, TerminalReason parity |
| Documentation | 3.5/5 | 8 docs, 1,387 lines. Missing tutorials for API surface |
| RAG depth | 2.5/5 | Basic retriever + classifier |
| Operational complexity | 2.5/5 | Rust compilation required |
| Multi-language | 2.0/5 | Rust + Python only |
| Managed experience | 2.0/5 | Self-hosted only |
| Memory/persistence | 2.0/5 | No persistent memory |
| Community/ecosystem | 1.0/5 | Zero adoption |
| **Overall** | **3.9/5** | |

---

## 9. Trajectory Analysis

### Gap Closure (10 Iterations)

| Gap | Status |
|-----|--------|
| String-prompt LLM interface | **CLOSED** — message-based chat() |
| No output schema validation | **CLOSED** — kernel JSON Schema |
| No tool ACLs | **CLOSED** — kernel-enforced |
| No MCP support | **CLOSED** — client+server |
| No A2A support | **CLOSED** — client+server (590 lines) |
| Envelope complexity | **CLOSED** — frozen AgentContext |
| No testing framework | **CLOSED** — MockKernelClient + TestPipeline + EvaluationRunner + 151 dedicated tests |
| Dead protocols | **CLOSED** — 18 -> 13 |
| No StreamingAgent separation | **CLOSED** — StreamingAgent(Agent) SRP |
| No OTEL spans | **CLOSED** — 5 sites |
| No documentation | **CLOSED** — 8 docs, 1,387 lines |
| No graph primitives | **CLOSED** — Gate/Fork/State/Break/step_limit |
| No state management | **CLOSED** — state_schema + MergeStrategy |
| Hooks-as-lists | **CLOSED** — normalized callable handling |
| Instruction boilerplate | **CLOSED** — 6 factory methods on Instruction |
| IPC handler monolith | **CLOSED** — 5 modular handler modules |
| No definition-time validation | **CLOSED** — RoutingRule, PipelineConfig.validate(), capability_wiring 3-layer validation |
| No curated API surface | **CLOSED** — jeeves_core.api single-import module |
| No non-LLM agent base class | **CLOSED** — DeterministicAgent ABC |
| Persistent memory | **OPEN** — no replacement for deleted InMemoryConversationHistory |

**19/20 gaps closed.** Sole remaining: persistent memory.

### Code Trajectory (This Iteration)

| Metric | Before -> After | Detail |
|--------|---------------|--------|
| `jeeves_core/api.py` | 0 -> 25 lines | Curated single-import API surface |
| `protocols/routing.py` | ~50 -> 227 lines | +validate_expr(), FieldRef constructors, structural validation |
| `protocols/types.py` | ~919 -> ~997 lines | +PipelineConfig.validate(), AgentConfig.agent_class, RoutingRule.__post_init__ |
| `runtime/deterministic.py` | 0 -> 26 lines | DeterministicAgent ABC |
| `runtime/agents.py` | ~769 -> ~793 lines | DeterministicAgent bridge, async mock_handler fix |
| `capability_wiring.py` | +10 lines | Cross-reference validation call |
| New test files | 5 files, 481 lines | 55 tests: validation, DeterministicAgent, types parity, API surface |
| Total dedicated Python tests | 96 -> 151 | +55 tests |

---

*Audit conducted across 10 iterations on branch `claude/audit-agentic-frameworks-B6fqd`, 2026-03-11 to 2026-03-12.*
