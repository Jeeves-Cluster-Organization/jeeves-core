# Mission System ↔ Code Analysis Coupling: Analysis & Decoupling Plan

## Executive Summary

The mission system has **significant hardcoded coupling to code_analysis**, despite the architectural intent to be capability-agnostic via `CapabilityResourceRegistry`. The coupling exists at multiple layers:

| Layer | Severity | Primary Issue |
|-------|----------|---------------|
| gRPC Servicer | **CRITICAL** | Parameter names, pipeline references hardcoded |
| HTTP API | **HIGH** | Session titles, "read-only" assumptions, WebSocket events |
| Prompts | **HIGH** | Code analysis identity and read-only assumptions |
| Orchestrator Package | **HIGH** | Package identity tied to "Code Analysis" |
| Configuration | MEDIUM | Language config keys, schema comments |
| Web UI | MEDIUM | Hardcoded branding |
| Tests | MEDIUM | Hardcoded service name assertions |

---

## Part 1: Complete Coupling Inventory

### 1.1 FlowServicer (gRPC Layer) — CRITICAL

**File:** `mission_system/orchestrator/flow_service.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 2 | Docstring | "FlowServicer - gRPC servicer that delegates to Code Analysis pipeline." | Documentation |
| 4 | Docstring | "This module provides the gRPC interface for the Code Analysis Agent." | Documentation |
| 5 | Docstring | "All requests are handled by the 6-agent Code Analysis pipeline." | Documentation |
| 7 | Docstring | "Pipeline: Perception → Intent → Planner → Traverser → Critic → Integration" | Hardcodes pipeline stages |
| 46 | Docstring | "All requests are delegated to the Code Analysis pipeline." | Documentation |
| 50 | Docstring | "- StartFlow: Process code analysis queries" | Documentation |
| **61** | **Code** | `code_analysis_servicer: Any` | **Variable name couples to code_analysis** |
| 69 | Docstring | "code_analysis_servicer: CodeAnalysisServicer instance" | Documentation |
| **74** | **Code** | `self.code_analysis_servicer = code_analysis_servicer` | **Attribute name couples to code_analysis** |
| 81 | Docstring | `"""Start a code analysis flow and stream events."""` | Documentation |
| **88** | **Code** | `"code_analysis_request"` | **Log event name hardcoded** |
| 94 | Comment | `# Delegate to Code Analysis pipeline` | Comment |
| **95** | **Code** | `self.code_analysis_servicer.process_request(...)` | **Uses hardcoded attribute** |
| **202** | **Code** | `f"Code Analysis - {datetime...}"` | **Session title hardcoded** |

**Root Cause:** FlowServicer was designed as a wrapper specifically for Code Analysis, not as a generic capability dispatcher.

---

### 1.2 HTTP API Server — HIGH

**File:** `mission_system/api/server.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| ~202 | Code | `f"Code Analysis - {datetime...}"` | Session title default |
| ~585 | Comment | `confirmation_needed=False, # Code analysis is read-only` | Assumes read-only |
| ~603 | Docstring | "Note: Code Analysis Agent is read-only and doesn't require confirmations." | Assumes read-only |
| ~615-619 | Code | Endpoint rejects confirmations: "Code analysis is read-only" | **Blocks write capabilities** |
| ~703-709 | Docstring | WebSocket events mention: perception.completed, intent.completed, planner.generated, etc. | **Pipeline stages hardcoded** |

**Root Cause:** API behavior was designed around code analysis being the only capability with read-only semantics.

---

### 1.3 Prompt Blocks — HIGH

**File:** `mission_system/prompts/core/blocks/safety_block.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 3-6 | Docstring | "These rules ensure safe operation in read-only code analysis mode." | Assumes read-only |
| 3-6 | Docstring | "Since all operations are read-only, the safety focus is on accuracy" | Assumes read-only |
| **10** | **Prompt** | "All operations are READ-ONLY - you cannot modify files" | **Cannot be reused for write capabilities** |

**File:** `mission_system/prompts/core/blocks/style_block.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 6 | Docstring | "Updated for code analysis focus." | Documentation |

**File:** `mission_system/prompts/core/blocks/role_invariants.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 5 | Docstring | "Updated for code analysis focus." | Documentation |
| 9 | Prompt | "Never hallucinate code not present in tool results" | Code-specific |

**File:** `mission_system/prompts/core/versions/planner.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| ~29 | Prompt | `"You are the Planner agent for Code Analysis Agent."` | Identity hardcoded |

**Root Cause:** Prompt blocks were written specifically for code analysis rather than being templated for any capability.

---

### 1.4 Orchestrator Package Identity — HIGH

**File:** `mission_system/orchestrator/__init__.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 2 | Docstring | "Code Analysis Orchestrator - gRPC service layer." | Package identity |
| 5 | Docstring | "Wraps the 7-agent code analysis orchestration" | Pipeline assumption |
| 6 | Docstring | "Exposes services for code analysis flow" | Capability assumption |
| 10 | Docstring | "Agent pipeline (Perception → Intent → Planner → Traverser → Synthesizer → Critic → Integration)" | Pipeline stages |

**File:** `mission_system/orchestrator/vertical_service.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 19 | Docstring | `vertical_id: ID of the registered vertical (e.g., "code_analysis")` | Example |

---

### 1.5 Web UI / Governance Dashboard — MEDIUM

**File:** `mission_system/api/governance.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| ~379 | HTML | `<title>Code Analysis Agent - Governance Dashboard</title>` | UI branding |
| ~458 | HTML | `<strong>Code Analysis Agent v3.0</strong>` | UI branding |

---

### 1.6 Database Schema — MEDIUM

**File:** `avionics/database/schemas/001_postgres_schema.sql`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 4 | Comment | `-- Schema for Code Analysis Agent v3.0` | Documentation |
| ~456 | Comment | `-- Event types for Code Analysis Agent` | Documentation |
| ~644 | Comment | `-- Edge types for Code Analysis Agent` | Documentation |

---

### 1.7 Tests — MEDIUM

**File:** `mission_system/tests/integration/test_api.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 138 | Assertion | `assert data["service"] == "Code Analysis Agent"` | Test expectation |

**File:** `mission_system/tests/integration/test_api_ci.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 38 | Assertion | `assert data["service"] == "Code Analysis Agent"` | Test expectation |

---

### 1.8 Configuration — LOW

**File:** `mission_system/config/registry.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 24 | Comment | `"language_config": Language configuration for code analysis capability` | Documentation |

**File:** `mission_system/contracts_core.py`

| Line | Type | Content | Impact |
|------|------|---------|--------|
| 97, 110 | Comment | References to `jeeves-capability-code-analyser/tools/catalog.py` | Path reference |

---

## Part 2: Architectural Patterns Causing Coupling

### Pattern 1: Single-Capability Design

**Where:** `flow_service.py` constructor

```python
def __init__(
    self,
    db: DatabaseClientProtocol,
    code_analysis_servicer: Any,  # Single servicer, not a registry
    ...
):
```

**Problem:** The FlowServicer accepts ONE orchestrator/servicer, not a capability-aware dispatcher. This means:
- No routing based on capability type
- No way to support multiple capabilities simultaneously
- Parameter naming creates semantic coupling

**Effect:** Any new capability must either:
1. Replace code_analysis entirely, OR
2. Have the caller construct the appropriate servicer before passing it in

---

### Pattern 2: Hardcoded Read-Only Semantics

**Where:** `server.py`, `safety_block.py`

```python
# server.py ~615
return {"error": "Code analysis is read-only and does not require confirmations"}

# safety_block.py line 10
"All operations are READ-ONLY - you cannot modify files"
```

**Problem:** The mission system assumes all capabilities are read-only like code analysis. This blocks:
- Code generation capabilities
- Code modification capabilities
- File writing capabilities
- Any capability that requires user confirmation

**Effect:** Write-capable capabilities cannot use the standard API flow.

---

### Pattern 3: Pipeline Stage Assumptions

**Where:** `flow_service.py` docstrings, `server.py` WebSocket documentation

```
Pipeline: Perception → Intent → Planner → Traverser → Critic → Integration
```

**Problem:** The mission system documents and logs specific pipeline stages (Perception, Intent, Planner, etc.). This:
- Assumes all capabilities use the same 6/7 agent pipeline
- Makes WebSocket event documentation incorrect for other pipeline structures
- Creates expectations in client code about event names

**Effect:** Capabilities with different architectures (e.g., 3-agent pipeline, single-agent, different stage names) will:
- Generate unexpected events
- Have incorrect documentation
- Confuse monitoring/observability tools

---

### Pattern 4: Implicit Interface via `Any` Type

**Where:** `flow_service.py:61`

```python
code_analysis_servicer: Any
```

**Problem:** Using `Any` avoids import coupling but creates an undocumented protocol dependency. The servicer must have:
- `process_request(user_id, session_id, message, context)` method
- Async iterator return type
- Specific event format

**Effect:** No compile-time or IDE verification that a capability implements the required interface.

---

### Pattern 5: Domain Knowledge in Infrastructure

**Where:** Prompt blocks in `prompts/core/blocks/`

**Problem:** Core infrastructure prompts contain domain-specific safety rules ("read-only code analysis") instead of loading these from capability configuration.

**Effect:** To add a new capability, you must either:
1. Edit shared prompt blocks (couples all capabilities)
2. Override/replace blocks entirely (duplication)
3. The capability inherits inappropriate rules

---

## Part 3: Decoupling Plan

### Phase 1: Interface Formalization (Foundation)

#### 1.1 Define CapabilityServicerProtocol

**File to create:** `protocols/servicer.py`

```python
from typing import Protocol, AsyncIterator, Any, Optional

class CapabilityServicerProtocol(Protocol):
    """Protocol for capability orchestrator servicers.

    Any capability that wants to be invoked via FlowServicer must implement this.
    """

    async def process_request(
        self,
        user_id: str,
        session_id: Optional[str],
        message: str,
        context: Optional[dict],
    ) -> AsyncIterator[Any]:
        """Process a request and yield events.

        Args:
            user_id: User identifier
            session_id: Optional session identifier
            message: User message/query
            context: Optional context dictionary

        Yields:
            Flow events (implementation-specific format)
        """
        ...
```

**Effect:**
- Type safety for capability implementations
- IDE autocomplete and error checking
- Documentation of required interface
- No coupling to specific capability

---

#### 1.2 Add Capability Metadata to Registry

**File to modify:** `protocols/capability.py`

Add to `DomainServiceConfig`:

```python
@dataclass
class DomainServiceConfig:
    service_id: str
    service_type: str = "flow"
    capabilities: List[str] = field(default_factory=list)
    max_concurrent: int = 10
    is_default: bool = False
    # NEW FIELDS
    is_readonly: bool = True  # Whether this capability modifies state
    requires_confirmation: bool = False  # Whether actions need user confirmation
    default_session_title: str = "Session"  # Default title for new sessions
    pipeline_stages: List[str] = field(default_factory=list)  # Agent stages if any
```

**Effect:**
- API can query `is_readonly` instead of hardcoding
- Session titles come from capability config
- WebSocket documentation can be generated from `pipeline_stages`
- Confirmation handling is capability-specific

---

### Phase 2: FlowServicer Generalization

#### 2.1 Rename Parameter and Attribute

**File:** `mission_system/orchestrator/flow_service.py`

**Change:**
```python
# Before
def __init__(
    self,
    db: DatabaseClientProtocol,
    code_analysis_servicer: Any,
    ...
):
    self.code_analysis_servicer = code_analysis_servicer

# After
def __init__(
    self,
    db: DatabaseClientProtocol,
    capability_servicer: CapabilityServicerProtocol,
    ...
):
    self._servicer = capability_servicer
```

**Effect:**
- Generic naming removes semantic coupling
- Protocol type enables static analysis
- Internal attribute becomes private (underscore prefix)

---

#### 2.2 Update Log Event Names

**Change:**
```python
# Before
self._logger.info("code_analysis_request", ...)

# After
self._logger.info("capability_request", ...)
```

**Effect:**
- Log analysis tools don't assume code_analysis
- Dashboards work for any capability

---

#### 2.3 Dynamic Session Titles

**Change:**
```python
# Before
title = request.title or f"Code Analysis - {datetime.now(...)}"

# After
from protocols.capability import get_capability_resource_registry

registry = get_capability_resource_registry()
default_service = registry.get_default_service()
service_config = registry.get_service_config(default_service) if default_service else None
default_title = service_config.default_session_title if service_config else "Session"
title = request.title or f"{default_title} - {datetime.now(...)}"
```

**Effect:**
- Session titles reflect active capability
- No hardcoded "Code Analysis" string

---

#### 2.4 Update Docstrings to be Generic

**Change all docstrings** from:
- "Code Analysis pipeline" → "capability pipeline"
- "Code Analysis Agent" → "registered capability"
- Pipeline stage list → "See capability configuration for stages"

**Effect:**
- Documentation doesn't mislead about architecture
- Reduces maintenance burden when adding capabilities

---

### Phase 3: API Server Generalization

#### 3.1 Remove Read-Only Assumption

**File:** `mission_system/api/server.py`

**Change confirmation endpoint:**
```python
# Before
return {"error": "Code analysis is read-only and does not require confirmations"}

# After
registry = get_capability_resource_registry()
default_service = registry.get_default_service()
service_config = registry.get_service_config(default_service)

if service_config and not service_config.requires_confirmation:
    return {"error": f"{service_config.service_id} does not require confirmations"}
# Otherwise, process the confirmation
```

**Effect:**
- Write-capable capabilities can use confirmation flow
- Error messages are capability-specific

---

#### 3.2 Dynamic WebSocket Event Documentation

**Change docstring generation:**
```python
# Before (hardcoded)
"""
Events:
- perception.completed
- intent.completed
...
"""

# After (generated)
registry = get_capability_resource_registry()
# Generate docs from registry.get_agents() or service_config.pipeline_stages
```

**Effect:**
- Documentation matches actual capability pipeline
- Reduces documentation drift

---

### Phase 4: Prompt Block Templating

#### 4.1 Make Safety Block Capability-Aware

**File:** `mission_system/prompts/core/blocks/safety_block.py`

**Change:**
```python
# Before
SAFETY_BLOCK = """Safety Rules:
- All operations are READ-ONLY - you cannot modify files
...
"""

# After
def get_safety_block(is_readonly: bool = True) -> str:
    """Get safety block appropriate for capability mode.

    Args:
        is_readonly: Whether the capability is read-only

    Returns:
        Safety block text
    """
    if is_readonly:
        return """Safety Rules:
- All operations are READ-ONLY - you cannot modify files
- When uncertain, ask for clarification rather than guess
..."""
    else:
        return """Safety Rules:
- You may modify files when instructed - be careful and deliberate
- Always confirm destructive operations before executing
- When uncertain, ask for clarification rather than guess
..."""
```

**Effect:**
- Write-capable capabilities get appropriate safety rules
- Read-only capabilities retain current behavior
- Single source of truth for safety rules

---

#### 4.2 Template Other Blocks

Apply similar pattern to:
- `style_block.py` - parameterize based on capability domain
- `role_invariants.py` - parameterize based on capability focus

**Effect:**
- Prompt blocks become reusable across capabilities
- Capability-specific content loaded from registry

---

### Phase 5: UI and Test Generalization

#### 5.1 Dynamic UI Branding

**File:** `mission_system/api/governance.py`

**Change:**
```python
# Before
<title>Code Analysis Agent - Governance Dashboard</title>

# After
registry = get_capability_resource_registry()
service_name = registry.get_default_service() or "Jeeves"
# Use in template: <title>{service_name} - Governance Dashboard</title>
```

**Effect:**
- UI shows correct capability name
- No manual updates when switching capabilities

---

#### 5.2 Update Test Assertions

**Files:** `test_api.py`, `test_api_ci.py`

**Change:**
```python
# Before
assert data["service"] == "Code Analysis Agent"

# After
# Option 1: Query registry for expected name
registry = get_capability_resource_registry()
expected = registry.get_default_service() or "default"
assert data["service"] == expected

# Option 2: Just check a service name exists
assert "service" in data
assert isinstance(data["service"], str)
```

**Effect:**
- Tests pass regardless of registered capability
- Tests focus on structural correctness, not specific strings

---

### Phase 6: Documentation Cleanup

#### 6.1 Update Package Docstrings

Remove code_analysis references from:
- `orchestrator/__init__.py`
- `orchestrator/vertical_service.py`
- `api/server.py` module docstring

**Effect:**
- Documentation reflects actual architecture
- New developers don't assume code_analysis is mandatory

---

#### 6.2 Update Schema Comments

**File:** `avionics/database/schemas/001_postgres_schema.sql`

Change comments from "Code Analysis Agent" to "Jeeves Platform" or similar generic.

**Effect:**
- Schema is clearly generic infrastructure
- No confusion about capability-specific tables

---

## Part 4: Migration Checklist

### Critical Path (Must complete before adding new capability)

- [ ] Create `CapabilityServicerProtocol` in `protocols/servicer.py`
- [ ] Add `is_readonly`, `requires_confirmation`, `default_session_title` to `DomainServiceConfig`
- [ ] Rename `code_analysis_servicer` → `capability_servicer` in FlowServicer
- [ ] Update session title generation to use registry
- [ ] Remove hardcoded confirmation rejection in API server

### High Priority (Should complete soon after)

- [ ] Convert safety_block.py to function with `is_readonly` parameter
- [ ] Update prompt blocks to be capability-parameterized
- [ ] Change log event names from `code_analysis_*` to `capability_*`
- [ ] Update WebSocket documentation to be dynamic

### Medium Priority (Complete for production readiness)

- [ ] Update governance dashboard branding
- [ ] Fix test assertions to be capability-agnostic
- [ ] Update all package/module docstrings

### Low Priority (Nice to have)

- [ ] Update SQL schema comments
- [ ] Update contracts_core.py comments
- [ ] Create capability onboarding documentation

---

## Part 5: Effect Summary

| Change | Immediate Effect | Long-term Benefit |
|--------|------------------|-------------------|
| CapabilityServicerProtocol | Type safety for servicers | IDE support, static analysis |
| DomainServiceConfig extensions | Dynamic capability behavior | Self-describing capabilities |
| Rename code_analysis_servicer | Removes semantic coupling | Clearer abstraction |
| Dynamic session titles | Correct UI for any capability | Less maintenance |
| Read-only flag | Write capabilities work | Platform extensibility |
| Templated prompts | Capability-appropriate rules | Reusable prompt infrastructure |
| Dynamic UI branding | Correct dashboard labels | Professional appearance |
| Generic test assertions | Tests pass for any capability | CI stability |

---

## Appendix: Files Requiring Changes

### Must Change
1. `mission_system/orchestrator/flow_service.py` (17 changes)
2. `mission_system/api/server.py` (6 changes)
3. `mission_system/prompts/core/blocks/safety_block.py` (1 major refactor)
4. `protocols/capability.py` (4 new fields)

### Should Change
5. `mission_system/prompts/core/blocks/style_block.py`
6. `mission_system/prompts/core/blocks/role_invariants.py`
7. `mission_system/prompts/core/versions/planner.py`
8. `mission_system/orchestrator/__init__.py`
9. `mission_system/api/governance.py`

### Nice to Change
10. `mission_system/tests/integration/test_api.py`
11. `mission_system/tests/integration/test_api_ci.py`
12. `mission_system/orchestrator/vertical_service.py`
13. `avionics/database/schemas/001_postgres_schema.sql`
14. `mission_system/contracts_core.py`
15. `mission_system/config/registry.py`

### New Files to Create
16. `protocols/servicer.py` (CapabilityServicerProtocol)
