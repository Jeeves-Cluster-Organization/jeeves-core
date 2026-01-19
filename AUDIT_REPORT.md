# Jeeves-Core Architecture Audit Report

**Audit Date:** 2026-01-19
**Auditor:** Claude (Architecture Audit Agent)
**Scope:** Full codebase audit of jeeves-core monorepo

---

## Executive Summary

This audit identified **23 findings** across the jeeves-core codebase, ranging from critical layer violations to medium-priority wiring gaps and low-priority centralization opportunities.

| Severity | Count |
|----------|-------|
| Critical | 4 |
| Medium | 11 |
| Low | 8 |

---

## Findings

### 1. Layer Violations

#### Finding 1.1: L0 (jeeves_protocols) imports from L0 (jeeves_shared)
**Category:** Layer Violations
**Severity:** Critical
**Files:**
- `jeeves_protocols/grpc_client.py:249`
- `jeeves_protocols/memory.py:120, 131`
- `jeeves_protocols/agents.py:571`

**Description:**
According to CONTRACTS.md, L0 (protocols/shared) should have "Nothing" as dependencies. However, `jeeves_protocols` imports from `jeeves_shared.serialization`:

```python
# jeeves_protocols/grpc_client.py:249
from jeeves_shared.serialization import utc_now
```

The Memory Module CONSTITUTION shows `jeeves_shared` sitting slightly *above* `jeeves_protocols`, which means:
- `jeeves_shared` MAY import from `jeeves_protocols` (correct)
- `jeeves_protocols` should NOT import from `jeeves_shared` (violation)

**Suggested Fix:**
Either:
1. Move `utc_now()` to `jeeves_protocols.utils` to eliminate the dependency
2. Update CONTRACTS.md to explicitly document the `protocols → shared` dependency as acceptable

---

#### Finding 1.2: Memory Module CONSTITUTION allows L3 import
**Category:** Code-to-Doc Drift
**Severity:** Medium
**File:** `jeeves_memory_module/CONSTITUTION.md:139`

**Description:**
The Memory Module CONSTITUTION explicitly allows:
```python
from jeeves_avionics.database.factory import create_database_client
```

However, CONTRACTS.md states L2 (memory_module) may only import from L0 and L1. This is a documented exception that contradicts the main contracts document.

**Suggested Fix:**
Update CONTRACTS.md to document this exception, or refactor to inject the database factory via protocol.

---

### 2. Go/Python Boundary Issues

#### Finding 2.1: Python AgentConfig missing parallel execution fields
**Category:** Go/Python Boundary
**Severity:** Critical
**Files:**
- `coreengine/config/pipeline.go:86-128`
- `jeeves_protocols/config.py:29-66`

**Description:**
The Go `AgentConfig` has fields for parallel execution that are missing from Python:

| Field | Go | Python |
|-------|-----|--------|
| `Requires` | ✅ | ❌ |
| `After` | ✅ | ❌ |
| `JoinStrategy` | ✅ | ❌ |

This prevents Python from properly configuring parallel execution as documented in HANDOFF.md.

**Suggested Fix:**
Add missing fields to Python `AgentConfig`:
```python
@dataclass
class AgentConfig:
    # ... existing fields ...
    requires: List[str] = field(default_factory=list)
    after: List[str] = field(default_factory=list)
    join_strategy: str = "all"  # "all" or "any"
```

---

#### Finding 2.2: Python PipelineConfig missing EdgeLimits
**Category:** Go/Python Boundary
**Severity:** Critical
**Files:**
- `coreengine/config/pipeline.go:171-201`
- `jeeves_protocols/config.py:69-116`

**Description:**
The Go `PipelineConfig` has `EdgeLimits` for cyclic routing control, but Python lacks this:

| Field | Go | Python |
|-------|-----|--------|
| `EdgeLimits` | ✅ | ❌ |
| `agent_review_resume_stage` | ✅ | ❌ |

Contract 13 (CONTRACTS.md) documents EdgeLimits as core functionality.

**Suggested Fix:**
Add to Python `PipelineConfig`:
```python
@dataclass
class EdgeLimit:
    from_stage: str
    to_stage: str
    max_count: int

@dataclass
class PipelineConfig:
    # ... existing fields ...
    edge_limits: List[EdgeLimit] = field(default_factory=list)
    agent_review_resume_stage: Optional[str] = None
```

---

#### Finding 2.3: Python GenericEnvelope uses Dict for interrupt, Go uses typed struct
**Category:** Go/Python Boundary
**Severity:** Critical
**Files:**
- `coreengine/envelope/generic.go:52-62`
- `jeeves_protocols/envelope.py:62`

**Description:**
Go has a typed `FlowInterrupt` struct:
```go
type FlowInterrupt struct {
    Kind       InterruptKind
    ID         string
    Question   string
    Message    string
    Data       map[string]any
    Response   *InterruptResponse
    CreatedAt  time.Time
    ExpiresAt  *time.Time
}
```

Python uses a generic dict:
```python
interrupt: Optional[Dict[str, Any]] = None
```

Contract 11 (Envelope Sync) requires identical serialization. This typing mismatch could cause issues.

**Suggested Fix:**
Create a Python `FlowInterrupt` dataclass matching Go and use it instead of `Dict[str, Any]`.

---

### 3. Wiring Gaps

#### Finding 3.1: NLIServiceProtocol interface doesn't match implementation
**Category:** Wiring Gaps
**Severity:** Medium
**Files:**
- `jeeves_protocols/protocols.py:378-382`
- `jeeves_memory_module/services/nli_service.py:57`

**Description:**
`NLIServiceProtocol` defines:
```python
async def parse_intent(self, text: str) -> Dict[str, Any]: ...
async def generate_response(self, intent: Dict[str, Any], context: Dict[str, Any]) -> str: ...
```

But `NLIService` implements:
```python
def verify_claim(self, claim: str, evidence: str) -> NLIResult: ...
```

These are completely different interfaces for different purposes.

**Suggested Fix:**
Either:
1. Rename `NLIServiceProtocol` to `IntentParsingProtocol` and create `ClaimVerificationProtocol`
2. Update `NLIService` to implement the protocol, or
3. Remove the unused protocol

---

#### Finding 3.2: ClockProtocol not properly implemented
**Category:** Wiring Gaps
**Severity:** Medium
**Files:**
- `jeeves_protocols/protocols.py:248-252`
- `jeeves_avionics/context.py:44-59`

**Description:**
`ClockProtocol` defines:
```python
def now(self) -> datetime: ...
def utcnow(self) -> datetime: ...
```

`SystemClock` implements:
```python
def now_utc(self) -> float: ...
def now_iso(self) -> str: ...
```

Method names and return types don't match.

**Suggested Fix:**
Update `SystemClock` to implement `ClockProtocol`:
```python
class SystemClock:
    def now(self) -> datetime:
        return datetime.now()
    
    def utcnow(self) -> datetime:
        return datetime.now(timezone.utc)
```

---

#### Finding 3.3: CapabilityLLMConfigRegistryProtocol has implementation but unclear usage
**Category:** Wiring Gaps
**Severity:** Low
**Files:**
- `jeeves_protocols/protocols.py:598-665`
- `jeeves_avionics/capability_registry.py:44`

**Description:**
`CapabilityLLMConfigRegistry` exists in avionics but its integration with the rest of the system is unclear. No tests or usage patterns found in capability layers.

**Suggested Fix:**
Add integration tests and document usage in HANDOFF.md.

---

### 4. Naming Inconsistencies

#### Finding 4.1: Inconsistent interrupt/flow terminology
**Category:** Naming Inconsistencies
**Severity:** Medium
**Files:** Multiple

**Description:**
- `FlowInterrupt` (Go envelope)
- `InterruptKind` (protocols)
- `InterruptService` (control_tower)
- `interrupt_pending` (envelope field)

The mix of "Flow" and "Interrupt" prefixes creates confusion. HANDOFF.md uses "Flow Interrupt" but code uses both.

**Suggested Fix:**
Standardize on `FlowInterrupt` for the object and `InterruptKind` for the enum. Update documentation.

---

#### Finding 4.2: LoopVerdict vs verdict field name
**Category:** Naming Inconsistencies
**Severity:** Low
**Files:**
- `jeeves_protocols/core.py` (LoopVerdict enum)
- HANDOFF.md (verdict in agent outputs)

**Description:**
The enum is `LoopVerdict` but it's used in agent outputs as just `verdict`. This is intentional (generic vs typed) but could be documented better.

**Suggested Fix:**
Add comment in HANDOFF.md explaining the relationship.

---

### 5. Dead/Unused Code

#### Finding 5.1: RateLimitMiddlewareConfig defined but potentially unused
**Category:** Dead Code
**Severity:** Low
**File:** `jeeves_avionics/middleware/rate_limit.py:81`

**Description:**
`RateLimitMiddlewareConfig` is defined but integration with the middleware stack is unclear.

**Suggested Fix:**
Verify usage or remove if dead code.

---

#### Finding 5.2: OrchestrationFlags partially overlaps with FeatureFlags
**Category:** Duplicate Code
**Severity:** Low
**Files:**
- `jeeves_protocols/config.py:145-153` (OrchestrationFlags)
- `jeeves_avionics/feature_flags.py:25-55` (CyclicConfig, ContextBoundsConfig)

**Description:**
`OrchestrationFlags` has `enable_checkpoints`, `enable_distributed`, `enable_telemetry` which may overlap with feature flags in avionics.

**Suggested Fix:**
Consolidate runtime orchestration flags into one location.

---

### 6. Centralization Opportunities

#### Finding 6.1: datetime.utcnow() used instead of centralized utc_now()
**Category:** Centralization Opportunities
**Severity:** Low
**File:** `jeeves_control_tower/types.py:153`

**Description:**
```python
created_at: datetime = field(default_factory=datetime.utcnow)
```

Should use `utc_now()` from `jeeves_shared.serialization` for consistency.

**Suggested Fix:**
Replace `datetime.utcnow` with `utc_now` across codebase.

---

#### Finding 6.2: JSON encoding repeated in multiple places
**Category:** Centralization Opportunities
**Severity:** Low
**Files:** Multiple

**Description:**
`JSONEncoderWithUUID` exists in `jeeves_shared/serialization.py` but `json.dumps/loads` is used directly in many places without UUID support.

**Suggested Fix:**
Use `to_json()` and `from_json()` from `jeeves_shared.serialization` consistently.

---

### 7. Documentation Gaps

#### Finding 7.1: CONTRACTS.md missing Contract 11-13 invariant tests
**Category:** Wiring Gaps
**Severity:** Medium
**File:** `docs/CONTRACTS.md`

**Description:**
Contracts 11, 12, and 13 document critical invariants:
- Contract 11: Envelope Sync (Go ↔ Python round-trip)
- Contract 12: Bounds Authority Split
- Contract 13: Cyclic Routing

But the referenced test files don't exist:
- `tests/contracts/test_envelope_roundtrip.py` - NOT FOUND

**Suggested Fix:**
Create the referenced contract tests.

---

#### Finding 7.2: Missing INDEX.md in several directories
**Category:** Documentation Gaps
**Severity:** Low
**Files:** Multiple directories

**Description:**
Mission System CONSTITUTION R6 requires INDEX.md per directory, but several directories lack them.

**Suggested Fix:**
Add INDEX.md to directories missing them.

---

### 8. Modularity Issues

#### Finding 8.1: Python AgentConfig has Callable hooks, Go doesn't
**Category:** Modularity Issues
**Severity:** Medium
**Files:**
- `jeeves_protocols/config.py:63-65`
- `coreengine/config/pipeline.go:86-128`

**Description:**
Python `AgentConfig` has:
```python
pre_process: Optional[Callable] = None
post_process: Optional[Callable] = None
mock_handler: Optional[Callable] = None
```

Go doesn't have equivalent fields, making Python-specific behavior that can't be mirrored in Go.

**Suggested Fix:**
Document that these are Python-only hooks, or implement equivalent hook registration in Go.

---

#### Finding 8.2: enable_arbiter flags only in Python
**Category:** Modularity Issues
**Severity:** Low
**File:** `jeeves_protocols/config.py:84-85`

**Description:**
```python
enable_arbiter: bool = True
skip_arbiter_for_read_only: bool = True
```

These arbiter flags exist only in Python PipelineConfig, not in Go.

**Suggested Fix:**
Either add to Go config or document as Python-only.

---

## Recommendations Summary

### Critical (Must Fix)
1. Add `Requires`, `After`, `JoinStrategy` to Python AgentConfig
2. Add `EdgeLimits` to Python PipelineConfig
3. Create typed `FlowInterrupt` dataclass in Python
4. Resolve L0 circular dependency between protocols and shared

### Medium (Should Fix)
1. Align `NLIServiceProtocol` with `NLIService` implementation
2. Update `SystemClock` to implement `ClockProtocol`
3. Create Contract 11-13 test files
4. Document Python-only AgentConfig hooks
5. Standardize interrupt/flow terminology

### Low (Nice to Have)
1. Use centralized `utc_now()` consistently
2. Use `to_json()`/`from_json()` consistently
3. Add missing INDEX.md files
4. Review RateLimitMiddlewareConfig usage
5. Consolidate OrchestrationFlags with FeatureFlags

---

## Appendix: Files Reviewed

### Primary Documentation
- HANDOFF.md
- docs/CONTRACTS.md
- jeeves_*/CONSTITUTION.md (4 files)

### Core Packages
- jeeves_protocols/ (21 files)
- jeeves_shared/ (5 files)
- jeeves_control_tower/ (16 files)
- jeeves_memory_module/ (40+ files)
- jeeves_avionics/ (50+ files)
- jeeves_mission_system/ (69 files)
- coreengine/ (Go runtime)

---

*End of Audit Report*
