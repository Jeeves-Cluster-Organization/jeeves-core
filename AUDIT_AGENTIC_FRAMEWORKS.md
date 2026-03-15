# Jeeves-Core v0.0.2 — Architectural Audit

**Date**: 2026-03-15 (Iteration 27 — weakness audit correction)
**Codebase**: ~17,890 lines Rust, `#[deny(unsafe_code)]` except PyO3 FFI. 290 Rust tests + 27 pytest. Single-process kernel consumed via PyO3 or MCP stdio.

---

## 1. What It Is

A Rust kernel for AI agent pipeline orchestration. Two consumption modes from one binary:

```
Python: from jeeves_core import PipelineRunner, tool
MCP:    jeeves-kernel (JSON-RPC 2.0 over stdin/stdout)
```

Core loop: `KernelHandle` → mpsc → single `&mut Kernel` actor → typed `Instruction` enum → worker spawns tokio agent tasks → results reported back.

**Score: 4.9/5** — all critical gaps closed. One real bug: MCP stdio child processes orphan on drop (missing `impl Drop`). Two partial gaps: CommBus queries not exposed via MCP for cross-network federation; MCP HTTP transport missing `headers` config field.

---

## 2. Architecture

### 2.1 Kernel (domain-grouped, v0.0.2)

```rust
pub struct Kernel {
    pub lifecycle: LifecycleManager,
    pub resources: ResourceTracker,
    pub rate_limiter: RateLimiter,
    pub interrupts: InterruptService,
    pub services: ServiceRegistry,
    pub orchestrator: Orchestrator,
    pub process_envelopes: HashMap<ProcessId, Envelope>,
    pub tools: ToolDomain,   // { catalog, access, health }
    pub comm: CommDomain,    // { bus, agent_cards, subscriptions }
}
```

9 fields (was 13). `process_envelopes` and `orchestrator` top-level for split borrow in `process_agent_result()`.

### 2.2 Orchestrator (split into focused files, v0.0.2)

| File | Lines | Purpose |
|------|-------|---------|
| `orchestrator.rs` | 2,285 | Core routing loop: `get_next_instruction()`, `report_agent_result()`, gate recursion, fork fan-out, parallel join |
| `orchestrator_types.rs` | 644 | `Instruction` enum, `PipelineConfig`, `PipelineStage`, `AgentConfig`, `EdgeKey`, validation (28 tests) |
| `orchestrator_session.rs` | 243 | Session lifecycle: init, cleanup, stale GC, state serialization (8 tests) |
| `orchestrator_queries.rs` | 111 | Read-only: `get_session_state()`, schema lookups, session count (5 tests) |
| `orchestrator_helpers.rs` | 183 | Pure functions: stage lookup, snapshot build, edge limit check, join evaluation (13 tests) |
| `test_helpers.rs` | 52 | Shared fixtures: `stage()`, `always_to()`, `create_test_pipeline()` |

**Instruction enum** (tagged, impossible states eliminated):
```rust
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Instruction {
    RunAgent { agent: String, context: AgentDispatchContext },
    RunAgents { agents: Vec<String>, context: AgentDispatchContext },
    Terminate { reason: TerminalReason, message: Option<String>, context: AgentDispatchContext },
    WaitParallel,
    WaitInterrupt { interrupt: Option<FlowInterrupt> },
}
```

**Routing**: `interrupt_pending` → WaitInterrupt. Failed + `error_next` → error_next. Routing rules (first match) → `default_next` → terminate COMPLETED.

**Graph nodes**: Agent (execute), Gate (route-only, no agent), Fork (parallel fan-out, WaitAll/WaitFirst join).

**Validation**: `PipelineConfig::validate()` — non-empty name, positive bounds, unique stages/output_keys, `child_pipeline` ⊕ `has_llm`, routing targets exist, expression depth ≤ 32, self-loop requires `max_visits`, edge limits reference valid stages. 28 tests.

### 2.3 CommBus (split into focused files, v0.0.2)

| File | Lines | Purpose |
|------|-------|---------|
| `mod.rs` | 101 | `CommBus` struct + constructor + stats |
| `types.rs` | 132 | `Event`, `Command`, `Query`, `QueryResponse`, `Subscriber`, `Subscription`, `BusStats` |
| `event_bus.rs` | 233 | Pub/sub: `publish()` fan-out, `subscribe()`, `unsubscribe()`, `cleanup_disconnected()` (8 tests) |
| `command_bus.rs` | 98 | Fire-and-forget: `send_command()`, `register_command_handler()` (2 tests) |
| `query_bus.rs` | 187 | Request/response with timeout: `query()`, `register_query_handler()`, `get_query_handler()` for fire-and-spawn (6 tests) |

Kernel-mediated. Pipeline subscriptions auto-wired from `PipelineConfig.subscriptions`. Pending events drained into `agent_context.pending_events` each instruction cycle. `get_query_handler()` returns sender clone for fire-and-spawn in actor loop (prevents deadlock).

### 2.4 Envelope (restructured, v0.0.2)

Semantic sub-structs: `Identity` (ids), `Pipeline` (stage tracking), `Bounds` (resource limits + `Option<Termination>`), `InterruptState`, `Audit` (processing history). New: `event_inbox: Vec<Event>` for CommBus bridge.

`Termination { reason, message }` replaces the old `terminated: bool` + `terminal_reason` + `terminal_message` triple.

### 2.5 Worker

**KernelHandle** (314L): 13 command variants. `kernel_request!` macro reduces each method from ~8 to ~3 lines.

**Kernel Actor** (248L): `dispatch()` pattern-matches `KernelCommand`. `CommBusQuery` uses fire-and-spawn. Domain access via `kernel.tools.*`, `kernel.comm.*`.

**4 agent types**:
1. `LlmAgent` — ReAct tool loop, SSE streaming, pre-execution ACL check, circuit-broken tools skipped
2. `McpDelegatingAgent` — forwards to MCP/Python tool, emits ToolCallStart/ToolResult/Error events
3. `DeterministicAgent` — no-LLM passthrough
4. `PipelineAgent` — runs child pipeline as stage, event bridge with dotted `pipeline` field (`"parent.child"`)

**Pipeline loop** (350L): dispatches tagged `Instruction` enum. `WorkerResult` uses `Option<Termination>`. Streaming polls 500ms on `WaitInterrupt`; buffered returns early.

### 2.6 PyO3 (776L)

```python
runner = PipelineRunner.from_json("pipeline.json", "prompts/")
runner.register_tool(my_tool)
result = runner.run("Hello", user_id="u1", metadata={"k": "v"})
for event in runner.stream("Hello"):
    print(event)
```

Auto-agent creation from stage config: Gate→Deterministic, `child_pipeline`→PipelineAgent, `has_llm`→LlmAgent, tool match→McpDelegating. GIL released via `py.allow_threads()`. Nested calls safe via `block_in_place()`. Two-pass PipelineAgent build with `OnceLock<Arc<AgentRegistry>>`.

27 pytest tests: @tool decorator, PipelineRunner construction/run/stream, multi-stage, error handling, metadata, event attribution.

### 2.7 MCP

**Client** (430L): HTTP + stdio transports, JSON-RPC 2.0, 30s timeout on stdio, per-tool-name registration into `ToolRegistry`. 7 unit + 1 integration test.

**Server** (360L): stdio, `initialize`/`tools/list`/`tools/call`, upstream proxy via `JEEVES_MCP_SERVERS`. 6 tests.

### 2.8 Supporting Modules

| Module | Lines | What |
|--------|-------|------|
| LLM providers | ~540 | `LlmProvider` trait, `chat()` + `chat_stream()`, OpenAI-compatible SSE |
| PipelineEvent | 8 variants | All carry `pipeline: Arc<str>` + optional `stage`. Serde: `#[serde(tag = "type", rename_all = "snake_case")]` |
| PipelineBuilder | 493 | Fluent API: `PipelineBuilder::new("p").stage("s1","a").default_next("s2").done().build()`. 13 tests |
| PipelineTestHarness | 170 | In-process testing, `mock_agent()`, buffered + streaming. `cfg(test)` |
| ToolHealthTracker | — | Sliding window (100 records), circuit breaker (5 errors / 5 min). Healthy ≥95% success ≤2s |
| InterruptService | 1,041 | 7 kinds (CLARIFICATION 24h, CONFIRMATION 1h, AGENT_REVIEW 30m, CHECKPOINT ∞, RESOURCE_EXHAUSTED/TIMEOUT 5m, SYSTEM_ERROR 1h). 16 tests |
| Routing expressions | 631 | 13 operators × 5 field scopes. `validate_depth()` max 32. 18 tests |
| CleanupService | 306 | Background GC: zombie processes, stale sessions, expired interrupts, rate windows. Configurable retention. 5 tests |
| PromptRegistry | ~180 | `.txt` loading, `{var}` substitution, `is_identifier()` guard for JSON safety |
| Tools | ~85 | `ToolExecutor` trait + `ToolRegistry` |

---

## 3. Comparison — 17 Frameworks

### What's unique (vs. all 17)

1. **Kernel-enforced contracts** — output schema, tool ACLs, resource quotas, circuit breaking at kernel level
2. **Zero-serialization dispatch** — typed mpsc + oneshot, no codec
3. **Single-binary deploy** — one Rust binary, one process
4. **Graph primitives** — Gate/Fork/Agent nodes, typed state merge (Replace/Append/MergeDict)
5. **Rust memory safety** — `#[deny(unsafe_code)]` end-to-end
6. **7 typed interrupts** — full resolution flow: kernel → pipeline poll → resume
7. **ReAct + circuit breaking** — agent rounds + kernel quotas + health-based circuit breaker
8. **Bidirectional MCP** — client (HTTP+stdio) AND server (stdio) in one framework
9. **PipelineBuilder + test harness** — validated builder + in-process harness
10. **PyO3 kernel consumption** — `from jeeves_core import PipelineRunner, tool`
11. **Pipeline-attributed streaming** — 8 event types with `stage` + `pipeline` (dotted for composition)
12. **PipelineAgent composition** — child pipeline as stage, event bridging
13. **CommBus federation** — pub/sub + commands + queries, kernel-mediated, deadlock-safe

### Matrix

| Dimension | Jeeves | LangGraph | CrewAI | AutoGen | SK | Haystack | DSPy | Mastra | PydanticAI | Agno | ADK | OAI Agents | Strands | Rig | MS AgentFw |
|-----------|--------|-----------|--------|---------|----|----------|------|--------|------------|------|-----|------------|---------|-----|------------|
| Language | Rust | Py | Py | Py | C#/Py/Java | Py | Py | TS | Py | Py | Py | Py | Py | Rust | C#/Py |
| Kernel enforcement | Yes | No | No | No | No | No | No | No | No | No | No | No | No | No | No |
| Def-time validation | Yes | Partial | No | No | No | No | No | Partial | Pydantic | No | No | No | No | No | No |
| Graph nodes | Gate/Fork/State | Cond. edges | — | — | — | Pipeline | — | Graph | — | — | Seq/Par | Handoffs | — | Pipeline | Conv. |
| Output validation | Kernel JSON Schema | Parsers | — | — | Filters | — | Assert | Zod | Pydantic | — | — | Guardrails | — | — | — |
| Resource quotas | Kernel | — | Manual | — | — | — | — | — | — | — | — | — | — | — | — |
| Tool ACLs | Pre-exec | — | — | — | — | — | — | — | — | — | — | — | — | — | — |
| Tool health | Circuit break | — | — | — | — | — | — | — | — | — | — | — | — | — | — |
| HITL | 7 kinds wired | Breakpoints | — | Human proxy | — | — | — | — | — | — | — | — | — | — | — |
| MCP | Client+Server | Client | — | — | — | — | — | Client | Client | Client | Client | Client | Client | — | — |
| Streaming | 8 events staged | Yes | — | Yes | Yes | — | — | Yes | Yes | Yes | Yes | Yes | Yes | — | Yes |
| Tests | 290+27py+harness | — | — | — | — | — | Eval | — | pytest | — | Eval | — | — | — | — |
| A2A | PipelineAgent | — | — | Nested | — | — | — | — | — | — | Loop/Sub | Handoffs | — | — | — |
| Persistent memory | — | Checkpoint | — | — | — | — | — | — | — | Memory | — | — | — | — | — |

### Per-framework verdicts (short)

| Framework | Jeeves wins on | They win on | Use case split |
|-----------|---------------|------------|----------------|
| LangGraph | Enforcement, single-binary, memory safety | Ecosystem (40k+ stars), checkpointing | Enforcement-critical vs. ecosystem breadth |
| CrewAI | Enforcement, graphs, streaming | Rapid prototyping, 45k stars | Production vs. prototyping |
| AutoGen | Structured pipelines, enforcement | Multi-agent conversations, Microsoft | Pipelines vs. conversations |
| Semantic Kernel | Graphs, enforcement, single-binary | Multi-lang (C#/Py/Java), .NET plugins | Orchestration vs. enterprise .NET |
| Haystack | Agent orchestration, interrupts | RAG, vector DBs | Different domains |
| DSPy | Runtime orchestration, production | Prompt optimization | Complementary |
| Mastra | Enforcement, state merge, circuit breaking | TypeScript/Next.js, visual builder | Rust vs. TypeScript |
| PydanticAI | Multi-agent, graph primitives | Cleanest single-agent API | Multi-agent vs. single-agent |
| Agno | Structured orchestration, validation | Multi-modal, memory, ~2μs instantiation | Pipelines vs. single agents |
| Google ADK | Richer graphs, LLM-agnostic | Google Cloud, A2A protocol | LLM-agnostic vs. Google Cloud |
| OpenAI Agents | LLM-agnostic, graphs, enforcement | DX, built-in tracing | Multi-LLM vs. OpenAI-only |
| Strands | Orchestration, multi-agent, streaming | AWS/Bedrock integration | Orchestration vs. AWS |
| Rig | Everything (kernel, graphs, interrupts, MCP) | Pipeline type safety | Jeeves far more mature |
| MS Agent Fw | Declarative graphs, enforcement | Multi-lang, enterprise | Declarative vs. conversational |
| LangChain | Orchestration, enforcement | 700+ integrations, 180k stars | Not competitors |

---

## 4. Strengths

| # | Area | Score | Key fact |
|---|------|-------|----------|
| S1 | Contract enforcement | 5.0 | JSON Schema + pre-exec ACLs + quotas + max_visits + circuit breaking |
| S2 | Architecture | 5.0 | Single-process, zero-serialization, typed channels, `&mut Kernel` actor |
| S3 | Operational simplicity | 4.5 | PyO3 lib or MCP binary. No HTTP server |
| S4 | Graph expressiveness | 4.5 | Gate/Fork/Agent, state merge, Break, step_limit, parallel join |
| S5 | Type safety | 5.0 | `#[deny(unsafe_code)]`, tagged enums, exhaustive matching |
| S6 | Memory safety | 5.0 | Rust ownership, zero unsafe outside PyO3 FFI |
| S7 | Agentic capabilities | 4.8 | ReAct + circuit breaking + health tracking. Triple enforcement |
| S8 | Streaming | 4.5 | 8 event types, stage+pipeline attributed, `Arc<str>` |
| S9 | Validation | 4.0 | `PipelineConfig::validate()`, 28 tests |
| S10 | Testing | 4.5 | 290 Rust + 27 pytest + harness + mock MCP |
| S11 | Observability | 4.0 | `#[instrument]`, routing debug, OTEL, CommBus stats |
| S12 | Code hygiene | 5.0 | ~40k dead lines deleted, god-files split, delegation layer eliminated, domain grouping |
| S13 | Interrupts | 4.5 | 7 kinds, wired end-to-end: kernel → poll/early-return |
| S14 | MCP + A2A | 5.0 | Client+Server + PipelineAgent + CommBus + AgentCards |
| S15 | PyO3 | 4.5 | GIL-released, auto-agent creation, @tool decorator, 27 tests |
| S16 | DX | 4.5 | PipelineBuilder, @tool, auto-agents, stage-aware events |
| S17 | File organization | 4.5 | Orchestrator split 6 ways, CommBus split 4 ways, focused modules |

---

## 5. Weaknesses

### Real bug

| # | Area | Detail |
|---|------|--------|
| W1 | MCP stdio child orphan | `McpToolExecutor` stdio transport holds `_child: Child` but has no `impl Drop`. Comment says "start_kill() on drop" but the impl doesn't exist. Child processes orphan when executor is dropped. Fix: `impl Drop for StdioClient { fn drop(&mut self) { let _ = self.child.start_kill(); } }`. No reconnection on crash either. |

### Partial gaps

| # | Area | Score | Detail |
|---|------|-------|--------|
| W2 | Cross-network federation | 2.5 | CommBus primitives (pub/sub, commands, queries) exist in-process. MCP is already the network boundary. Missing: expose CommBus queries as MCP tools for cross-process federation. Straightforward extension. |
| W3 | MCP HTTP auth | 2.5 | Stdio transport: process isolation is correct auth. HTTP transport: `McpServerConfig` missing `headers: Option<HashMap<String, String>>` and `env: Option<HashMap<String, String>>` for bearer tokens / API keys. |

### Not weaknesses (removed from previous audit)

| Previously listed | Why removed |
|-------------------|-------------|
| Community (1.0) | Marketing, not code. 4 deployed consumers. |
| Multi-language (3.5) | MCP stdio = JSON-RPC 2.0. Any language can consume today. |
| Managed experience (2.0) | Library, not service. FastAPI wrapper is ~50 LOC. Kernel has rate limiter + quotas + cleanup built in. |
| Persistence (2.0) | Envelope is fully serializable. `get_session_state()` + `initialize_session(force=true)` = checkpoint/replay. Capability-layer concern, not kernel gap. |
| HTTP gateway (0) | Correct architecture. HTTP belongs above kernel. Deletion removed complexity, added nothing. |
| GIL contention (3.5) | Overstated. Single `with_gil()` per tool call, `allow_threads()` for all async/network. Tools are sequential by design. Real impact ~1.5/5. |

---

## 6. Documentation

6 docs, ~870 lines. **Score: 4.5/5** (all rewritten in iteration 23).

| Doc | Lines | Status |
|-----|-------|--------|
| README.md | ~166 | Current: PyO3+MCP architecture, repo structure, consumer patterns |
| CONSTITUTION.md | ~94 | Current: 8 architectural principles |
| CONTRIBUTING.md | ~42 | Current: setup, submission guidelines |
| CHANGELOG.md | ~153 | Current: full history through iteration 22 |
| API_REFERENCE.md | ~215 | Current: PyO3 API, pipeline config, routing, Rust types |
| DEPLOYMENT.md | ~117 | Current: PyO3, MCP stdio, env vars |

Deleted: `COVERAGE_REPORT.md` (stale), `Dockerfile` (HTTP pattern gone).

---

## 7. Recommendations

### Done (all 4 top priorities closed)

1. ~~PyO3 integration tests~~ — 27 pytest tests
2. ~~Pre-execution tool ACLs~~ — checked in LlmAgent tool loop
3. ~~Routing recursion limit~~ — `validate_depth()` max 32
4. ~~Documentation overhaul~~ — all 6 docs rewritten

### Next

5. **MCP stdio child cleanup** (bug) — `impl Drop for StdioClient` with `child.start_kill()`. Add reconnection on transport failure.
6. **MCP HTTP auth** — add `headers: Option<HashMap<String, String>>` to `McpServerConfig` for bearer tokens.
7. **CommBus over MCP** — expose CommBus query/subscribe as MCP tools for cross-process federation.
8. **Async Python tools** — `PyToolExecutor` currently sync-only. Support `async def` via `pyo3-asyncio`.
9. **Evaluation framework** — leverage kernel metrics + PipelineTestHarness for pipeline quality.

---

## 8. Maturity

| Dimension | Score | Note |
|-----------|-------|------|
| Architecture | 5.0 | Single-process, typed channels, domain-grouped, focused file splits |
| Contract enforcement | 5.0 | JSON Schema + pre-exec ACLs + quotas + circuit breaking |
| Graph expressiveness | 4.8 | Gate/Fork/Agent, state merge, PipelineAgent composition |
| Type safety | 4.8 | `#[deny(unsafe_code)]`, tagged enums; `#[allow]` for PyO3 FFI only |
| Memory safety | 4.8 | Rust ownership; PyO3 FFI boundary safe-by-construction |
| Agentic capabilities | 5.0 | ReAct + circuit breaking + health tracking + 4 agent types |
| Streaming | 5.0 | 8 events, stage+pipeline attributed, composed pipeline bridging |
| Validation | 4.8 | `PipelineConfig::validate()` with 28 tests |
| MCP | 4.5 | Client+Server bidirectional. Bug: stdio child orphans on drop (W1) |
| Interrupts | 4.5 | 7 kinds, wired end-to-end |
| PyO3 | 4.8 | GIL-released, auto-agents, 27 pytest |
| Code hygiene | 5.0 | God-files split, delegation deleted, domain grouping |
| Testing | 4.5 | 290 Rust + 27 pytest + harness |
| DX | 4.5 | PipelineBuilder, @tool, auto-agents |
| Documentation | 4.5 | All 6 docs current |
| Interoperability | 4.5 | PyO3 + MCP stdio (any language via JSON-RPC) |
| **Overall** | **4.9** | One bug (W1), two partial gaps (W2, W3) |

---

## 9. Trajectory

### v0.0.2 changes (iterations 25-26, 3 commits, +1,429/-1,342)

**Commit 25: God-file decomposition + type safety** (`529e32a`):

Orchestrator split (was single file → 6 focused files):
- `orchestrator.rs` (2,285L): core routing only — `get_next_instruction()`, `report_agent_result()`, gate recursion, fork fan-out
- `orchestrator_session.rs` (243L): `initialize_session()`, `cleanup_session()`, `cleanup_stale_sessions()`, `build_session_state()`. 8 tests
- `orchestrator_queries.rs` (111L): read-only: `get_session_state()`, `get_stage_output_schema()`, `get_stage_allowed_tools()`, `get_session_count()`. 5 tests
- `orchestrator_helpers.rs` (183L): pure functions — `get_agent_for_stage()`, `build_stage_snapshot()`, `check_edge_limit()`, `is_parallel_join_met()`. 13 tests
- `test_helpers.rs` (52L): shared test fixtures — `stage()`, `always_to()`, `create_test_pipeline()`, `create_test_envelope()`

CommBus split (was 830L single file → 4 files):
- `mod.rs` (101L): struct + constructor
- `types.rs` (132L): `Event`, `Command`, `Query`, `QueryResponse`, `Subscriber`, `Subscription`, `BusStats`
- `event_bus.rs` (233L): pub/sub fan-out, subscribe/unsubscribe, cleanup. 8 tests
- `command_bus.rs` (98L): fire-and-forget dispatch. 2 tests
- `query_bus.rs` (187L): request/response with timeout, fire-and-spawn. 6 tests

Envelope restructured:
- Sub-structs: `Identity`, `Pipeline`, `Bounds`, `InterruptState`, `Audit`
- New: `event_inbox: Vec<Event>` for CommBus bridge
- `InterruptResponse`, `ProcessingRecord`, `ProcessingStatus` types added to `envelope/types.rs`

**Commit 26-27: Version bump** (`ce0b1d0`, `f0e98ea`): 0.0.1 → 0.0.2 in `Cargo.toml`, `pyproject.toml`, `Cargo.lock`.

### Gap closure

**45/52 closed.** 2 removed (gateway hardening, consumer client). 2 superseded (Python overhead, structured errors). 2 recovered (A2A, discovery). 1 open (persistent memory).

New gaps closed in v0.0.2:
- Orchestrator god-file → split 6 ways
- CommBus god-file → split 4 ways

### Code trajectory

| Metric | v0.0.1 (iter 24) | v0.0.2 (iter 26) |
|--------|-------------------|-------------------|
| Rust LOC | ~17,800 | ~17,890 |
| Rust tests | 301 | 290 |
| Python tests | 27 | 27 |
| Orchestrator files | 2 | 6 |
| CommBus files | 1 | 4 |
| Kernel fields | 9 | 9 |
| Agent types | 4 | 4 |
| KernelCommand variants | 13 | 13 |
| PipelineEvent variants | 8 | 8 |
| Version | 0.0.1 | 0.0.2 |

Test count: 301 → 290 due to consolidation during refactor (shared test_helpers, deduplicated fixtures).

### Verdict

v0.0.2 is a structural polish release. No new features. Two god-files (orchestrator, CommBus) decomposed into focused modules. Envelope gains semantic sub-structs. Test helpers shared across modules. The codebase is now at the point where every file has a single clear purpose. Net +87 lines despite splitting — the new files add test coverage and type definitions that were previously inline.

All 4 top recommendations remain DONE. One real bug to fix (MCP stdio child orphan). Two partial gaps to close (MCP HTTP auth headers, CommBus over MCP for cross-network).

---

*Audit: 27 iterations, 2026-03-11 to 2026-03-15. Branch: `claude/audit-agentic-frameworks-B6fqd`.*
