# Jeeves-Core Future Plan

**Created:** 2026-01-06
**Last Updated:** 2026-01-06 (Hardening Review)
**Status:** Planning Document
**Purpose:** Capture identified gaps and roadmap for future development

---

## Executive Summary

This document captures gaps identified during the 2026-01-06 architecture review session. The core finding is that jeeves-core is currently **"opinionated structured glue"** rather than a full runtime - the Go layer orchestrates but Python does the actual work.

### Key Decisions from Hardening Review (2026-01-06)

| Decision | Resolution |
|----------|------------|
| **Backward compat** | **DELETE old code, don't wrap it. No fallbacks.** |
| Executor naming | Delete `DAGExecutor`, create `StateMachineExecutor` |
| Execution paradigm | Unified executor. Core doesn't judge graph structure. |
| IPC mechanism | gRPC only. Delete subprocess client. |
| Envelope sync | Contract tests (Go → Python → Go round-trips) |
| Import linter | **BLOCKER** - completed ✓ |
| Bounds authority | Go in-loop, Python post-hoc audit |
| Cycle limits | Global cap + per-edge configurable (capability layer) |
| LifecycleManager coverage | Raise to 80%+ before Phase 2 |
| Temporal/Cadence | Adopt only if gains outweigh complexity |

### Code to DELETE (not deprecate)

| File/Pattern | Reason |
|--------------|--------|
| `dag_executor.go` | Replaced by StateMachineExecutor |
| `jeeves_protocols/client.py` | Replaced by gRPC client |
| `CyclePolicyReject` | Core doesn't validate graph structure |
| `EnableDAGExecution` flag | Always state machine |
| `EnableArbiter` flag in PipelineConfig | Capability concern |
| `if go_client else python` patterns | Go only. Fail loudly. |
| `topologicalOrder` in PipelineConfig | Not needed with cycles |

---

## Table of Contents

1. [Identified Gaps](#1-identified-gaps)
2. [Phase 1: Immediate Cleanup](#2-phase-1-immediate-cleanup)
3. [Phase 2: Graph Executor Rewrite](#3-phase-2-graph-executor-rewrite)
4. [Phase 3: Go Runtime Expansion](#4-phase-3-go-runtime-expansion)
5. [Phase 4: Full Go Runtime](#5-phase-4-full-go-runtime)
6. [Decision Log](#6-decision-log)
7. [Open Questions](#7-open-questions)

---

## 1. Identified Gaps

### 1.1 Graph Executor Misnaming

**Problem:** The executor is called `DAGExecutor` but the system supports cycles.

```
Current graph structure (NOT a DAG):

Perception → Intent → Planner → Executor → Synthesizer → Critic
                ↑         ↑                                  │
                │         └────────[REPLAN]──────────────────┤
                └────────────────[REINTENT]──────────────────┘
```

**Impact:**
- Misleading terminology
- DAGExecutor can't actually handle cycles (sequential executor does via routing_rules)
- No proper state rollback for retry loops
- No nested loop support

### 1.2 Go Runtime is Orchestration-Only

**Current state:**
```
Request → Python (FastAPI)
              ↓
         Go (DAG orchestration) ←── "When should agents run?"
              ↓
         Python (LLM call) ←── Actual HTTP request
              ↓
         Python (Tool execution) ←── Actual file I/O
              ↓
         Python (Database) ←── Actual PostgreSQL queries
```

**Problem:** Go adds IPC overhead without providing performance benefits.

| Component | Current | Should Be | Performance Impact |
|-----------|---------|-----------|-------------------|
| HTTP Server | Python (FastAPI) | Go | 10-100x less memory |
| LLM Requests | Python | Go | Eliminate IPC |
| File I/O Tools | Python | Go | Native async |
| PostgreSQL | Python (psycopg2) | Go (pgx) | 3-5x faster |
| Rate Limiting | Python | Go | True parallelism |

### 1.3 Layer Boundary Enforcement

**Problem:** Layer boundaries are convention-only, not enforced by tooling.

```python
# Nothing prevents this violation:
from jeeves_avionics.tools.catalog import ToolId  # Capability importing from avionics
```

**Need:** Import linter or pre-commit hook to enforce layer rules.

### 1.4 Remaining Cleanup Items

From 2026-01-06 session:

| Item | Status | Description |
|------|--------|-------------|
| `resilient_ops` module | Pending | Verify usage or remove dead code |
| `CapabilityResourceRegistry` | Pending | Verify bootstrap.py queries registry |
| Logging chain | Pending | Verify shared → avionics flow |

---

## 2. Phase 1: Immediate Cleanup

**Timeline:** 1-2 sessions
**Goal:** Complete outstanding cleanup from architecture review

### 2.1 Verify resilient_ops Module

```bash
# Check if resilient_ops is used anywhere
rg "resilient_ops|RESILIENT_OPS" --type py
```

- [ ] If unused: Delete module and skipped tests
- [ ] If used: Document usage and fix any issues

### 2.2 Verify CapabilityResourceRegistry Wiring

```python
# bootstrap.py should query:
registry = get_capability_resource_registry()
schemas = registry.get_schemas()  # Used?
modes = registry.get_mode_configs()  # Used?
```

- [ ] Audit bootstrap.py for registry usage
- [ ] Document or wire up unused registry features

### 2.3 Verify Logging Chain

```
jeeves_shared.logging → jeeves_avionics.logging → mission_system.adapters
```

- [ ] Ensure no direct avionics imports from capabilities
- [ ] Verify adapters.get_logger() works correctly

### 2.4 Add Import Linter ⚠️ PHASE 2 BLOCKER

**Decision (D14):** This MUST complete before Phase 2 begins.

Create pre-commit hook or CI check:

```python
# scripts/check_layer_boundaries.py
LAYER_RULES = {
    "jeeves_protocols": [],  # L0: imports nothing
    "jeeves_shared": ["jeeves_protocols"],
    "jeeves_control_tower": ["jeeves_protocols", "jeeves_shared"],
    "jeeves_memory_module": ["jeeves_protocols", "jeeves_shared"],
    "jeeves_avionics": ["jeeves_protocols", "jeeves_shared", "jeeves_control_tower", "jeeves_memory_module"],
    "jeeves_mission_system": ["jeeves_protocols", "jeeves_shared", "jeeves_avionics", ...],
}

def check_imports(file_path: str) -> List[str]:
    """Return list of layer violations."""
    ...
```

### 2.5 Add Contract 11: Envelope Sync Tests

**Decision (D12):** Ensure Go → Python → Go envelope roundtrips are lossless.

```python
# tests/contracts/test_envelope_roundtrip.py
def test_envelope_go_python_go_roundtrip():
    """Verify envelope serialization is lossless."""
    # Create envelope via gRPC
    grpc = GrpcGoClient()
    go_envelope = grpc.create_envelope(...)
    
    # Serialize to Python
    py_envelope = GenericEnvelope.from_dict(go_envelope.to_state_dict())
    
    # Serialize back to Go
    go_envelope_2 = grpc.create_from_dict(py_envelope.to_state_dict())
    
    # Assert equality
    assert go_envelope.to_state_dict() == go_envelope_2.to_state_dict()
```

### 2.6 Raise LifecycleManager Coverage ⚠️ PHASE 2 BLOCKER

**Decision (D17):** LifecycleManager at 22% coverage is unacceptable.

- [ ] Add tests for `submit()`, `schedule()`, `terminate()`
- [ ] Add tests for state transitions
- [ ] Target: 80%+ line coverage

---

## 3. Phase 2: StateMachineExecutor Rewrite

**Timeline:** 2-4 sessions
**Goal:** Replace DAGExecutor with unified `StateMachineExecutor`
**BLOCKER:** Phase 1 (Import Linter) must complete first

### 3.1 Rename and Restructure

**Decision (2026-01-06):** Name is `StateMachineExecutor`, not `GraphExecutor` or `DAGExecutor`.

```go
// OLD: coreengine/runtime/dag_executor.go
type DAGExecutor struct { ... }

// NEW: coreengine/runtime/state_machine_executor.go
type StateMachineExecutor struct {
    Config       *config.PipelineConfig
    Agents       map[string]*agents.UnifiedAgent
    Logger       agents.Logger
    Persistence  PersistenceAdapter
    
    // Unified execution (merges sequential + parallel best practices)
    // - Sequential: routing_rules, cycle handling
    // - Parallel: goroutine coordination, channel-based completion
    
    // Cycle handling (global + per-edge)
    globalMaxIterations int                      // Global cap (from config)
    edgeLimits          map[string]*EdgeLimit    // Per-edge limits (capability-defined)
    visitCounts         map[string]int           // Current visit counts per stage
    
    // State management
    checkpoints  map[string]*envelope.GenericEnvelope
    transitions  []Transition
    
    // Concurrency control
    mu            sync.RWMutex
    activeCount   int
    maxParallel   int
}

// EdgeLimit allows capability layer to configure per-transition limits
type EdgeLimit struct {
    From      string
    To        string
    MaxCount  int
    Current   int
}

// No more ExecutionMode enum - unified executor handles all cases
```

### 3.2 Proper Cycle Support

```go
// Transition with cycle awareness
type Transition struct {
    From        string
    To          string
    Reason      TransitionReason  // ROUTING_RULE, DEFAULT, REPLAN, REINTENT, ERROR
    Iteration   int               // Which iteration of the cycle
    Timestamp   time.Time
}

// Cycle limits per edge, not just global
type CycleLimit struct {
    From      string
    To        string
    MaxCount  int
    Current   int
}
```

### 3.3 State Rollback for Retries

```go
// Checkpoint at key points
func (e *GraphExecutor) checkpoint(stage string, env *envelope.GenericEnvelope) {
    e.checkpoints[stage] = env.Clone()
}

// Rollback for retry
func (e *GraphExecutor) rollbackTo(stage string) (*envelope.GenericEnvelope, error) {
    checkpoint, exists := e.checkpoints[stage]
    if !exists {
        return nil, fmt.Errorf("no checkpoint for stage: %s", stage)
    }
    return checkpoint.Clone(), nil
}
```

### 3.4 Nested Loop Support

```go
// Loop context stack
type LoopContext struct {
    LoopID      string
    StartStage  string
    Iteration   int
    MaxIter     int
    Parent      *LoopContext  // For nested loops
}

func (e *GraphExecutor) enterLoop(loopID, startStage string, maxIter int) {
    ctx := &LoopContext{
        LoopID:     loopID,
        StartStage: startStage,
        MaxIter:    maxIter,
        Parent:     e.currentLoop,
    }
    e.loopStack = append(e.loopStack, ctx)
    e.currentLoop = ctx
}
```

---

## 4. Phase 3: Go Runtime Expansion

**Timeline:** 4-8 sessions
**Goal:** Move performance-critical operations from Python to Go

### 4.1 Native Tool Execution

**Priority tools to implement in Go:**

| Tool | Complexity | Performance Gain |
|------|------------|-----------------|
| `read_file` | Low | High (no Python I/O) |
| `glob_files` | Low | High (filepath.Glob) |
| `grep_search` | Medium | Very High (no subprocess) |
| `tree_structure` | Low | Medium |
| `git_log` | Medium | Medium (exec.Command) |

```go
// coreengine/tools/native/file_tools.go
func ReadFile(path string, startLine, endLine int) ([]byte, error) {
    // Native Go implementation
}

func GlobFiles(pattern string, root string) ([]string, error) {
    // Use filepath.Glob or doublestar
}

func GrepSearch(pattern, path string, opts GrepOptions) ([]GrepMatch, error) {
    // Use ripgrep binary or implement in Go
}
```

### 4.2 LLM Gateway in Go

```go
// coreengine/llm/gateway.go
type LLMGateway struct {
    providers map[string]*ProviderConfig
    client    *http.Client
    limiter   *rate.Limiter
}

func (g *LLMGateway) Generate(ctx context.Context, req GenerateRequest) (*GenerateResponse, error) {
    // Direct HTTP to LLM server
    // No Python IPC
}
```

**Benefits:**
- Eliminate Python ↔ Go IPC for LLM calls
- Native connection pooling
- Goroutine-based streaming

### 4.3 Database Client in Go

```go
// coreengine/database/pgx_client.go
import "github.com/jackc/pgx/v5"

type PgxClient struct {
    pool *pgxpool.Pool
}

func (c *PgxClient) SaveState(ctx context.Context, threadID string, state map[string]any) error {
    // Direct PostgreSQL via pgx (3-5x faster than psycopg2)
}
```

---

## 5. Phase 4: Full Go Runtime

**Timeline:** 8-16 sessions
**Goal:** Go handles everything except complex business logic

### 5.1 Go HTTP Server

Replace FastAPI with Go:

```go
// cmd/server/main.go
func main() {
    router := chi.NewRouter()
    
    router.Post("/api/v1/chat/messages", handleMessage)
    router.Get("/api/v1/chat/sessions", handleListSessions)
    router.Get("/health", handleHealth)
    
    http.ListenAndServe(":8000", router)
}
```

**Benefits:**
- Single binary deployment
- 10-100x less memory per connection
- Native WebSocket/SSE support

### 5.2 Python Only for Complex Logic

After full Go runtime, Python is used ONLY for:

```
Python Responsibility (via gRPC):
├── Complex LLM prompt engineering
├── LangChain integrations (if needed)
├── ML-specific libraries (transformers, etc.)
├── Custom business logic hooks
└── Third-party Python SDK integrations
```

### 5.3 Deployment Simplification

**Before (current):**
```
Docker image:
├── Python 3.11
├── All Python dependencies (500MB+)
├── Go binary
└── Complex entrypoint script
```

**After (full Go):**
```
Docker image:
├── Go binary (50MB)
└── Optional: Python sidecar for complex logic
```

---

## 6. Decision Log

### Early Session (2026-01-06)

| ID | Decision | Rationale |
|----|----------|-----------|
| D1 | ToolId is capability-owned | Prevents L5→L2 layer violation |
| D2 | Remove ToolId from avionics catalog | Capability defines its own enum |
| D3 | Remove ToolId from contracts_core exports | Force capabilities to use own |
| D4 | Keep resilient_ops tests skipped | Verify before removing |
| D5 | Verify registry before changes | Don't break working code |

### Hardening Review Session (2026-01-06)

| ID | Decision | Rationale |
|----|----------|-----------|
| D6 | Rename `DAGExecutor` → `StateMachineExecutor` | Misleading name; system supports cycles |
| D7 | Merge sequential + DAG execution | Two paradigms is technical debt |
| D8 | Fix DAG validation to allow cycles | Current validation rejects cycles but system needs them |
| D9 | Migrate IPC from subprocess to gRPC | Subprocess spawns per operation = overhead |
| D10 | Measure subprocess IPC latency | No current baseline for performance |
| D11 | Keep LLM/prompt logic in Python | Refine Go/Python boundary later |
| D12 | Envelope sync via contract tests | Go → Python → Go round-trip tests |
| D13 | Stronger `FlowInterrupt` typing in Python | Python currently uses `Dict[str, Any]` |
| D14 | Import linter is Phase 2 BLOCKER | Violations compound daily |
| D15 | Cycle limits: global + per-edge | Global cap, per-edge configurable by capability |
| D16 | Bounds authority: defense in depth | Go enforces in-loop, Python audits post-hoc |
| D17 | LifecycleManager to 80%+ coverage | Currently at 22%, manages critical state |
| D18 | Temporal: evaluate carefully | Adopt only if gains outweigh complexity |

---

## 7. Open Questions

### Resolved ✅

| Question | Resolution | Decision ID |
|----------|------------|-------------|
| Should we support nested loops? | Yes, via StateMachineExecutor | D6-D8 |
| What's the Go/Python boundary? | Python keeps LLM/prompts, refine later | D11 |
| Use Temporal/Cadence? | Evaluate carefully, adopt only if gains outweigh | D18 |

### Still Open 🔴

1. **Recovery protocol for IPC failures**
   - What happens if Go binary crashes mid-envelope?
   - Python has no way to recover state currently
   - **Priority:** Lower (deferred)

2. **Parallel execution data races**
   - Multiple goroutines write to envelope concurrently
   - Field ownership matrix needed
   - **Priority:** Add to Phase 2 rewrite scope

3. **Envelope Clone() implementation**
   - Required for state rollback
   - Not yet implemented in Go envelope
   - **Priority:** Phase 2 prerequisite

4. **gRPC migration path**
   - How to migrate from subprocess to gRPC incrementally?
   - Options: Feature flag, strangler pattern
   - **Priority:** Design before Phase 3

### New Findings (Hardening Review)

5. **Authority split: ControlTower vs Go Runtime**
   - Both track bounds independently
   - Go: in-loop enforcement via `env.CanContinue()`
   - Python: post-hoc audit via `ResourceTracker.check_quota()`
   - **Resolution:** Defense in depth (D16), but need invariant check

6. **DAG validation rejects cycles**
   - `validateDAG()` explicitly rejects cycles ("dependency cycle detected")
   - But system architecture requires REINTENT cycles
   - **Resolution:** Fix validation to support intentional cycles (D8)

7. **Envelope sync hazard**
   - Go: 983-line GenericEnvelope with typed FlowInterrupt
   - Python: 268-line GenericEnvelope with `Dict[str, Any]` interrupt
   - **Resolution:** Contract tests (D12), stronger Python typing (D13)

---

## Appendix A: Competitive Gap Analysis

| Feature | Jeeves-Core | LangGraph | Temporal |
|---------|-------------|-----------|----------|
| Cyclic Graphs | Hacky | Native | Native |
| State Rollback | No | Checkpoint | Full replay |
| Nested Loops | No | Manual | Native |
| Native Performance | Partial Go | Python | Go/Java |
| Deployment | Complex | Simple | Complex |

---

## Appendix B: Resource Estimates

| Phase | Sessions | Risk | Dependencies |
|-------|----------|------|--------------|
| Phase 1 | 1-2 | Low | None |
| Phase 2 | 2-4 | Medium | Phase 1 |
| Phase 3 | 4-8 | High | Phase 2 |
| Phase 4 | 8-16 | High | Phase 3 |

**Total estimated effort:** 15-30 sessions for full Go runtime

---

## Appendix C: Files to Modify

### Phase 1 (Cleanup + Blockers)
```
jeeves-core/
├── jeeves_avionics/tools/catalog.py      # Remove ToolId (DONE)
├── jeeves_mission_system/contracts_core.py  # Remove ToolId exports (DONE)
├── jeeves_mission_system/bootstrap.py    # Verify registry usage
├── scripts/check_layer_boundaries.py     # CREATE: Import linter (BLOCKER)
├── docs/CONTRACTS.md                     # ADD: Contract 11 (Envelope Sync)
└── jeeves_protocols/envelope.py          # ADD: Typed FlowInterrupt (align with Go)
```

### Phase 1.5 (Coverage Hardening - BLOCKER)
```
jeeves-core/
├── jeeves_control_tower/lifecycle/manager.py    # COVERAGE: 22% → 80%+
├── jeeves_control_tower/tests/test_lifecycle.py # ADD: Critical path tests
└── tests/contracts/test_envelope_roundtrip.py   # CREATE: Go↔Python sync tests
```

### Phase 2 (StateMachineExecutor)
```
jeeves-core/coreengine/
├── runtime/
│   ├── dag_executor.go              # DELETE (after migration)
│   ├── state_machine_executor.go    # CREATE: Unified executor
│   ├── checkpoints.go               # CREATE: State rollback
│   └── edge_limits.go               # CREATE: Per-edge cycle limits
├── envelope/
│   └── generic.go                   # ADD: Clone() method
└── config/
    └── pipeline.go                  # ADD: EdgeLimit config, remove cycle rejection
```

### Phase 3 (Go Tools)
```
jeeves-core/coreengine/
├── tools/
│   ├── native/
│   │   ├── file_tools.go     # CREATE: read_file, glob
│   │   ├── search_tools.go   # CREATE: grep
│   │   └── git_tools.go      # CREATE: git_log
│   └── executor.go           # UPDATE: Route to native
└── llm/
    └── gateway.go            # CREATE: Direct LLM calls
```

### Phase 4 (Full Runtime)
```
jeeves-core/
├── cmd/
│   └── server/
│       └── main.go           # CREATE: Go HTTP server
├── coreengine/
│   └── database/
│       └── pgx_client.go     # CREATE: Native PostgreSQL
└── docker/
    └── Dockerfile            # UPDATE: Single binary
```

---

*End of Future Plan Document*

