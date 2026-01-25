# Architecture Review: Execution Model

**Date:** 2026-01-22
**Status:** Complete

This document tracks architectural improvements to the execution model.

---

## Completed: Issue 1 - DAG → ParallelMode Rename

### Changes Made
Renamed `DAGMode` / `dag_mode` to `ParallelMode` / `parallel_mode` throughout the codebase:

**Go Files:**
- `coreengine/envelope/envelope.go` - Field renamed, JSON key updated
- `coreengine/envelope/generic_test.go` - Test names and assertions updated
- `coreengine/config/pipeline.go` - Comment updated
- `coreengine/runtime/runtime.go` - Comment updated
- `coreengine/grpc/server.go` - Comments updated
- `coreengine/proto/engine.proto` - Comment updated

**Python Files:**
- `protocols/envelope.py` - Field renamed, JSON keys updated
- `protocols/agents.py` - Docstring updated

### Rationale
"DAG" (Directed Acyclic Graph) was misleading because:
- The routing graph explicitly supports cycles via `loop_back` routing
- The flag actually indicates **parallel execution mode**, not acyclicity
- `ParallelMode` accurately describes what the flag controls

---

## Next: Issue 2 - Execution Flow Unification

### Completed

Refactored the runtime to use a single `Execute()` method with `RunOptions`:

```go
type RunMode string
const (
    RunModeSequential RunMode = "sequential"
    RunModeParallel   RunMode = "parallel"
)

type RunOptions struct {
    Mode     RunMode  // sequential or parallel
    Stream   bool     // send outputs to channel
    ThreadID string   // for persistence
}

// Single entry point
func (r *Runtime) Execute(ctx, env, opts RunOptions) (*Envelope, <-chan StageOutput, error)

// Convenience methods
func (r *Runtime) Run(ctx, env, threadID) (*Envelope, error)
func (r *Runtime) RunParallel(ctx, env, threadID) (*Envelope, error)
func (r *Runtime) RunWithStream(ctx, env, threadID) (<-chan StageOutput, error)
```

**Changes Made:**
- Renamed `UnifiedRuntime` to `Runtime`
- Added `RunMode` enum to `config.PipelineConfig`
- Created internal `runSequentialCore` and `runParallelCore` methods
- `Execute()` delegates to the appropriate core based on mode
- `ParallelMode` flag set automatically when using parallel mode

---

## Issue 3: Graph Terminology

The system has two graph concepts:

1. **Routing Graph** (can have cycles)
   - Defined by: `RoutingRules` in AgentConfig
   - Cycles: Allowed, bounded by `EdgeLimits` and `MaxIterations`

2. **Dependency Graph** (must be acyclic)
   - Defined by: `Requires` and `After` in AgentConfig
   - Used for: Parallel execution ordering

---

## Completed: Issue 4 - Runtime shouldContinue() Consolidation

### Changes Made

Refactored `shouldContinue()` in `coreengine/runtime/runtime.go` to consolidate
continuation logic:

**Before:**
```go
func (r *Runtime) shouldContinue(env *Envelope) (bool, string) {
    if !env.CanContinue() {
        r.Logger.Warn("pipeline_bounds_exceeded", ...)
        return false, "bounds_exceeded"
    }
    if env.InterruptPending {
        r.Logger.Info("pipeline_interrupt", ...)
        return false, "interrupt_pending"
    }
    return true, ""
}
```

**After:**
```go
func (r *Runtime) shouldContinue(env *Envelope) (bool, string) {
    if !env.CanContinue() {
        // CanContinue() already checks: Terminated, InterruptPending, bounds
        reason := "cannot_continue"
        if env.Terminated {
            reason = "terminated"
        } else if env.InterruptPending {
            reason = "interrupt_pending"
            r.Logger.Info("pipeline_interrupt", ...)
        } else if env.TerminalReason_ != nil {
            reason = "bounds_exceeded"
            r.Logger.Warn("pipeline_bounds_exceeded", ...)
        }
        return false, reason
    }
    return true, ""
}
```

### Rationale

1. **Single source of truth**: `CanContinue()` is the authoritative check
2. **No redundant checks**: Removed duplicate `InterruptPending` check
3. **Better reason reporting**: Now returns specific reason (terminated, interrupt, bounds)
4. **Cleaner logging**: Logging happens only when appropriate condition is met

---

## Completed: Issue 5 - Dead Code Removal (contracts.go)

### Removed Functions

From `coreengine/agents/contracts.go`:

1. **`AgentOutcomeFromString()`** - String parsing for AgentOutcome enum
   - Unused after routing rules moved to config-driven approach
   - Outcomes are now set directly, not parsed from strings

2. **`NewEvidence()`** - Helper constructor for Evidence struct
   - Callers create Evidence directly with struct literals
   - Constructor added no value over direct initialization

### Rationale

Per Contract 6 (Dead Code Removal): Unused code must be removed, not kept "for future use."

---

## Completed: Issue 6 - gRPC Client Implementation

### Added Modules

1. **`protocols/grpc_client.py`** (~710 lines)
   - Full gRPC client for Go runtime
   - Contract 11 compliance: Lossless envelope round-trip
   - Contract 12 compliance: Go authoritative for bounds

2. **`protocols/grpc_stub.py`** (~60 lines)
   - Clean re-exports of proto types
   - Provides `protocols.grpc_stub` import path

### Key Types

```python
class GrpcGoClient:
    def create_envelope(...) -> Envelope
    def update_envelope(...) -> Envelope
    def check_bounds(...) -> BoundsResult
    def clone_envelope(...) -> Envelope
    def execute_agent(...) -> AgentResult
    def execute_pipeline(...) -> Iterator[ExecutionEvent]

@dataclass
class BoundsResult:
    can_continue: bool
    terminal_reason: Optional[str]
    llm_calls_remaining: int
    agent_hops_remaining: int
    iterations_remaining: int
```

---

*Architecture review complete. All execution model improvements implemented.*
