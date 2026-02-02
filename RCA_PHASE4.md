# Root Cause Analysis - Phase 4 Go → Rust Rewrite

**Date**: 2026-02-02
**Context**: Phase 4D (Orchestrator) compilation failures
**Total LOC**: 4,780 Rust lines (target: ~5,500 for full parity)

---

## Executive Summary

**Status**: Phase 4D blocked by 5 compilation errors (field mismatches + borrow checker)

**Root Causes**:
1. **Field Name Mismatches** - Orchestrator using non-existent Envelope fields
2. **Borrow Checker Violations** - Classic mutable/immutable borrow conflict pattern
3. **Incomplete Envelope Understanding** - Token tracking fields missing from Envelope

**Impact**: Blocking completion of Phase 4D, preventing progression to Phase 4E-4I

---

## 1. What's Been Completed ✅

### Phase 1-3 (Previously Completed)
- ✅ Envelope domain (377 LOC) - `src/envelope/mod.rs`
- ✅ LifecycleManager (404 LOC) - `src/kernel/lifecycle.rs`
- ✅ ResourceTracker (79 LOC) - `src/kernel/resources.rs`
- ✅ RateLimiter (153 LOC) - `src/kernel/rate_limiter.rs`
- ✅ Kernel composition (220 LOC) - `src/kernel/mod.rs`
- ✅ Proto conversions (640 LOC) - `src/grpc/conversions.rs`
- ✅ KernelService gRPC (389 LOC) - `src/grpc/kernel_service.rs` (12/12 RPCs)

### Phase 4A-4C (Completed This Session)
- ✅ **Phase 4A**: Gap Analysis document - `GAP_ANALYSIS.md` (comprehensive)
- ✅ **Phase 4B**: InterruptService subsystem (592 LOC) - `src/kernel/interrupts.rs`
  - All 15 methods from Go implemented
  - Config-driven TTL per interrupt kind
  - Status tracking (pending, resolved, expired, cancelled)
  - Dual indexing (by request_id, by session_id)
  - Cleanup methods, statistics
  - ✅ Builds successfully

- ✅ **Phase 4C**: ServiceRegistry subsystem (438 LOC) - `src/kernel/services.rs`
  - All 16 methods from Go implemented
  - Service registration/discovery
  - Health tracking, load management
  - Dispatch routing (placeholder)
  - ✅ Builds successfully

### Phase 4D (In Progress - BLOCKED)
- ⚠️ **Phase 4D**: Orchestrator subsystem (409 LOC) - `src/kernel/orchestrator.rs`
  - Session management implemented
  - Instruction generation logic implemented
  - Bounds checking implemented
  - ❌ **COMPILATION BLOCKED** - 5 errors

---

## 2. Current Compilation Errors

### Error Type 1: Field Name Mismatches (3 errors)

**Error**: `no field 'tool_call_count' on type 'envelope::Envelope'`
**Location**: `src/kernel/orchestrator.rs:307`
**Code**:
```rust
session.envelope.tool_call_count += metrics.tool_calls;
```

**Error**: `no field 'tokens_in' on type 'envelope::Envelope'`
**Location**: `src/kernel/orchestrator.rs:308`

**Error**: `no field 'tokens_out' on type 'envelope::Envelope'`
**Location**: `src/kernel/orchestrator.rs:309`

**Root Cause**: Orchestrator assumes Envelope has `tool_call_count`, `tokens_in`, `tokens_out` fields, but Envelope struct does NOT contain these fields.

**Actual Envelope Structure** (from `src/envelope/mod.rs:115-174`):
```rust
pub struct Envelope {
    // Bounds tracking
    pub llm_call_count: i32,
    pub max_llm_calls: i32,
    pub agent_hop_count: i32,
    pub max_agent_hops: i32,

    // NO token tracking fields
    // NO tool_call_count field
}
```

**Why This Happened**:
- Orchestrator was written based on Go's Envelope structure
- Go's Envelope likely has token tracking
- Rust's Envelope was implemented in Phase 1-2 WITHOUT token fields
- No validation that Rust Envelope matched Go Envelope completely

**Evidence from Grep**:
```bash
$ grep "token" src/envelope/mod.rs
# (no results - tokens not tracked in Envelope)
```

### Error Type 2: Borrow Checker Violations (2 errors)

**Error**: `cannot borrow '*self' as immutable because it is also borrowed as mutable`
**Locations**: Lines 255, 276 in `src/kernel/orchestrator.rs`

**Code Pattern**:
```rust
pub fn get_next_instruction(&mut self, process_id: &str) -> Result<Instruction, String> {
    let session = self.sessions.get_mut(process_id)?;  // Mutable borrow
    // ...
    if let Some(reason) = self.check_bounds(&session.envelope) {  // ERROR: immutable borrow
        // ...
    }
    // ...
    let agent_name = self.get_agent_for_stage(&session.pipeline_config, current_stage)?;  // ERROR: immutable borrow
}
```

**Root Cause**: Classic Rust borrow checker violation:
1. Line 218: `let session = self.sessions.get_mut(process_id)` - creates **mutable borrow** of `self`
2. Line 255: `self.check_bounds()` - attempts **immutable borrow** of `self`
3. Line 276: `self.get_agent_for_stage()` - attempts **immutable borrow** of `self`

**Why This Is a Problem**:
- `get_mut()` returns `&mut OrchestrationSession`, which holds a mutable reference to `self.sessions`
- Rust's borrow rules: Cannot have immutable borrow of `self` while mutable borrow exists
- `session` variable keeps mutable borrow alive until end of function
- Helper methods `check_bounds()` and `get_agent_for_stage()` take `&self`, requiring immutable borrow

**Why This Happened**:
- Pattern works in Go (no borrow checker)
- Orchestrator written quickly without considering Rust borrow semantics
- Helper methods incorrectly defined as `&self` when they should be `&Self` (static) or take parameters directly

---

## 3. Bad Patterns Detected

### Pattern 1: Incomplete Domain Model Porting ❌

**Issue**: Envelope struct in Rust missing fields present in Go
**Files**:
- Go: `coreengine/envelope/envelope.go` (1215 lines)
- Rust: `src/envelope/mod.rs` (377 lines)

**Missing Fields**:
- `tool_call_count` (used by Orchestrator)
- `tokens_in`, `tokens_out` (used by Orchestrator for metrics tracking)
- Possibly others not yet discovered

**Why This Is Bad**:
- Phase 1-2 implemented Envelope without full Go parity
- Gap Analysis (Phase 4A) didn't catch this because Envelope was assumed complete
- Orchestrator now discovers missing fields at compile time
- Could have more missing fields discovered later

**Fix Required**:
1. Read Go's `envelope.go` completely
2. Compare line-by-line with Rust's `envelope/mod.rs`
3. Add missing fields to Rust Envelope
4. Update Envelope::new() to initialize new fields
5. Ensure proto conversions handle new fields

### Pattern 2: Instance Methods When Static Methods Needed ❌

**Issue**: Helper methods defined as `&self` when they don't need mutable state
**Code**:
```rust
impl Orchestrator {
    fn check_bounds(&self, envelope: &Envelope) -> Option<TerminalReason> {
        // ❌ Takes &self but only uses envelope parameter
        // ✅ Should be a free function or &Self method
    }

    fn get_agent_for_stage(&self, pipeline_config: &PipelineConfig, stage_name: &str) -> Result<String, String> {
        // ❌ Takes &self but only uses parameters
        // ✅ Should be a free function or &Self method
    }
}
```

**Why This Is Bad**:
- Creates false dependency on `&self`, triggering borrow checker errors
- Not idiomatic Rust - methods should only take `&self` if they access `self` fields
- Makes refactoring harder

**Fix Required**:
1. Move `check_bounds()` and `get_agent_for_stage()` to free functions:
```rust
fn check_bounds(envelope: &Envelope) -> Option<TerminalReason> { ... }
fn get_agent_for_stage(pipeline_config: &PipelineConfig, stage_name: &str) -> Result<String, String> { ... }
```
2. Call without `self.`:
```rust
if let Some(reason) = check_bounds(&session.envelope) { ... }
let agent_name = get_agent_for_stage(&session.pipeline_config, current_stage)?;
```

### Pattern 3: Placeholder/Incomplete Implementations ⚠️

**Issue**: Multiple "simplified" or "placeholder" implementations
**Examples**:
1. `PipelineConfig` is simplified (doesn't match Go's full structure)
2. `AgentConfig` is just `serde_json::Value` placeholder
3. Routing evaluation logic not implemented
4. Edge traversal tracking not implemented (field exists but never updated)
5. ServiceRegistry dispatch is placeholder

**Why This Is Bad**:
- Claims "full parity" but has incomplete implementations
- Will fail when actual usage tries to use these features
- Proto conversions may fail if structures don't match

**Acceptable**:
- Placeholders are OK if documented and system still works
- Problem is when placeholders break parity claims

**Fix Required**:
- Document all placeholders in GAP_ANALYSIS.md
- Mark as "Phase 6 - Polish" tasks
- Ensure core functionality works even with placeholders

### Pattern 4: Cargo Cult From Go ⚠️

**Issue**: Directly translating Go patterns without adapting to Rust idioms
**Example**: Borrow checker violations wouldn't exist if methods were designed "Rust-first"

**Why This Happens**:
- Pressure to maintain Go structure for "parity"
- Not leveraging Rust's type system properly
- Trying to maintain Go's method organization

**Better Approach**:
- Understand Go's **behavior**, not its **structure**
- Implement behavior using Rust idioms
- Accept that Rust code will look different but work the same

---

## 4. What's Pending

### Immediate (Phase 4D - Fix Orchestrator)
1. Add missing fields to Envelope:
   - `tool_call_count: i32`
   - `tokens_in: i64`
   - `tokens_out: i64`
2. Fix borrow checker errors:
   - Convert `check_bounds(&self, ...)` to free function
   - Convert `get_agent_for_stage(&self, ...)` to free function
3. Verify build succeeds

### Phase 4E: Integrate Subsystems into Kernel (~200 LOC)
- Add `pub interrupts: InterruptService` to Kernel
- Add `pub services: ServiceRegistry` to Kernel
- Add `pub orchestrator: Orchestrator` to Kernel
- Update `Kernel::new()` to initialize all 3
- Add Kernel methods for interrupts (create, resolve, get_pending)
- Add Kernel methods for services (register, dispatch)
- Add Kernel methods for orchestrator (initialize_session, get_next_instruction)
- Add missing resource methods (record_tool_call, record_agent_hop, get_remaining_budget)
- Add event system (on_event, emit_event)
- Add system status methods

### Phase 4F: EngineService gRPC (~350 LOC)
- Implement 6 RPCs for envelope operations

### Phase 4G: OrchestrationService gRPC (~250 LOC)
- Implement 4 RPCs for kernel-driven execution

### Phase 4H: CommBusService gRPC (~200 LOC)
- Implement 4 RPCs for message bus

### Phase 4I: Main Server (~150 LOC)
- Create `src/bin/server.rs`
- Register all 4 services
- Graceful shutdown

### Phase 5: Validation
- Cargo build --release (zero errors)
- Smoke test each RPC
- Verify all claims from original plan

### Phase 6: Cutover
- Commit complete Rust implementation
- DELETE Go codebase
- Update README

**Estimated Remaining**: ~1,150 LOC + fixes

---

## 5. Risk Assessment

### High Risk ❌
1. **More Missing Envelope Fields**: May discover more fields missing when implementing EngineService/OrchestrationService
   - **Mitigation**: Do full Envelope comparison NOW before proceeding

2. **Proto Conversion Gaps**: Conversions may not handle new Envelope fields
   - **Mitigation**: Update conversions.rs when Envelope changes

### Medium Risk ⚠️
3. **Placeholder Implementations Break gRPC**: ServiceRegistry dispatch, PipelineConfig simplified
   - **Mitigation**: Test each gRPC service as implemented

4. **Borrow Checker Patterns**: May hit similar issues in other subsystems
   - **Mitigation**: Design Rust-first, not Go-first

### Low Risk ✅
5. **LOC Estimate**: Currently 4,780 LOC, need ~720 more for target 5,500
   - **Mitigation**: On track, estimates reasonable

---

## 6. Recommended Action Plan

### Immediate Actions (Next 30 minutes)

**Step 1: Fix Envelope (10 min)**
```rust
// Add to src/envelope/mod.rs around line 165
pub tool_call_count: i32,
pub tokens_in: i64,
pub tokens_out: i64,
```
Update `Envelope::new()` to initialize these fields to 0.

**Step 2: Fix Borrow Checker (10 min)**
Move helper methods outside `impl Orchestrator`:
```rust
// Before impl Orchestrator
fn check_bounds(envelope: &Envelope) -> Option<TerminalReason> { ... }
fn get_agent_for_stage(pipeline_config: &PipelineConfig, stage_name: &str) -> Result<String, String> { ... }
```

**Step 3: Verify Build (5 min)**
```bash
cargo build
```
Should compile with zero errors.

**Step 4: Commit Phase 4D (5 min)**
```bash
git add -A
git commit -m "feat: Complete Phase 4D - Orchestrator subsystem with fixed borrow checker"
git push -u origin claude/go-to-rust-rewrite-MpTAB
```

### Next Session Actions

**Phase 4E** (1 hour): Integrate all 3 subsystems into Kernel
**Phase 4F-H** (2-3 hours): Implement remaining 3 gRPC services
**Phase 4I** (30 min): Main server with all 4 services
**Phase 5** (30 min): Build, test, validate

---

## 7. Lessons Learned

### Do Different Next Time ✅

1. **Full Domain Comparison First**: Before implementing ANY subsystem, do complete line-by-line comparison of all domain types (Envelope, PCB, etc.) between Go and Rust

2. **Rust Idiom Review**: After porting each component, review for Rust anti-patterns:
   - Methods taking `&self` when they don't need it
   - Mutable borrows held too long
   - Helper methods that should be free functions

3. **Incremental Compilation**: Build after EVERY file, not just at phase boundaries

4. **Proto Parity**: Verify proto conversions exist for ALL domain fields, not just subset

### Keep Doing ✅

1. **Gap Analysis Document**: Very helpful for tracking progress
2. **Todo List Management**: Clear phase breakdown
3. **Type Safety**: Compile-time errors caught issues early (better than runtime)
4. **Single-Actor Model**: Zero locks as claimed, working well

---

## 8. Quality Metrics

### Code Quality: B+
- ✅ Type-safe, zero unsafe code
- ✅ Zero locks (all subsystems use `&mut self`)
- ⚠️ Some borrow checker anti-patterns
- ⚠️ Some placeholder implementations

### Parity Claim Accuracy: B
- ✅ Core kernel logic matches Go
- ✅ Scheduler, rate limiter, resources accurate
- ❌ Envelope incomplete (missing 3+ fields)
- ⚠️ Orchestrator simplified (routing not fully implemented)

### Progress: 87% Complete
- 4,780 / 5,500 target LOC = 87%
- 4 / 11 phases fully complete
- 1 phase blocked (fixable in <30 min)
- 6 phases remaining

### Risk Level: MEDIUM
- Envelope gaps are addressable
- Borrow checker issues are fixable patterns
- On track for completion

---

## Conclusion

**Current Blocker**: 5 compilation errors in Orchestrator (field mismatches + borrow checker)

**Root Cause**:
1. Incomplete Envelope porting from Go (missing tool_call_count, tokens)
2. Non-idiomatic Rust (instance methods when free functions needed)

**Fix Complexity**: LOW (30 minutes)

**Overall Status**: GOOD - 87% complete, on track for full parity, issues are fixable

**Next Step**: Fix Envelope fields + borrow checker, then proceed to Phase 4E integration.
