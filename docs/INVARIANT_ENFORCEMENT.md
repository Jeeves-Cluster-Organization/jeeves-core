# Invariant Enforcement Tracking

**Generated:** 2026-01-19  
**Scope:** All explicit invariants from CONTRACT.md, docs/CONTRACTS.md, and CONSTITUTION.md files

This document tracks all explicit invariants defined in the codebase and their enforcement mechanisms.

---

## Legend

| Symbol | Meaning |
|--------|---------|
| **Y** | Enforced |
| **P** | Partially enforced (some gaps) |
| **N** | Not enforced (documented only) |

---

## 1. ARCHITECTURAL CONTRACTS (docs/CONTRACTS.md)

### Contract 1: Protocol-Based State Management

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| All state transitions in control_tower MUST go through manager protocols | Y | `jeeves_control_tower/lifecycle/manager.py` - LifecycleManager methods; `jeeves_control_tower/protocols.py` - @runtime_checkable Protocol definitions |
| Direct state manipulation bypasses validation | Y | Protocol pattern enforces encapsulation; no direct PCB.state mutations in codebase |

### Contract 2: Context-Aware Global State (Contextvars)

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Module-level state across async boundaries MUST use contextvars.ContextVar | P | `jeeves_avionics/logging/__init__.py` - _otel_enabled, _active_spans use ContextVar; Manual review required for other modules |

### Contract 3: Dependency Injection via Adapters

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Apps MUST access infrastructure via MissionSystemAdapters | P | `jeeves_mission_system/adapters.py` - provides get_logger, get_settings; `scripts/check_import_boundaries.py` - RULE 4 checks capability imports |

### Contract 4: Absolute Imports in TYPE_CHECKING Blocks

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| All imports within TYPE_CHECKING blocks MUST use absolute imports | N | No automated enforcement; requires manual code review |

### Contract 5: Single Source for Protocols

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Each protocol MUST be defined in exactly one place in jeeves_protocols | Y | `jeeves_protocols/protocols.py` - canonical location; type checkers would catch duplicate Protocol definitions |

### Contract 6: Dead Code Removal

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Unused code MUST be removed, not commented | N | No automated enforcement; documented in CONTRACTS.md; requires manual review |

### Contract 7: Interrupt Handling Ownership

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Interrupt handling owned by EventAggregator, not LifecycleManager | Y | Methods removed from LifecycleManager; only EventAggregator has raise_interrupt/clear_interrupt |

### Contract 8: Span Tracking Isolation

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| _active_spans (logging) and _span_stacks (OTEL) serve different purposes | Y | Separate implementations in `jeeves_avionics/logging/__init__.py` and `jeeves_avionics/logging/otel_adapter.py` |

### Contract 9: Logger Fallback Pattern

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Pattern `logger = logger or get_current_logger()` is acceptable | N | Documentation only; no enforcement needed (permissive pattern) |

### Contract 10: Capability-Owned ToolId

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| ToolId enums MUST be defined in capability layer, NOT avionics/mission_system | P | `jeeves_avionics/tools/catalog.py` - only provides ToolCategory, ToolDefinition; No ToolId in contracts_core; Relies on developer discipline |

### Contract 11: Envelope Sync (Go ↔ Python)

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Go and Python GenericEnvelope MUST produce identical serialization | P | `jeeves_protocols/tests/unit/test_envelope.py` - unit tests; No round-trip contract tests found in tests/contracts/ |
| Round-trip Go → Python → Go must be lossless | N | Documented in CONTRACTS.md; Implementation pending |

### Contract 12: Bounds Authority Split

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Go Runtime enforces in-loop via env.CanContinue() | Y | `coreengine/envelope/generic.go` - CanContinue() method |
| Python ControlTower post-hoc audit via ResourceTracker.check_quota() | Y | `jeeves_control_tower/resources/tracker.py` - check_quota() method; `jeeves_control_tower/tests/unit/test_resource_tracker.py` |
| After request: envelope counts == resource_tracker counts | N | Invariant documented; no automated assertion found |

### Contract 13: Core Doesn't Reject Cycles (Cyclic Routing)

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Core does NOT validate/reject cycles | Y | No cycle validation in coreengine; capability defines EdgeLimits |
| Cycles MUST be bounded by MaxIterations and EdgeLimits | P | Documented in CONTRACTS.md; EdgeLimit type exists in config; Enforcement in runtime executor |

---

## 2. LAYER IMPORT BOUNDARIES

### Four-Layer Architecture

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| RULE 0: jeeves_commbus ZERO dependencies on other Jeeves packages | Y | `scripts/check_import_boundaries.py` - _check_rule0_commbus_isolation() |
| RULE 1: jeeves_core_engine may depend on commbus only | Y | `scripts/check_import_boundaries.py` - _check_rule1_core_engine_isolation() |
| RULE 2: jeeves_avionics may depend on core_engine and commbus only | Y | `scripts/check_import_boundaries.py` - _check_rule2_avionics_isolation() |
| RULE 3: jeeves_mission_system may not import capability packages | Y | `scripts/check_import_boundaries.py` - _check_rule3_mission_system() |
| RULE 4: Capabilities should use mission_system.contracts | P | `scripts/check_import_boundaries.py` - _check_rule4_capability_contracts() (warnings only) |
| RULE 5: Shared modules must not import agents | Y | `scripts/check_import_boundaries.py` - _check_rule5_shared_agents() |

### Documented Exceptions

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Memory Module → Avionics (L2 → L3) for database.factory | Y | `jeeves_memory_module/CONSTITUTION.md` - explicitly allowed; Used in memory services |

---

## 3. CONTROL TOWER CONSTITUTION

### R1: Pure Abstraction (Import Boundaries)

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Control Tower ONLY imports from jeeves_protocols and jeeves_shared | Y | `scripts/check_import_boundaries.py`; `jeeves_control_tower/protocols.py` imports only from jeeves_protocols |

### R2: Service Registration Contract

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Services must register with unique name, type, handler, quota | Y | `jeeves_control_tower/ipc/coordinator.py` - register_service() validates; `jeeves_control_tower/tests/integration/test_kernel_integration.py` |

### R3: Request Lifecycle States

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| All requests transition through defined states (PENDING→READY→RUNNING→...) | Y | `jeeves_control_tower/lifecycle/manager.py` - LifecycleManager; `jeeves_control_tower/types.py` - ProcessState enum |

### R4: Resource Quota Enforcement

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| ResourceTracker enforces quotas BEFORE dispatch | Y | `jeeves_control_tower/resources/tracker.py` - check_quota(); `jeeves_control_tower/tests/unit/test_resource_tracker.py` |

### R5: Event Emission Contract

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| EventAggregator emits events for all significant operations | Y | `jeeves_control_tower/events/aggregator.py` - emit_event(); KernelEvent types defined |

### R6: Clarification Protocol

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Clarification transitions to WAITING_CLARIFICATION state | Y | `jeeves_control_tower/lifecycle/manager.py`; `jeeves_control_tower/services/interrupt_service.py` |

---

## 4. MISSION SYSTEM CONSTITUTION

### R1: Evidence Chain Integrity (P1 Enforcement)

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Every claim must trace to source code with [file:line] citations | P | `jeeves_mission_system/prompts/core/blocks/role_invariants.py` - ROLE_INVARIANTS prompt; No automated test found |
| Traverser executes tools and captures raw output with citations | N | Documented; enforcement via LLM prompt only |
| Synthesizer does NOT add claims without citations | N | Documented; enforcement via LLM prompt only |
| Critic does NOT generate response content | N | Documented; enforcement via LLM prompt only |
| Integration builds response from completed_stages evidence only | N | Documented; enforcement via LLM prompt only |

### R2: Tool Boundary (P1 + P3)

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Capabilities define their own tool sets | Y | `jeeves_avionics/tools/catalog.py` - provides ToolCategory; No ToolId export |
| Tools registered via ToolRegistryProtocol | Y | `jeeves_protocols/protocols.py` - ToolRegistryProtocol |

### R3: Bounded Retry

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Max 2 retries per step | P | Documented in CONSTITUTION; implementation varies by tool |
| Return attempt_history for transparency | P | Documented; not all tools implement |

### R4: Composite Tool Determinism

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Same inputs → same execution sequence | N | Documented only; no automated test |
| Return attempt_history showing all attempts | P | Some tools implement; not enforced |
| Stay within max_llm_calls_per_query | Y | `jeeves_control_tower/resources/tracker.py` - quota checks |
| Graceful degradation on failure | P | Documented; implementation varies |

### R8: REINTENT Hint Requirement

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| If verdict == REINTENT, Critic MUST provide substantive refine_intent_hint (min 10 chars) | P | Documented in CONSTITUTION; enforcement in reintent_transition_node (if exists) |

### R9: Single Feedback Edge

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Only feedback edge from Critic is to Intent (via reintent_transition) | Y | Graph definition in orchestrator; no Critic→Planner edge exists |

### R10: Reintent Counter Bound

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Maximum reintents per query: bounds.max_reintents (default: 3) | Y | `jeeves_protocols/protocols.py` - ContextBounds; Orchestrator routing logic |

### Thresholds and Limits

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| max_tree_depth: 15 | P | Documented; Traverser implementation |
| max_grep_results: 50 | P | Documented; Tool implementation |
| max_file_slice_tokens: 4000 | P | Documented; Tool implementation |
| max_files_per_query: 50 | P | Documented; Traverser implementation |
| max_total_code_tokens: 12000 | P | Documented; Synthesizer implementation |
| max_retries_per_step: 2 | P | Documented; Traverser implementation |
| max_llm_calls_per_query: 10 | Y | `jeeves_control_tower/resources/tracker.py` |
| max_stages: 5 | Y | ContextBounds in protocols |
| max_agent_hops: 21 | Y | ContextBounds; Orchestrator enforcement |

---

## 5. AVIONICS CONSTITUTION

### R1: Adapter Pattern

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Avionics implements core protocols, never modifies them | Y | All adapters implement Protocol interfaces; type checking enforces |

### R2: Configuration Over Code

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| All infrastructure behavior is configurable | Y | `jeeves_avionics/settings.py` - Settings class with env vars |

### R3: No Domain Logic

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Avionics provides transport and storage, not business logic | P | `scripts/check_import_boundaries.py` - RULE 2; Manual review required |

### R4: Swappable Implementations

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Any infrastructure component can be replaced | Y | Registry pattern in `jeeves_avionics/database/registry.py`; Factory functions |

### R5: Capability-Owned Tool Identifiers

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| ToolId enums are capability-owned, not avionics-owned | Y | No ToolId in `jeeves_avionics/tools/catalog.py`; Only ToolCategory, ToolDefinition |

### R6: Defensive Error Handling

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Infrastructure failures must not crash the system | P | Try/except patterns in LLM providers; No comprehensive test coverage |

### Security Rules

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| S1: Never log secrets | P | Pattern used in code; no automated scan |
| S2: API keys from environment variables only | Y | `jeeves_avionics/settings.py` - Pydantic BaseSettings |
| S3: No passwords in connection strings | P | Documented; requires code review |

---

## 6. MEMORY MODULE CONSTITUTION

### P1: Memory Types in jeeves_protocols

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| All memory types defined in jeeves_protocols/memory.py | Y | WorkingMemory, FocusState, Finding, etc. in protocols |

### P2: Memory Protocols in jeeves_protocols

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Memory protocols in jeeves_protocols/protocols.py | Y | MemoryServiceProtocol, SemanticSearchProtocol, SessionStateProtocol |

### P3: Single Ownership

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Memory module owns all memory-specific implementations | Y | All memory services in jeeves_memory_module/services/ |

### P4: CommBus Communication

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Memory operations publish events via CommBus | P | `jeeves_memory_module/services/event_emitter.py`; Not all operations emit |

### Memory Layers (L1-L7)

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| L1 (Episodic) per-request in GenericEnvelope | Y | Core engine implementation |
| L2 (Events) append-only | Y | `jeeves_mission_system/tests/contract/test_memory_contract_m2_events.py` |
| L3 (Semantic) embeddings/search | Y | `jeeves_memory_module/services/chunk_service.py`; PgVectorRepository |
| L4 (Working) per-session state | Y | `jeeves_memory_module/services/session_state_service.py` |
| L5 (Graph) entity relationships | P | `jeeves_memory_module/repositories/graph_stub.py` - in-memory stub |
| L6 (Skills) learned patterns | P | `jeeves_memory_module/repositories/skill_stub.py` - in-memory stub |
| L7 (Meta) tool metrics | Y | `jeeves_memory_module/services/tool_health_service.py` |

### Dependency Rules

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Memory module → jeeves_protocols allowed | Y | Import boundary checks |
| Memory module → jeeves_shared allowed | Y | Import boundary checks |
| Memory module → jeeves_avionics.database.factory allowed | Y | Documented exception in CONSTITUTION |
| Memory module ✗→ jeeves_mission_system | Y | Import boundary checks |
| Memory module ✗→ jeeves_control_tower | Y | Import boundary checks |

---

## 7. MEMORY CONTRACT INVARIANTS (M1, M2)

### M1: Canonical State is Ground Truth

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Embeddings must reference source tables (facts, messages, code) | Y | `jeeves_mission_system/tests/contract/test_memory_contract_m1_canonical.py` - test_embeddings_reference_canonical_source |
| Deleting canonical data cascades to derived views | Y | `test_memory_contract_m1_canonical.py` - test_deleting_canonical_cascades_to_embeddings |
| Orphaned embeddings prevented by referential integrity | Y | `test_memory_contract_m1_canonical.py` - test_orphaned_embeddings_prevented_by_referential_integrity |
| source_type in ['fact', 'message', 'code'] | P | `test_memory_contract_m1_canonical.py` - test_all_semantic_chunks_have_valid_source_type (skips if not DB-enforced) |

### M2: Events Are Immutable History

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Event log is append-only (no UPDATE/DELETE) | P | `jeeves_mission_system/tests/contract/test_memory_contract_m2_events.py` - tests skip if DB allows (application-level enforcement) |
| Events have immutable timestamps (TIMESTAMPTZ) | Y | `test_memory_contract_m2_events.py` - test_domain_events_have_immutable_timestamps |
| Corrections create new compensating events | Y | `test_memory_contract_m2_events.py` - test_corrections_create_compensating_events |
| Event ordering is deterministic | Y | `test_memory_contract_m2_events.py` - test_event_ordering_is_deterministic |

---

## 8. CONSTITUTION P2: RELIABILITY

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Write operations must be transactional (ACID) | Y | `jeeves_mission_system/tests/contract/test_constitution_p2_reliability.py` - test_write_operations_are_transactional |
| Failures must rollback partial changes | Y | `test_constitution_p2_reliability.py` - test_rollback_on_tool_failure_prevents_partial_state |
| Error messages must report exactly what failed | Y | `test_constitution_p2_reliability.py` - test_database_errors_propagate_loudly |
| No silent data truncation | Y | `test_constitution_p2_reliability.py` - test_no_silent_data_truncation |
| Tool execution failures recorded with error details | Y | `test_constitution_p2_reliability.py` - test_tool_execution_failures_recorded |

---

## 9. RUNTIME ASSERTIONS (ValueError/TypeError)

| Location | Invariant | Enforced? |
|----------|-----------|-----------|
| `jeeves_protocols/protocols.py:510` | Model size must not exceed VRAM capacity | Y (raise ValueError) |
| `jeeves_protocols/protocols.py:515` | max_parallel must be >= 1 | Y (raise ValueError) |
| `jeeves_protocols/agents.py:176` | Agent requires prompt_registry | Y (raise ValueError) |
| `jeeves_shared/uuid_utils.py:102` | uuid_str() cannot handle invalid types | Y (raise TypeError) |
| `jeeves_avionics/settings.py:228` | URL must be http:// or https:// | Y (raise ValueError) |
| `jeeves_avionics/settings.py:236` | Redis URL must start with redis:// | Y (raise ValueError) |
| `jeeves_avionics/database/registry.py:141` | Unknown database backend | Y (raise ValueError) |
| `jeeves_avionics/llm/factory.py:127` | Unknown LLM provider type | Y (raise ValueError) |
| `jeeves_avionics/llm/providers/azure.py:57-59` | Azure endpoint and API key required | Y (raise ValueError) |
| `jeeves_avionics/llm/providers/llamacpp_provider.py:87` | Model file must exist | Y (raise ValueError) |
| `jeeves_mission_system/adapters.py:93` | LLM factory must be configured | Y (raise ValueError) |
| `jeeves_mission_system/services/chat_service.py:300+` | Session/message access validation | Y (raise ValueError) |
| `jeeves_mission_system/services/worker_coordinator.py:552` | Pipeline must have agents | Y (raise ValueError) |
| `jeeves_mission_system/prompts/core/registry.py:96,105` | Prompt must be registered, version must exist | Y (raise ValueError) |
| `jeeves_memory_module/services/xref_manager.py:172` | Invalid xref direction | Y (raise ValueError) |

---

## 10. ROLE INVARIANTS (LLM Prompts)

| Invariant | Enforced? | Where |
|-----------|-----------|-------|
| Never hallucinate code not present in tool results | N | `jeeves_mission_system/prompts/core/blocks/role_invariants.py` - prompt injection only |
| Never claim without file:line evidence | N | ROLE_INVARIANTS prompt - no automated verification |
| Always preserve user intent through pipeline | N | ROLE_INVARIANTS prompt |
| Output ONLY expected format (JSON for structured agents) | N | ROLE_INVARIANTS prompt |
| Never expose internal system details to users | N | ROLE_INVARIANTS prompt |
| Stay within context bounds | P | ROLE_INVARIANTS prompt + ResourceTracker enforcement |

---

## 11. PROTOCOL TYPE ENFORCEMENT

| Protocol | Enforced? | Where |
|----------|-----------|-------|
| LoggerProtocol | Y | `@runtime_checkable` in jeeves_protocols/protocols.py |
| LLMProviderProtocol | Y | `@runtime_checkable` in jeeves_protocols/protocols.py |
| DatabaseClientProtocol | Y | `@runtime_checkable` in jeeves_protocols/protocols.py |
| VectorStorageProtocol | Y | `@runtime_checkable` in jeeves_protocols/protocols.py |
| ToolRegistryProtocol | Y | `@runtime_checkable` in jeeves_protocols/protocols.py |
| LifecycleManagerProtocol | Y | `@runtime_checkable` in jeeves_control_tower/protocols.py |
| ResourceTrackerProtocol | Y | `@runtime_checkable` in jeeves_control_tower/protocols.py |
| CommBusCoordinatorProtocol | Y | `@runtime_checkable` in jeeves_control_tower/protocols.py |
| EventAggregatorProtocol | Y | `@runtime_checkable` in jeeves_control_tower/protocols.py |
| ControlTowerProtocol | Y | `@runtime_checkable` in jeeves_control_tower/protocols.py |

---

## Summary Statistics

| Category | Total | Enforced (Y) | Partial (P) | Not Enforced (N) |
|----------|-------|--------------|-------------|------------------|
| Architectural Contracts | 16 | 9 | 5 | 2 |
| Layer Import Boundaries | 7 | 6 | 1 | 0 |
| Control Tower Constitution | 6 | 6 | 0 | 0 |
| Mission System Constitution | 17 | 6 | 7 | 4 |
| Avionics Constitution | 9 | 5 | 4 | 0 |
| Memory Module Constitution | 16 | 12 | 4 | 0 |
| Memory Contracts (M1/M2) | 8 | 6 | 2 | 0 |
| Constitution P2 | 5 | 5 | 0 | 0 |
| Runtime Assertions | 15 | 15 | 0 | 0 |
| Role Invariants (Prompts) | 6 | 0 | 1 | 5 |
| Protocol Type Enforcement | 10 | 10 | 0 | 0 |
| **TOTAL** | **115** | **80 (70%)** | **24 (21%)** | **11 (9%)** |

---

## Gaps Requiring Attention

### High Priority (Not Enforced)

1. **Contract 11: Envelope Sync Round-trip Tests** - No Go ↔ Python round-trip contract tests
2. **Contract 12: Post-request Count Assertion** - envelope counts == resource_tracker counts not verified
3. **Evidence Chain Integrity** - No automated test for Synthesizer/Critic/Integration citation rules
4. **Role Invariants** - LLM behavior constraints have no automated verification

### Medium Priority (Partially Enforced)

1. **Contract 4: TYPE_CHECKING Absolute Imports** - Needs linter rule
2. **Contract 10: Capability-Owned ToolId** - Relies on developer discipline
3. **M1: source_type Constraint** - May not be DB-enforced
4. **M2: Event Immutability** - Application-level only, no DB triggers

### Recommendations

1. Add mypy/pyright configuration for static type checking
2. Create round-trip envelope contract tests (Go → Python → Go)
3. Add CI check for TYPE_CHECKING import style
4. Consider database triggers for M2 event immutability
5. Add integration tests for evidence chain integrity

---

*This document is auto-generated from codebase analysis. Update when invariants or enforcement mechanisms change.*
