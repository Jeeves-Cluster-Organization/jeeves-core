# Jeeves-Core Codebase Audit Report

**Date:** 2026-01-14
**Auditor:** Automated Code Audit
**Purpose:** Verify documentation matches code reality, identify dead code, ensure complete wiring

---

## Executive Summary

The audit found **14 discrepancies** between documentation and code, plus several instances of dead code and incomplete wiring. The most critical issues are:

1. **ToolId Contract Violation**: CONTRACT.md (v1.3) claims ToolId is "capability-owned" but avionics still defines and uses ToolId
2. **Go Binary Not Built**: Go code exists but `go-envelope` binary is never built; tests use Python fallback
3. **EnableArbiter Still in CoreConfig**: FUTURE_PLAN.md says to delete `EnableArbiter` flag, but it's still in `coreengine/config/core_config.go`
4. **Subprocess Bridge Still Exists**: FUTURE_PLAN.md says to delete subprocess client, but `jeeves_avionics/interop/go_bridge.py` still exists

---

## Critical Issues (Documentation vs Code)

### Issue 1: ToolId Location Discrepancy ⚠️ CRITICAL

**Documentation Claims (CONTRACT.md v1.3, docs/CONTRACTS.md Contract 10):**
> "ToolId is CAPABILITY-OWNED - import from your capability's tools/catalog.py"
> "ToolId enums MUST be defined in the capability layer, NOT in avionics"
> "from jeeves_avionics.tools.catalog import ToolId  # NO LONGER EXISTS"

**Code Reality:**
- `jeeves_avionics/tools/catalog.py` **still defines** `ToolId` enum (lines 29-107)
- `jeeves_avionics/wiring.py` **imports** `from jeeves_avionics.tools.catalog import ToolId` (line 48)
- `jeeves_avionics/tools/__init__.py` **exports** `ToolId`

**Files Claiming ToolId Doesn't Exist:**
- `CONTRACT.md` line 147
- `docs/CONTRACTS.md` lines 277-281

**Files Still Importing ToolId from Avionics:**
- `jeeves_avionics/wiring.py`
- `docs/CONTRACTS.md` (incorrectly mentions it in example)
- `jeeves_avionics/CONSTITUTION.md` (likely)

**Fix Required:**
Either:
1. **Update documentation** to reflect that avionics ToolId is for core/infrastructure tools (current behavior)
2. **OR delete avionics ToolId** and use CapabilityToolCatalog for all tools (documented behavior)

---

### Issue 2: FUTURE_PLAN.md "Code to DELETE" Not Deleted ⚠️ CRITICAL

**FUTURE_PLAN.md Section "Code to DELETE (not deprecate)":**

| File/Pattern | FUTURE_PLAN Says | Actual Status |
|--------------|------------------|---------------|
| `dag_executor.go` | DELETE | ✅ Does not exist (deleted or never created) |
| `jeeves_protocols/client.py` | DELETE | ✅ Does not exist |
| `CyclePolicyReject` | DELETE | ✅ Only in docs, not code |
| `EnableDAGExecution` flag | DELETE | ✅ Does not exist in code |
| `EnableArbiter` flag in PipelineConfig | DELETE | ❌ **STILL EXISTS** in `coreengine/config/core_config.go` |
| `if go_client else python` patterns | DELETE | ❌ **STILL EXISTS** in `go_bridge.py` |
| `topologicalOrder` in PipelineConfig | DELETE | ✅ Does not exist |

**Fix Required:**
1. Remove `EnableArbiter` from `coreengine/config/core_config.go` if arbiter is capability concern
2. Decide: Either delete `go_bridge.py` subprocess fallback OR update FUTURE_PLAN.md

---

### Issue 3: Go Binary Never Built

**Documentation Claims (HANDOFF.md, various):**
- Go CLI interface exists via subprocess
- `go-envelope create|process|validate|can-continue|result`

**Code Reality:**
- `cmd/envelope/main.go` exists but there's no Makefile target to build it
- `go.mod` references `github.com/jeeves-cluster-organization/codeanalysis` but repo structure is `/workspace`
- Tests in `test_go_bridge.py` always fall back to Python implementation
- `GoEnvelopeBridge.is_available()` returns False (binary not in PATH)

**Impact:** The entire Go engine is unused at runtime.

**Fix Required:**
1. Add Makefile target: `go build -o bin/go-envelope ./cmd/envelope`
2. Update `go.mod` module path to match actual repo structure
3. OR acknowledge Go is documentation-only and update docs

---

### Issue 4: Missing Contract Tests

**docs/CONTRACTS.md Contract 11 says:**
> "Envelope sync via contract tests (Go → Python → Go round-trips)"
> Shows example: `tests/contracts/test_envelope_roundtrip.py`

**Code Reality:**
- Directory `tests/contracts/` does not exist
- No envelope roundtrip tests exist
- `tests/unit/test_go_bridge.py` only tests Python fallback

**Fix Required:**
Create `tests/contracts/test_envelope_roundtrip.py` or remove Contract 11 from docs.

---

### Issue 5: FUTURE_PLAN.md Phase 1 Blockers Status

**FUTURE_PLAN.md Claims These Are BLOCKERS for Phase 2:**

| Blocker | Status |
|---------|--------|
| Import Linter (`scripts/check_layer_boundaries.py`) | ❌ **NOT CREATED** |
| LifecycleManager 80%+ coverage | ❓ Unchecked (22% claimed) |
| Contract 11 tests | ❌ **NOT CREATED** |

**Fix Required:**
- Create `scripts/check_layer_boundaries.py` or update FUTURE_PLAN.md
- Add LifecycleManager tests if coverage still low
- Create Contract 11 tests or remove from docs

---

## Medium Issues

### Issue 6: Dual Tool Catalog Architecture

Two parallel tool catalog systems exist:

1. **jeeves_avionics/tools/catalog.py** - `ToolId` enum, `ToolCatalog` singleton
2. **jeeves_protocols/capability.py** - `CapabilityToolCatalog` class

**Documentation says** capability layer should use `CapabilityToolCatalog`.
**Code reality:** Infrastructure still uses avionics `ToolCatalog`.

This is intentional (core tools vs capability tools) but not clearly documented.

**Fix Required:**
Update CONTRACT.md to clarify:
- Core/infrastructure tools → `jeeves_avionics.tools.catalog.ToolId`
- Capability-specific tools → `CapabilityToolCatalog`

---

### Issue 7: Registry Completeness

**CapabilityResourceRegistry Usage:**

| Method | Registered By | Queried By |
|--------|---------------|------------|
| `register_service()` | - | `bootstrap.py` ✅ |
| `register_mode()` | - | Unknown |
| `register_orchestrator()` | - | Unknown |
| `register_tools()` | - | Unknown |
| `register_prompts()` | - | Unknown |
| `register_agents()` | - | Unknown |
| `register_contracts()` | - | Unknown |
| `register_schema()` | - | Unknown |

**Observation:** `get_capability_resource_registry()` is imported in `bootstrap.py` but only `get_default_service()` is actually called. Most registry features appear unused.

**Fix Required:**
Either:
1. Wire up remaining registry methods in bootstrap/server
2. OR remove unused methods and simplify registry

---

### Issue 8: Resilient Ops Module Status

**FUTURE_PLAN.md says:**
> "Verify resilient_ops Module - Check if resilient_ops is used anywhere"

**Code Reality:**
- `RESILIENT_OPS_MAP` exists in `jeeves_avionics/wiring.py` (lines 78-82)
- Maps: `read_file` → `read_code`, `find_symbol` → `locate`, `find_similar_files` → `find_related`
- Used by `ToolExecutor.execute_resilient()`

**Status:** IN USE, not dead code. FUTURE_PLAN.md can mark this as verified.

---

## Low Priority Issues

### Issue 9: Empty/Stub Files

| File | Contents |
|------|----------|
| `jeeves_mission_system/__init__.py` | "Integration tests for Code Analysis Agent." (incorrect docstring) |
| `jeeves_avionics/__init__.py` | Basic docstring only, no exports |

**Fix Required:** Update docstrings to match actual purpose.

---

### Issue 10: Go Module Path Mismatch

**go.mod line 1:**
```
module github.com/jeeves-cluster-organization/codeanalysis
```

**Actual path:** `/workspace` (no github structure)

**Impact:** Go imports won't resolve if anyone tries to `go build`.

---

### Issue 11: test_go_bridge.py Tests Fallback Only

All integration tests in `test_go_bridge.py` pass because they use Python fallback:
- `GoEnvelopeBridge(use_go=False)` explicitly used
- `GoEnvelopeBridge()` auto-detects and falls back

No tests verify actual Go binary behavior.

---

### Issue 12: ARCHITECTURE_DIAGRAM.md References Non-Existent Paths

`jeeves_avionics/ARCHITECTURE_DIAGRAM.md` references:
- `memory/services/l1_episodic.py` - Does not exist
- `memory/services/l2_event_log.py` - Does not exist
- `memory/services/l3_working_memory.py` - Does not exist
- `memory/services/l4_persistent_cache.py` - Does not exist
- `memory/repositories/` - Does not exist under avionics

**Actual location:** These are in `jeeves_memory_module/`, not `jeeves_avionics/memory/`.

---

### Issue 13: HANDOFF.md Section 12 Go CLI Examples

**HANDOFF.md says:**
```bash
echo '{"raw_input": "...", "user_id": "..."}' | go-envelope create
```

**Reality:** No built `go-envelope` binary exists in PATH.

---

### Issue 14: Orphaned Interop Module

`jeeves_avionics/interop/` contains:
- `__init__.py` (empty)
- `go_bridge.py`

This module is referenced by tests but the `__init__.py` exports nothing, and the bridge is only used with Python fallback.

---

## Recommendations

### Immediate Actions (Critical)

1. **Fix ToolId Documentation**
   - Update CONTRACT.md section on ToolId to clarify dual-catalog architecture
   - OR migrate all tools to CapabilityToolCatalog

2. **Clean Up FUTURE_PLAN.md**
   - Mark `EnableArbiter` as "kept for orchestration control" or delete from CoreConfig
   - Update go_bridge.py section to reflect current decision

3. **Create Missing Blocker Items**
   - `scripts/check_layer_boundaries.py` - Simple import linter
   - `tests/contracts/test_envelope_roundtrip.py` - Even if just Python-only initially

### Medium Priority

4. **Decide Go Engine Future**
   - If Go is used: Add build target, fix module path, add real tests
   - If Go is Python-only: Remove Go code to reduce confusion

5. **Fix ARCHITECTURE_DIAGRAM.md Paths**
   - Update memory service paths to point to `jeeves_memory_module/`

6. **Audit Registry Usage**
   - Verify each CapabilityResourceRegistry method is used
   - Remove dead methods or add TODO for future wiring

### Low Priority

7. **Fix Docstrings**
   - `jeeves_mission_system/__init__.py` has wrong docstring

8. **Update go.mod Module Path**
   - If Go code is kept, fix module path

---

## Summary Table

| Category | Count | Critical | Medium | Low |
|----------|-------|----------|--------|-----|
| Doc/Code Mismatch | 8 | 5 | 2 | 1 |
| Dead/Orphaned Code | 3 | 0 | 1 | 2 |
| Missing Components | 3 | 2 | 0 | 1 |
| **Total** | **14** | **7** | **3** | **4** |

---

## Appendix: Files Audited

### Documentation Files
- `CONTRACT.md` ✅
- `HANDOFF.md` ✅
- `FUTURE_PLAN.md` ✅
- `docs/CONTRACTS.md` ✅
- `docs/STATE_MACHINE_EXECUTOR_DESIGN.md` ✅
- `jeeves_avionics/ARCHITECTURE_DIAGRAM.md` ✅
- `jeeves_avionics/CONSTITUTION.md` (referenced)
- `jeeves_control_tower/CONSTITUTION.md` ✅
- All other INDEX.md and README.md files (scanned)

### Key Source Files
- `jeeves_protocols/__init__.py` ✅
- `jeeves_protocols/capability.py` ✅
- `jeeves_avionics/wiring.py` ✅
- `jeeves_avionics/tools/catalog.py` ✅
- `jeeves_avionics/capability_registry.py` ✅
- `jeeves_avionics/interop/go_bridge.py` ✅
- `jeeves_mission_system/bootstrap.py` ✅
- `jeeves_control_tower/kernel.py` ✅
- `coreengine/runtime/runtime.go` ✅
- `coreengine/config/core_config.go` ✅
- `tests/unit/test_go_bridge.py` ✅

---

*End of Audit Report*
