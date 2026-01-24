# jeeves_mission_system - Application Layer (L4)

**Package:** `jeeves_mission_system`  
**Layer:** Application Layer (L4)  
**Version:** 4.0.0  
**Updated:** 2026-01-23

---

## Overview

The `jeeves_mission_system` package is the **Application Layer** of the Jeeves runtime. It provides the orchestration framework, API endpoints, and foundational infrastructure that capabilities build upon.

### Position in Architecture

```
Capability Layer (external)       →  Domain-specific capabilities (e.g., code analysis)
        ↓
jeeves_mission_system/            →  Application layer (THIS)
        ↓
jeeves_avionics/                  →  Infrastructure (database, LLM, gateway)
        ↓
jeeves_control_tower/             →  Kernel layer (lifecycle, resources, IPC)
        ↓
jeeves_protocols/, jeeves_shared/ →  Foundation (L0)
        ↓
commbus/, coreengine/             →  Go core
```

### Key Principle

This layer provides **orchestration infrastructure** using services from Avionics and abstractions from Core Engine. Domain-specific logic belongs in capability layers.

---

## Module Index

| Directory | Description | Documentation |
|-----------|-------------|---------------|
| [api/](api.md) | HTTP REST API endpoints (chat, health, governance) | [api.md](api.md) |
| [orchestrator/](orchestrator.md) | Flow orchestration, event emission, pipeline execution | [orchestrator.md](orchestrator.md) |
| [config/](config.md) | Generic configuration mechanisms and agent profiles | [config.md](config.md) |
| [contracts/](contracts.md) | Contract definitions and type re-exports | [contracts.md](contracts.md) |
| [events/](events.md) | Event integration layer for Control Tower | [events.md](events.md) |
| [prompts/](prompts.md) | Prompt registry and template system | [prompts.md](prompts.md) |
| [services/](services.md) | Application services (Chat, Debug, Worker) | [services.md](services.md) |

### Other Modules

| Directory/File | Description |
|----------------|-------------|
| `common/` | Shared utilities (CoT proxy, prompt compression, formatting) |
| `verticals/` | Vertical registry for capability registration |
| `proto/` | gRPC protocol definitions |
| `bootstrap.py` | Composition root, `create_app_context()` |
| `adapters.py` | Avionics service wrappers |

---

## Core Architecture

### Reference 7-Agent Pipeline

The Mission System provides infrastructure for the reference 7-agent pipeline:

```
                    ┌────────────────────────────────────────────────────────────────────┐
                    │        STAGE LOOP (goals-driven)                                   │
                    │                                                                    │
Perception → Intent → Planner → Traverser → Synthesizer → Critic ──┬──→ Integration → END
                ↑                                           │       │
                │                                           │       │
                └────────[reintent_transition]──────────────┘       │
                                                                    │
                └──────────[stage_transition]←[NEXT_STAGE]──────────┘
```

**REINTENT Architecture:** All feedback loops go through Intent agent (centralized goal reassessment).

### Agent Responsibilities

| # | Agent | Type | Role | Output |
|---|-------|------|------|--------|
| 1 | **Perception** | Non-LLM | Load session state, normalize input | `PerceptionOutput` |
| 2 | **Intent** | LLM | Classify query, extract goals | `IntentOutput` |
| 3 | **Planner** | LLM | Select tools, create execution plan | `PlanOutput` |
| 4 | **Traverser** | Non-LLM | Execute tools, collect evidence | `ExecutionOutput` |
| 5 | **Synthesizer** | LLM | Aggregate findings, structure analysis | `SynthesizerOutput` |
| 6 | **Critic** | LLM | Validate goal satisfaction | `CriticOutput` |
| 7 | **Integration** | Non-LLM | Format response, persist state | `Response` |

---

## Key Exports

### From `api/__init__.py`

```python
from jeeves_mission_system.api import (
    MissionRuntime,
    create_mission_runtime,
    ConfigRegistry,
)
```

### From `contracts/__init__.py`

```python
from jeeves_mission_system.contracts import (
    # Centralized Agent Architecture
    AgentConfig, PipelineConfig, RoutingRule, ToolAccess,
    Agent, Runtime, create_pipeline_runner,
    Envelope, ProcessingRecord, create_envelope,
    
    # Protocols
    LLMProviderProtocol, ToolProtocol, EventBusProtocol,
    
    # Config
    ConfigRegistry, AgentProfile, LLMProfile, ThresholdProfile,
)
```

### From `orchestrator/__init__.py`

```python
from jeeves_mission_system.orchestrator import (
    EventOrchestrator,
    create_event_orchestrator,
    AgentEventType,
    AgentEvent,
    EventEmitter,
    EventContext,
    # Factory functions
    create_agent_event_emitter,
    create_event_context,
)
```

---

## Import Boundary Rules

### Mission System MAY Import

- ✅ `jeeves_protocols` - Protocol definitions, types
- ✅ `jeeves_shared` - Shared utilities (logging, serialization)
- ✅ `jeeves_avionics` - Infrastructure layer
- ✅ `jeeves_control_tower` - Kernel layer

### Mission System MUST NOT Be Imported By

- ❌ `coreengine` (core is standalone Go)
- ❌ `jeeves_avionics` (one-way dependency)
- ❌ `jeeves_control_tower` (one-way dependency)

### Capabilities MUST

- ✅ Import from `mission_system.contracts` (not `core_engine`)
- ✅ Use `mission_system.adapters` (not `avionics` directly)

---

## Configuration

The Mission System provides **generic config mechanisms** while capabilities **own domain-specific configs**.

### Generic (Mission System)

- `ConfigRegistry` - Config injection pattern
- `AgentProfile`, `LLMProfile`, `ThresholdProfile` - Per-agent config types
- Operational constants (timeouts, retry limits)

### Domain-Specific (Capabilities Own)

- `LanguageConfig` - Language/file extension settings
- `ToolAccess` - Agent tool access matrix
- `InferenceEndpoint` - Hardware deployment profiles
- `Identity` - Product name, version

---

## Constitutional Alignment

### Evidence Chain Integrity (P1)

Every claim must trace to source code:

```
Tool Execution → ExecutionOutput → SynthesizerOutput → Integration → Response
                      ↓
                [file:line] citations
```

### Tool Boundary (P1 + P3)

Capabilities define their own tool sets. The runtime provides:
- **Composite** - Multi-step tools with fallback strategies
- **Resilient** - Wrapped base tools with graceful degradation
- **Standalone** - Simple, single-operation tools

### Bounded Retry (P3)

- Max 2 retries per step
- Return `attempt_history` for transparency
- Partial results acceptable with explanation

---

## Thresholds and Limits

| Limit | Default | Purpose |
|-------|---------|---------|
| `max_tree_depth` | 15 | Prevent runaway recursion |
| `max_grep_results` | 50 | Search result volume |
| `max_file_slice_tokens` | 4000 | Per-file code budget |
| `max_files_per_query` | 50 | Total file reads per query |
| `max_total_code_tokens` | 12000 | Total code in context |
| `max_retries_per_step` | 2 | Traverser retry limit |
| `max_llm_calls_per_query` | 10 | LLM budget per query |
| `max_stages` | 5 | Multi-stage execution limit |
| `max_agent_hops` | 21 | Total pipeline iterations |

---

## Related Documents

- [CONSTITUTION.md](../../../jeeves_mission_system/CONSTITUTION.md) - Full constitutional rules
- [ARCHITECTURE_DIAGRAM.md](../../../jeeves_mission_system/ARCHITECTURE_DIAGRAM.md) - Visual architecture
- [INDEX.md](../../../jeeves_mission_system/INDEX.md) - Directory index

---

*This documentation was auto-generated from source analysis.*
