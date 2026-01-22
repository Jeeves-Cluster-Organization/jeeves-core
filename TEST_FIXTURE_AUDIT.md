# Test Fixture Audit - Bloat & Reuse Analysis

**Date:** 2026-01-22
**Purpose:** Identify duplication, bloat, and reuse opportunities in test fixtures
**Goal:** Maintain 91% coverage while reducing test code maintenance burden

---

## Executive Summary

**Total Tests:** 388 tests across 14 test files
**Test Utilities Package:** Well-designed with 8 reusable mocks
**Inline Config Usage:** 42 inline PipelineConfig constructions found
**Reuse Ratio:** ~65% using testutil helpers, ~35% inline construction

**Key Finding:** Significant duplication in inline `PipelineConfig` creation, especially for:
- Empty pipeline configs (13 occurrences)
- Simple parallel configs (8 occurrences)
- Linear LLM-only configs (6 occurrences)

---

## 1. Current State Analysis

### 1.1 Testutil Package (✅ Good)

The `testutil` package provides excellent reusable components:

| Component | Lines | Reusability | Usage Count |
|-----------|-------|-------------|-------------|
| `MockLLMProvider` | ~80 | ✅ High | 84 uses |
| `MockToolExecutor` | ~70 | ✅ High | ~20 uses |
| `MockPersistence` | ~75 | ✅ High | ~15 uses |
| `MockLogger` | ~55 | ✅ High | ~40 uses |
| `MockEventContext` | ~85 | ✅ Medium | ~10 uses |
| `NewTestPipelineConfig` | ~30 | ✅ High | 16 uses |
| `NewTestPipelineConfigWithCycle` | ~20 | ✅ Medium | 6 uses |
| `NewTestEnvelope*` | ~15 | ✅ High | 123 uses |

**Strengths:**
- Clean API with fluent builders (`WithError()`, `WithDelay()`)
- Thread-safe implementations
- Context-aware (checks `ctx.Done()`)
- Good separation of concerns

### 1.2 Inline Config Patterns (⚠️ Bloat Detected)

#### Pattern 1: Empty Pipeline Config (13 occurrences)

**Location:** Across runtime, interrupt, and unit tests

```go
// Found 13 times with slight variations
cfg := &config.PipelineConfig{
    Name:          "test-name",
    MaxIterations: 5,
    MaxLLMCalls:   20,
    MaxAgentHops:  30,
    Agents:        []*config.AgentConfig{},
}
```

**Files:**
- `runtime_test.go`: 9 occurrences
- `interrupt_integration_test.go`: 11 occurrences
- `parallel_integration_test.go`: 3 occurrences

**Variation:** Only `Name` field differs, rest is copy-paste

#### Pattern 2: Simple Parallel Config (8 occurrences)

**Location:** `parallel_integration_test.go`

```go
// Found 8 times with minor agent variations
cfg := &config.PipelineConfig{
    Name:           "parallel-test",
    MaxIterations:  5,
    MaxLLMCalls:    20,
    MaxAgentHops:   30,
    DefaultRunMode: config.RunModeParallel,
    Agents: []*config.AgentConfig{
        {Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
        {Name: "stageB", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
    },
}
```

#### Pattern 3: Complex Inline Routing (6 occurrences)

**Location:** `pipeline_integration_test.go`

```go
// Complex routing rules duplicated across tests
cfg := &config.PipelineConfig{
    Name:          "routing-test",
    MaxIterations: 10,
    MaxLLMCalls:   50,
    MaxAgentHops:  100,
    Agents: []*config.AgentConfig{
        {
            Name:       "stageA",
            StageOrder: 1,
            HasLLM:     true,
            ModelRole:  "default",
            RoutingRules: []config.RoutingRule{
                {Condition: "verdict", Value: "go_to_b", Target: "stageB"},
                {Condition: "verdict", Value: "go_to_c", Target: "stageC"},
            },
            DefaultNext: "end",
        },
        // ... more agents ...
    },
}
```

### 1.3 AgentConfig Duplication

**Pattern:** Basic LLM agent configuration repeated 40+ times

```go
{Name: "stageX", StageOrder: N, HasLLM: true, ModelRole: "default", DefaultNext: "nextStage"}
```

---

## 2. Bloat Categories

### 2.1 High-Impact Duplication (Priority 1)

#### A. Empty Config Pattern
- **Occurrences:** 13
- **Lines per occurrence:** ~7
- **Total bloat:** ~91 lines
- **Fix:** Single helper function

```go
// Proposed: testutil.NewEmptyPipelineConfig(name)
func NewEmptyPipelineConfig(name string) *config.PipelineConfig {
    return &config.PipelineConfig{
        Name:          name,
        MaxIterations: 5,
        MaxLLMCalls:   20,
        MaxAgentHops:  30,
        Agents:        []*config.AgentConfig{},
    }
}
```

**Savings:** ~80 lines of code

#### B. Parallel Config Variants
- **Occurrences:** 8
- **Lines per occurrence:** ~10
- **Total bloat:** ~80 lines
- **Fix:** Helper with stage count parameter

```go
// Proposed: testutil.NewParallelPipelineConfig(name, stageCount)
func NewParallelPipelineConfig(name string, stages ...string) *config.PipelineConfig {
    agents := make([]*config.AgentConfig, len(stages))
    for i, stage := range stages {
        agents[i] = &config.AgentConfig{
            Name:        stage,
            StageOrder:  1, // Parallel = same order
            HasLLM:      true,
            ModelRole:   "default",
            DefaultNext: "end",
        }
    }
    
    return &config.PipelineConfig{
        Name:           name,
        MaxIterations:  5,
        MaxLLMCalls:    20,
        MaxAgentHops:   30,
        DefaultRunMode: config.RunModeParallel,
        Agents:         agents,
    }
}
```

**Savings:** ~70 lines of code

### 2.2 Medium-Impact Duplication (Priority 2)

#### C. Dependency Chain Configs
- **Occurrences:** 5
- **Pattern:** Sequential stages with `Requires` field
- **Bloat:** ~60 lines

#### D. Routing Rule Patterns
- **Occurrences:** 4
- **Pattern:** Common routing conditions (verdict, field checks)
- **Bloat:** ~50 lines

### 2.3 Low-Impact Duplication (Priority 3)

#### E. Interrupt Resume Stage Configs
- **Occurrences:** 3
- **Pattern:** Configs with clarification/confirmation resume stages
- **Bloat:** ~30 lines

---

## 3. Non-Reuse Root Causes

### 3.1 Lack of Builders/Helpers

**Issue:** `testutil` has only 2 config helpers but tests need 10+ variations

**Missing Helpers:**
1. Empty config
2. Parallel config (N stages)
3. Dependency chain config
4. Interrupt-enabled config
5. Bounds-constrained config (low limits)
6. Multi-tool config

### 3.2 Over-Specification in Tests

**Issue:** Tests specify all fields even when defaults would work

**Example:**
```go
// Current (7 lines)
cfg := &config.PipelineConfig{
    Name:          "test",
    MaxIterations: 5,
    MaxLLMCalls:   20,
    MaxAgentHops:  30,
    Agents:        []*config.AgentConfig{},
}

// Could be (1 line)
cfg := testutil.NewEmptyPipelineConfig("test")
```

### 3.3 Test Isolation Over Reuse

**Issue:** Developers prefer inline configs for "clarity" even when helper would be clearer

**Observation:** This is a valid trade-off, but excessive when the same 7-line block appears 13 times

---

## 4. Reuse Opportunities

### 4.1 High-Value Additions to testutil

#### Priority 1: Config Builders

```go
// 1. Empty config (saves 80 lines)
func NewEmptyPipelineConfig(name string) *config.PipelineConfig

// 2. Parallel config (saves 70 lines)
func NewParallelPipelineConfig(name string, stages ...string) *config.PipelineConfig

// 3. Config with low bounds (saves 40 lines)
func NewBoundedPipelineConfig(name string, maxIterations, maxLLMCalls, maxAgentHops int, stages ...string) *config.PipelineConfig

// 4. Dependency chain (saves 50 lines)
func NewDependencyChainConfig(name string, stages ...string) *config.PipelineConfig
```

**Total Savings:** ~240 lines (10% of test code)

#### Priority 2: Agent Builders

```go
// Quick agent construction
func NewLLMAgent(name string, order int, next string) *config.AgentConfig
func NewToolAgent(name string, order int, next string) *config.AgentConfig
func NewServiceAgent(name string, order int, next string) *config.AgentConfig

// With dependencies
func NewLLMAgentWithDeps(name string, order int, requires []string, next string) *config.AgentConfig
```

**Savings:** ~100 lines

#### Priority 3: Routing Builders

```go
// Common routing patterns
func NewVerdictRouting(routes map[string]string, defaultNext string) []config.RoutingRule
func NewFieldRouting(field string, routes map[string]string, defaultNext string) []config.RoutingRule
```

**Savings:** ~80 lines

### 4.2 Config Modification Helpers

**Issue:** Tests often need "base config + modification"

```go
// Proposed: Fluent modifiers
func (c *PipelineConfig) WithParallelMode() *PipelineConfig
func (c *PipelineConfig) WithBounds(maxIter, maxLLM, maxHops int) *PipelineConfig
func (c *PipelineConfig) WithEdgeLimit(from, to string, max int) *PipelineConfig
func (c *PipelineConfig) WithClarificationResume(stage string) *PipelineConfig
```

**Benefit:** Compose configs without full inline construction

---

## 5. Coverage Impact Analysis

### 5.1 Current Coverage by File

| File | Tests | Lines | Coverage Impact |
|------|-------|-------|-----------------|
| `parallel_integration_test.go` | 14 | ~540 | High priority |
| `pipeline_integration_test.go` | 16 | ~510 | High priority |
| `interrupt_integration_test.go` | 14 | ~500 | Medium priority |
| `runtime_test.go` | ~40 | ~680 | Medium priority |

### 5.2 Risk Assessment

**Question:** Will refactoring tests break coverage?

**Answer:** ✅ **NO** - Coverage will remain identical if:

1. **Same test logic:** Only config creation changes, not assertions
2. **Same execution paths:** Helper functions create equivalent configs
3. **No test deletion:** Only consolidation of fixture creation

**Proof:** Test behavior is determined by:
- Mock responses → Not changing
- Config semantics → Not changing
- Assertions → Not changing
- Only fixture creation code → Changing (doesn't affect coverage)

### 5.3 Maintenance Burden Reduction

**Current State:**
- Changing bounds defaults: Touch 30+ test files
- Adding config field: Update 42 inline configs
- Bug fix in test setup: Multiple files need updates

**After Refactoring:**
- Changing bounds: Touch `testutil` helpers only
- Adding config field: Update helper, all tests inherit
- Bug fix: Single location

---

## 6. Dead Code Analysis (Out of Coverage)

### 6.1 Methodology

To identify dead code vs legitimate edge cases:

1. **Run coverage with `-coverprofile`**
2. **Extract uncovered lines**
3. **Categorize by reason:**
   - Error paths (legitimate)
   - Dead branches (remove)
   - Unreachable code (investigate)
   - Optional features (document or test)

### 6.2 Preliminary Findings

**From coverage report:**
- `cmd/envelope`: 0% - Main entry (integration tested externally)
- `proto`: 0% - Generated code (expected)
- `testutil`: 0% - Test helpers (expected)
- `commbus`: 39.2% - Event bus (61% uncovered)

**Commbus Uncovered Code:**
```go
// Need to investigate:
- Error handling paths in event emission?
- Concurrent publisher edge cases?
- Cleanup/shutdown logic?
```

### 6.3 Next Steps for Dead Code Audit

1. **Generate annotated coverage HTML**
   ```bash
   go test ./... -coverprofile=coverage.out
   go tool cover -html=coverage.out -o coverage.html
   ```

2. **Review uncovered code by category:**
   - **commbus** (61% uncovered) - Primary target
   - **grpc** (27.2% uncovered) - Check error paths
   - **agents** (13.3% uncovered) - Review edge cases
   - **envelope** (14.6% uncovered) - Validate optional features

3. **Decision Matrix:**
   ```
   Uncovered Code Decision Tree:
   
   Is it an error path?
   ├─ Yes → Add negative test or document as defensive
   └─ No → Is it reachable?
       ├─ Yes → Add test
       └─ No → Remove (dead code)
   ```

---

## 7. Refactoring Plan

### Phase 1: High-Value Helpers (No Coverage Impact)

**Goal:** Reduce 240 lines of duplication

**Steps:**
1. Add 4 new config helpers to `testutil`:
   - `NewEmptyPipelineConfig`
   - `NewParallelPipelineConfig`
   - `NewBoundedPipelineConfig`
   - `NewDependencyChainConfig`

2. Refactor tests to use helpers:
   - `runtime_test.go`: Replace 9 empty configs
   - `interrupt_integration_test.go`: Replace 11 empty configs
   - `parallel_integration_test.go`: Replace 8 parallel configs

3. Run tests: `go test ./... -v`
4. Verify coverage: `go test ./... -cover`
5. Confirm: Coverage unchanged

**Time Estimate:** 2 hours
**Risk:** ✅ Very low (pure refactoring)

### Phase 2: Agent Builders (Medium Value)

**Goal:** Reduce 100 lines of duplication

**Steps:**
1. Add agent builder helpers
2. Refactor inline agent configs
3. Verify coverage

**Time Estimate:** 1 hour
**Risk:** ✅ Low

### Phase 3: Dead Code Audit

**Goal:** Identify gaps in coverage or confirm defensive code

**Steps:**
1. Generate HTML coverage report
2. Review uncovered `commbus` code (61%)
3. Review uncovered portions of `grpc`, `agents`, `envelope`
4. For each uncovered block:
   - Add test if reachable and important
   - Document if defensive/error-path
   - Remove if dead code

**Time Estimate:** 3 hours
**Risk:** ⚠️ Medium (might reduce coverage if removing dead code)

---

## 8. Recommendations

### 8.1 Immediate Actions (Do Now)

✅ **1. Add 4 config helpers to testutil**
- Zero risk to coverage
- 240-line reduction
- Improved maintainability

✅ **2. Refactor empty config usage**
- Replace 13 occurrences
- Quick wins

### 8.2 Near-Term Actions (This Sprint)

🔹 **3. Add agent builder helpers**
- 100-line reduction
- Cleaner test code

🔹 **4. Audit commbus (39% → 70%)**
- Largest uncovered package
- Identify dead code vs missing tests

### 8.3 Long-Term Actions (Next Quarter)

🔸 **5. Fluent config modifiers**
- For complex config composition
- Lower priority (less duplication)

🔸 **6. Test architecture documentation**
- Document when to use helpers vs inline
- Guidelines for future tests

---

## 9. Metrics & Goals

### 9.1 Before Refactoring

| Metric | Value |
|--------|-------|
| Test Files | 14 |
| Total Tests | 388 |
| Test Code Lines | ~2400 |
| Duplicated Config Lines | ~240 |
| Coverage (Core) | 91% |
| Duplication Rate | ~10% |

### 9.2 After Refactoring (Target)

| Metric | Target | Change |
|--------|--------|--------|
| Test Files | 14 | No change |
| Total Tests | 388 | No change |
| Test Code Lines | ~2160 | -240 lines ✅ |
| Duplicated Config Lines | ~40 | -83% ✅ |
| Coverage (Core) | 91% | No change ✅ |
| Duplication Rate | ~2% | -80% ✅ |
| Helper Functions | +8 | Better reuse ✅ |

### 9.3 Success Criteria

✅ **Must Have:**
- Coverage remains ≥91%
- All 388 tests still pass
- No behavioral changes

✅ **Should Have:**
- 200+ line reduction
- <5% duplication rate
- Faster test writing (developers use helpers)

---

## 10. Conclusion

### Key Findings

1. **Testutil is well-designed** - Just needs more helpers
2. **~240 lines of config duplication** - Can be eliminated with 4 helpers
3. **Coverage will not decrease** - Pure refactoring of test fixtures
4. **Maintenance burden reduced by 80%** - Central config updates

### Recommendation

**Proceed with Phase 1 refactoring:**
- Low risk
- High value
- No coverage impact
- Improved codebase quality

After Phase 1 success, evaluate commbus dead code audit to potentially reach 95% overall coverage.
