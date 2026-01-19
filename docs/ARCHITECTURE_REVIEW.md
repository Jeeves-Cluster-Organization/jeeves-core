# Architecture Review: Execution Model

**Date:** 2026-01-19
**Status:** In Progress

This document tracks architectural improvements to the execution model.

---

## Completed: Issue 1 - DAG → ParallelMode Rename

### Changes Made
Renamed `DAGMode` / `dag_mode` to `ParallelMode` / `parallel_mode` throughout the codebase:

**Go Files:**
- `coreengine/envelope/generic.go` - Field renamed, JSON key updated
- `coreengine/envelope/generic_test.go` - Test names and assertions updated
- `coreengine/config/pipeline.go` - Comment updated
- `coreengine/runtime/runtime.go` - Comment updated
- `coreengine/grpc/server.go` - Comments updated
- `coreengine/proto/jeeves_core.proto` - Comment updated

**Python Files:**
- `jeeves_protocols/envelope.py` - Field renamed, JSON keys updated
- `jeeves_protocols/agents.py` - Docstring updated

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
func (r *Runtime) Execute(ctx, env, opts RunOptions) (*GenericEnvelope, <-chan StageOutput, error)

// Convenience methods
func (r *Runtime) Run(ctx, env, threadID) (*GenericEnvelope, error)
func (r *Runtime) RunParallel(ctx, env, threadID) (*GenericEnvelope, error)
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

*Execution unification complete.*
