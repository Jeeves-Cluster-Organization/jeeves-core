# Architecture Review: Execution Model

**Date:** 2026-01-19
**Status:** For Review

This document flags architectural issues discovered during the cleanup pass that require design decisions before implementation.

---

## Issue 1: Misleading "DAG" Terminology

### Current State
The codebase uses "DAG" (Directed Acyclic Graph) terminology in several places:
- `DAGMode` / `dag_mode` - Flag in GenericEnvelope
- `RunParallel()` comment: "DAG-style pipelines"
- Test names: `TestDAGStateRoundtrip`, `TestCloneWithDAGState`

### Problem
The system explicitly supports **cyclic routing** via:
- `LoopVerdictLoopBack` - Route back to earlier stages
- `EdgeLimits` - Control how many times a cycle can be traversed
- `MaxIterations` - Global bound on pipeline loops

Calling anything "DAG" is misleading because DAG means "Directed **Acyclic** Graph".

### What DAGMode Actually Means
`DAGMode` indicates **parallel execution mode**, not acyclicity. It's set when using `RunParallel()` to execute independent stages concurrently.

### Proposed Fix
Rename to accurately describe the concept:
- `DAGMode` → `ParallelMode`
- `dag_mode` → `parallel_mode`
- Update all comments

**But see Issue 2 first** - this rename may be unnecessary if the execution model is unified.

---

## Issue 2: Separate Execution Flows

### Current State
The runtime has three separate execution methods:
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
- Routing decisions

This creates:
1. **Maintenance burden** - Changes must be made in multiple places
2. **Inconsistency risk** - Methods can drift apart
3. **API confusion** - Caller must choose execution method

### Root Cause
Execution mode is expressed as **method choice** rather than **configuration**.

### Proposed Design
A unified execution model where:

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

// Single entry point
func (r *UnifiedRuntime) Run(ctx, env, threadID) (*GenericEnvelope, error)
```

The runtime internally:
1. Checks `PipelineConfig.ExecutionMode`
2. If parallel: finds stages with satisfied dependencies, runs concurrently
3. If sequential: follows routing rules one stage at a time
4. Both modes handle cycles via routing rules and bounds

### Benefits
- Single code path for core logic
- Execution mode is config, not API
- `ParallelMode` flag in envelope becomes read-only (set by runtime based on config)
- Streaming becomes an output mode, not a separate method

---

## Issue 3: Graph Terminology Clarification

### Two Distinct Graphs
The system has two graph concepts that should be clearly distinguished:

1. **Routing Graph** (can have cycles)
   - Defined by: `RoutingRules` in AgentConfig
   - Edges: Routing decisions (proceed, loop_back, advance)
   - Cycles: Allowed and bounded by `EdgeLimits` and `MaxIterations`

2. **Dependency Graph** (must be acyclic)
   - Defined by: `Requires` and `After` in AgentConfig
   - Edges: Execution dependencies
   - Used for: Parallel execution ordering
   - Cycles: Would cause deadlock (should be validated)

### Current Confusion
The term "DAG" conflates these:
- The routing graph is NOT a DAG (cycles allowed)
- The dependency graph SHOULD be a DAG (cycles = error)

### Proposed Clarification
Documentation and code should clearly distinguish:
- "Pipeline graph" or "Routing graph" for the execution flow
- "Dependency graph" for parallel execution ordering
- Never use "DAG" to describe the overall system

---

## Recommendation

Address these issues together in a single design pass:

1. **Design unified execution model** (Issue 2)
   - Single `Run()` method
   - Execution mode in config
   
2. **Rename based on new design** (Issue 1)
   - If unified model: may not need `ParallelMode` flag at all
   - If kept: rename to `ParallelMode`

3. **Update documentation** (Issue 3)
   - Clearly distinguish routing graph vs dependency graph
   - Remove all "DAG" references for the routing graph

---

## Files Affected

If full refactoring is approved:

### Go (coreengine)
- `runtime/runtime.go` - Unify execution methods
- `envelope/generic.go` - Rename/remove DAGMode
- `envelope/generic_test.go` - Update tests
- `config/pipeline.go` - Add ExecutionMode

### Python (jeeves_protocols)
- `envelope.py` - Mirror Go changes
- Test files

### Documentation
- `HANDOFF.md` - Update runtime section
- `docs/CONTRACTS.md` - Clarify graph types
- `jeeves_mission_system/CONSTITUTION.md` - Update architecture diagrams

---

*This document requires user review before implementation.*
