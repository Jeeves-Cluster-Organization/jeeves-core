# Architectural Debt: Core Layer Pollution

**Created:** 2026-01-18
**Updated:** 2026-01-18
**Severity:** Medium
**Status:** Documented, pending refactoring

---

## Problem Statement

The core layer (`coreengine/`, `jeeves_protocols/`) contains domain-specific concepts that should belong in the capability layer. This violates the principle that **core should be generic** and only the capability layer should define domain-specific agents, stages, and workflows.

---

## Quantified Scope

| Category | Count | Files Affected |
|----------|-------|----------------|
| "reintent/REINTENT" references | 94 | 19 files |
| Hardcoded stage names in Go runtime | 4 | `runtime.go` |
| Domain-specific enums in Go | 6 | `enums.go`, `contracts.go`, `core_config.go` |
| Domain-specific Python code | 80 matches | 13 files in `jeeves_protocols/` |

---

## Violations Found

### 1. Domain-Specific Enums in Core

**Location:** `coreengine/envelope/enums.go`

| Enum | Violation | Fix |
|------|-----------|-----|
| `CriticVerdict` | "Critic" is capability-specific | Rename to `LoopVerdict` |
| `CriticVerdictReintent` | Implies "Intent" agent | Rename to `LoopVerdictLoopBack` |
| `TerminalReasonMaxCriticFiresExceeded` | Domain-specific | Rename to `MaxLoopExceeded` |
| `InterruptKindCriticReview` | Domain-specific | Rename to `InterruptKindAgentReview` |

---

### 2. Hardcoded Stage Names in Runtime

**Location:** `coreengine/runtime/runtime.go:285-297`

```go
// VIOLATION: Core knows about "intent" and "executor" agents
case envelope.InterruptKindClarification:
    env.CurrentStage = "intent"        // ← Hardcoded!
case envelope.InterruptKindConfirmation:
    env.CurrentStage = "executor"      // ← Hardcoded!
case envelope.InterruptKindCriticReview:
    env.CurrentStage = "intent"        // ← Hardcoded!
```

**Fix:** Add configurable resume stages to `PipelineConfig`:
```go
type PipelineConfig struct {
    ClarificationResumeStage string  // Capability sets: "intent"
    ConfirmationResumeStage  string  // Capability sets: "executor"
    ReviewResumeStage        string  // Capability sets: "intent"
}
```

---

### 3. Domain-Specific Config Fields

**Location:** `coreengine/config/core_config.go`

| Field | Violation | Fix |
|-------|-----------|-----|
| `EnableCriticLoop` | "Critic" is domain-specific | Rename to `EnableLoopBack` |
| `MaxCriticRejections` | "Critic" is domain-specific | Move to EdgeLimits |
| `SkipPlannerIfSimpleIntent` | "Planner" and "Intent" | Remove or make generic |
| `EnableSynthesizer` | "Synthesizer" is domain-specific | Remove from core |
| `MaxReplanIterations` | "Replan" implies domain pattern | Rename to `MaxLoopIterations` |

---

### 4. Domain-Specific Agent Outcomes

**Location:** `coreengine/agents/contracts.go`

```go
// VIOLATION: "reintent" implies "Intent" agent exists
AgentOutcomeReintent  AgentOutcome = "reintent"

// FIX: Use generic names
AgentOutcomeLoopBack  AgentOutcome = "loop_back"
```

The `RequiresLoop()` method is correct in concept but uses wrong enum:
```go
func (o AgentOutcome) RequiresLoop() bool {
    return o == AgentOutcomeReplan || o == AgentOutcomeReintent  // ← "reintent" is wrong
}
```

---

### 5. Domain-Specific Python Code

**Location:** `jeeves_protocols/`

| File | Violation | Fix |
|------|-----------|-----|
| `envelope.py` | `critic_feedback: List[str]` | Rename to `loop_feedback` |
| `core.py` | `CriticVerdict` enum | Rename to `LoopVerdict` |
| `interrupts.py` | `InterruptKind.CRITIC_REVIEW` | Rename to `AGENT_REVIEW` |
| `events.py` | `INTENT_STARTED`, `CRITIC_DECISION` | Move to capability layer |
| `agents.py` | `context["intent"]`, `context["critic"]` | Use generic stage access |

---

### 6. Test Files (Lower Priority)

Test files use domain-specific stage names for convenience:
- `runtime_test.go`: Uses `"intent"`, `"planner"`, `"critic"`, `"executor"` 
- `generic_test.go`: Uses `"intent"`, `"planner"`
- `tests/conftest.py`: Uses domain-specific stage orders

**Assessment:** Test files can use domain-specific names as they simulate capability layer.
**Priority:** Low - update as part of larger refactoring.

---

## Root Cause Analysis

The "REINTENT Architecture" naming in documentation created a conceptual leak:

1. **Documentation used domain terminology** → Developers assumed it was a core concept
2. **Core code was written to match docs** → Domain terms entered core layer
3. **Tests reinforced the pattern** → Harder to notice the violation

**Actual capability:** Cyclic routing with bounded loops (generic)
**Documented as:** "REINTENT Architecture" (domain-specific)

---

## Constitutional Justification

The refactoring plan is **aligned with existing CONSTITUTION.md principles**:

### Avionics Constitution R5:
> "**ToolId enums are CAPABILITY-OWNED, not avionics-owned**"

This principle should extend to all domain-specific enums. Just as `ToolId` was moved 
to capability layer, `CriticVerdict` and `AgentOutcomeReintent` should follow.

### Mission System Constitution:
> "The following describes the **reference 7-agent pipeline** that capabilities can adopt. 
> **Capabilities may customize this pipeline** or define their own agent configurations."

This confirms that agent names (Critic, Intent, Planner) are capability-layer concepts.
Core should provide generic routing primitives, not assume specific agent names.

### Control Tower Constitution R1:
> "Control Tower **ONLY imports from** `jeeves_protocols` and `jeeves_shared`"

Domain-specific enums like `InterruptKindCriticReview` violate the spirit of this rule -
protocols should be generic, not embed capability-layer agent names.

### Mission System Constitution - Config Architecture:
> "**Capability layer OWNS domain-specific configs** (language, tool access, deployment, identity)"

Config fields like `EnableCriticLoop` and `MaxCriticRejections` are domain-specific 
and should follow the same ownership pattern.

---

## Impact Assessment

| Impact | Severity | Description |
|--------|----------|-------------|
| **Capability lock-in** | Medium | Core assumes Planner/Executor/Critic pattern |
| **Naming confusion** | Low | "reintent" only makes sense with "Intent" agent |
| **Maintenance burden** | Medium | Core changes when capability layer changes |
| **Testing complexity** | Low | Can still test with mocked stages |
| **Functionality** | None | Cyclic routing works correctly |

**Overall:** The system WORKS correctly. This is a naming/placement issue, not a functional bug.

---

## Recommended Refactoring Plan

### Phase 1: Documentation (Completed)
- [x] Update `pipeline.go` comments to use generic terminology
- [x] Create this `ARCHITECTURAL_DEBT.md` document
- [ ] Update `CONTRACTS.md` Contract 13 to use generic terms
- [ ] Add warnings to domain-specific code

### Phase 2: Rename Core Enums (Low Risk)
```
CriticVerdict → LoopVerdict
  - approved → proceed
  - reintent → loop_back
  - next_stage → advance

AgentOutcomeReintent → AgentOutcomeLoopBack

EnableCriticLoop → EnableLoopBack
MaxCriticRejections → MaxLoopRejections
critic_feedback → loop_feedback
```

**Estimated Impact:** ~150 lines across 20 files
**Risk:** Low (rename only, no logic change)

### Phase 3: Make Resume Stages Configurable (Medium Risk)
```go
// Before: hardcoded in runtime.go
env.CurrentStage = "intent"

// After: configurable in PipelineConfig
env.CurrentStage = r.Config.ClarificationResumeStage
```

**Estimated Impact:** ~10 lines in runtime.go + config changes
**Risk:** Medium (requires capability layer updates)

### Phase 4: Move Domain Events to Capability (Higher Risk)
```python
# Before: in jeeves_protocols/events.py
INTENT_STARTED = "intent.started"
CRITIC_DECISION = "critic.decision"

# After: in capability layer, use generic core events
AGENT_STARTED = "agent.started"  # Core provides this
# Capability adds: agent_name="intent" in metadata
```

**Estimated Impact:** ~100 lines across event-emitting code
**Risk:** Higher (changes event schema)

---

## Files Requiring Changes

### High Priority (Core Layer)
| File | Changes Needed |
|------|----------------|
| `coreengine/envelope/enums.go` | Rename enums |
| `coreengine/agents/contracts.go` | Rename `AgentOutcomeReintent` |
| `coreengine/config/core_config.go` | Rename config fields |
| `coreengine/runtime/runtime.go` | Make resume stages configurable |
| `jeeves_protocols/envelope.py` | Rename `critic_feedback` |
| `jeeves_protocols/core.py` | Rename `CriticVerdict` |
| `jeeves_protocols/interrupts.py` | Rename `CRITIC_REVIEW` |

### Medium Priority (Protocols Layer)
| File | Changes Needed |
|------|----------------|
| `jeeves_protocols/events.py` | Remove domain-specific events |
| `jeeves_protocols/agents.py` | Generalize context building |
| `jeeves_protocols/__init__.py` | Update exports |

### Lower Priority (Tests/Docs)
| File | Changes Needed |
|------|----------------|
| `coreengine/runtime/runtime_test.go` | Update test stage names |
| `coreengine/envelope/generic_test.go` | Update test stage names |
| `docs/CONTRACTS.md` | Update Contract 13 |
| `FUTURE_PLAN.md` | Update terminology |
| `TECHNICAL_ASSESSMENT.md` | Update terminology |

---

## Decision Required

Before proceeding with refactoring:

1. **Keep backward compatibility?** 
   - If yes: Add aliases during transition
   - If no: Direct rename (breaking change)

2. **Scope of first refactor?**
   - Option A: Rename only (Phase 2) - Low risk
   - Option B: Rename + configurable stages (Phase 2+3) - Medium risk
   - Option C: Full cleanup (All phases) - Higher risk

**Recommendation:** Start with Phase 2 (renames only) as it has minimal risk and addresses the most visible violations.

---

## Current State Summary

| Aspect | Status |
|--------|--------|
| **Functionality** | ✅ Working correctly |
| **Core abstraction** | ⚠️ Leaky (domain terms in core) |
| **Documentation** | ⚠️ Partially updated |
| **Tests** | ⚠️ Use domain-specific names |

The system works. This is technical debt for future refactoring, not a blocking issue.

---

## Appendix: Parallel Execution Status

**Status:** Infrastructure complete, execution loop not wired

The parallel execution infrastructure is **fully implemented**:

| Component | Status | Location |
|-----------|--------|----------|
| `ActiveStages` map | ✅ Exists | `envelope/generic.go` |
| `CompletedStageSet` map | ✅ Exists | `envelope/generic.go` |
| `DAGMode` flag | ✅ Exists | `envelope/generic.go` |
| `StartStage()` method | ✅ Exists | `envelope/generic.go` |
| `CompleteStage()` method | ✅ Exists | `envelope/generic.go` |
| `FailStage()` method | ✅ Exists | `envelope/generic.go` |
| `Requires` dependency field | ✅ Exists | `config/pipeline.go` |
| `After` ordering field | ✅ Exists | `config/pipeline.go` |
| `GetAllDependencies()` | ✅ Exists | `config/pipeline.go` |
| `adjacencyList` built | ✅ Exists | `config/pipeline.go` |
| **Parallel execution loop** | ❌ Missing | `runtime/runtime.go` |

**Work remaining:** ~50-100 lines of goroutine coordination:
1. `getExecutableStages()` - find stages with satisfied dependencies (~50 lines)
2. Goroutine spawning with `sync.WaitGroup` (~30 lines)
3. Result collection and output merging (~20 lines)

This is a small implementation task, not a design or infrastructure gap.

---

*This document tracks architectural debt. Fixes should be prioritized based on capability layer development needs.*
