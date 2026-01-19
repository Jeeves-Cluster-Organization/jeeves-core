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

### Current State
The runtime has three separate execution methods with duplicated logic:

```go
func (r *UnifiedRuntime) Run(ctx, env, threadID)           // Sequential
func (r *UnifiedRuntime) RunStreaming(ctx, env, threadID)  // Sequential + streaming
func (r *UnifiedRuntime) RunParallel(ctx, env, threadID)   // Parallel
```

Plus:
```go
func (r *UnifiedRuntime) Resume(ctx, env, response, threadID)  // After interrupt
```

### Problem
All methods duplicate core logic:
- Bounds checking (`env.CanContinue()`)
- Interrupt handling (`env.InterruptPending`)
- State persistence
- Logging patterns
- Stage routing

This creates:
1. **Maintenance burden** - Changes must be made in multiple places
2. **Inconsistency risk** - Methods can drift apart
3. **API confusion** - Caller must choose execution method

### Proposed Design

#### Option A: Unified Method with Config
```go
type ExecutionMode string
const (
    ExecutionModeSequential ExecutionMode = "sequential"
    ExecutionModeParallel   ExecutionMode = "parallel"
)

type PipelineConfig struct {
    // ... existing fields ...
    ExecutionMode ExecutionMode  // Default: sequential
}

// Single entry point - execution mode from config
func (r *UnifiedRuntime) Run(ctx, env, threadID) (*GenericEnvelope, error)
```

#### Option B: Unified Core with Mode Parameter
```go
// Single entry point with mode parameter
func (r *UnifiedRuntime) Execute(ctx, env, threadID string, opts ExecutionOptions) (*GenericEnvelope, error)

type ExecutionOptions struct {
    Mode     ExecutionMode  // sequential, parallel
    Streaming bool          // whether to stream outputs
}
```

### Implementation Plan

1. **Create unified execution core** - Extract common logic into internal method
2. **Implement execution strategy pattern** - Sequential and parallel as strategies
3. **Refactor public methods** - Delegate to unified core
4. **Add streaming as output mode** - Not separate execution method
5. **Update ParallelMode flag** - Set automatically based on execution mode

### Benefits
- Single code path for core logic
- Execution mode is config/parameter, not API choice
- Streaming becomes an output option, not a separate method
- Easier to add new execution modes (e.g., hybrid)

---

## Issue 3: Graph Terminology Clarification

### Two Distinct Graphs
The system has two graph concepts that should be clearly documented:

1. **Routing Graph** (can have cycles)
   - Defined by: `RoutingRules` in AgentConfig
   - Edges: Routing decisions (proceed, loop_back, advance)
   - Cycles: Allowed and bounded by `EdgeLimits` and `MaxIterations`

2. **Dependency Graph** (must be acyclic)
   - Defined by: `Requires` and `After` in AgentConfig
   - Edges: Execution dependencies
   - Used for: Parallel execution ordering
   - Cycles: Would cause deadlock (should be validated)

### Documentation Updates Needed
- Add clear definitions to HANDOFF.md
- Add validation for dependency graph acyclicity
- Document the distinction in code comments

---

## Files Affected by Execution Unification

### Go (coreengine)
- `runtime/runtime.go` - Primary refactoring target
- `config/pipeline.go` - Add ExecutionMode if using Option A

### Python (jeeves_protocols)
- `agents.py` - Update UnifiedRuntime if mirroring Go changes

### Tests
- `runtime/runtime_test.go` - Update for unified API

---

*This document tracks progress on execution model improvements.*
