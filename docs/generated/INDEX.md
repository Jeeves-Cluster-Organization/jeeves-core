# Jeeves Core Documentation

**Version:** 2.0.2
**Last Updated:** 2026-01-25

This documentation provides a comprehensive guide to understanding the jeeves-core runtime architecture, components, and data flow.

---

## Test Status

| Suite | Status | Coverage |
|-------|--------|----------|
| **Go Unit** | ✅ All pass | 77-100% |
| **Python Unit** | ✅ 325 pass | 64% |
| **Integration (no DB)** | ✅ 59 pass | - |
| **Integration (needs PostgreSQL)** | ⏭️ 25 skipped | - |

### Go Coverage by Package

| Package | Coverage |
|---------|----------|
| coreengine/tools | 100% |
| coreengine/config | 95.6% |
| coreengine/runtime | 91.8% |
| coreengine/agents | 87.1% |
| coreengine/envelope | 85.2% |
| commbus | 77.9% |

### Dependencies for Full Test Suite

- **Unit tests**: No external deps (sentence-transformers optional)
- **Integration**: `opentelemetry-*`, `prometheus-client`, `redis`
- **Database tests**: PostgreSQL required

---

## Quick Navigation

### Go Components (Core Engine)

| Module | Description | Documentation |
|--------|-------------|---------------|
| **commbus/** | In-memory message bus for pub/sub, query/response | [README](commbus/README.md) |
| **coreengine/** | Core runtime, agents, config, envelope handling | [README](coreengine/README.md) |
| **cmd/** | CLI tools for Python-Go interop | [README](cmd/README.md) |

### Python Components (Application Layer)

| Module | Layer | Description | Documentation |
|--------|-------|-------------|---------------|
| **jeeves_protocols** | L0 | Type contracts, protocols, enums | [README](jeeves_protocols/README.md) |
| **jeeves_shared** | L0 | Shared utilities (UUID, logging, serialization) | [README](jeeves_shared/README.md) |
| **jeeves_control_tower** | L1 | OS-like kernel (lifecycle, resources, IPC) | [README](jeeves_control_tower/README.md) |
| **jeeves_memory_module** | L2 | Memory services (events, semantic, session) | [README](jeeves_memory_module/README.md) |
| **jeeves_avionics** | L3 | Infrastructure (LLM, DB, Gateway, Tools) | [README](jeeves_avionics/README.md) |
| **jeeves_mission_system** | L4 | Orchestration framework, API, contracts | [README](jeeves_mission_system/README.md) |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CAPABILITY LAYER                                   │
│     Your domain-specific code (agents, tools, configs)                       │
│     Example: jeeves-capability-code-analysis                                 │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  L4: jeeves_mission_system        │  Application Layer                      │
│  ┌─────────────┐ ┌──────────────┐ │  - Orchestration framework              │
│  │ Orchestrator│ │  REST API    │ │  - HTTP/gRPC endpoints                  │
│  └─────────────┘ └──────────────┘ │  - Capability registration              │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
┌───────────────────────────────┐   ┌─────────────────────────────────────────┐
│  L1: jeeves_control_tower     │   │  L3: jeeves_avionics                    │
│  ┌─────────┐ ┌─────────────┐  │   │  ┌─────────┐ ┌──────┐ ┌─────────────┐  │
│  │ Kernel  │ │ Lifecycle   │  │   │  │ Gateway │ │ LLM  │ │  Database   │  │
│  └─────────┘ └─────────────┘  │   │  └─────────┘ └──────┘ └─────────────┘  │
│  ┌─────────┐ ┌─────────────┐  │   │  ┌─────────┐ ┌──────┐ ┌─────────────┐  │
│  │Resources│ │   IPC       │  │   │  │  Tools  │ │Observ│ │  Middleware │  │
│  └─────────┘ └─────────────┘  │   │  └─────────┘ └──────┘ └─────────────┘  │
└───────────────────────────────┘   └─────────────────────────────────────────┘
                    │                               │
                    └───────────────┬───────────────┘
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  L2: jeeves_memory_module                                                   │
│  ┌─────────────┐ ┌───────────────┐ ┌─────────────┐ ┌──────────────────────┐ │
│  │Event Source │ │Semantic Memory│ │Session State│ │   Tool Health        │ │
│  └─────────────┘ └───────────────┘ └─────────────┘ └──────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  L0: jeeves_protocols + jeeves_shared     │  Foundation Layer               │
│  ┌───────────────┐ ┌──────────────┐ ┌───────────────┐ ┌──────────────────┐  │
│  │  Protocols    │ │    Enums     │ │  Dataclasses  │ │    Utilities     │  │
│  └───────────────┘ └──────────────┘ └───────────────┘ └──────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  GO CORE: coreengine + commbus            │  Authoritative Runtime          │
│  ┌─────────────┐ ┌──────────────┐ ┌───────────────┐ ┌──────────────────┐    │
│  │   Runtime   │ │   Agents     │ │   Envelope    │ │    CommBus       │    │
│  └─────────────┘ └──────────────┘ └───────────────┘ └──────────────────┘    │
│  ┌─────────────┐ ┌──────────────┐ ┌───────────────┐                         │
│  │   Config    │ │    Tools     │ │    gRPC       │                         │
│  └─────────────┘ └──────────────┘ └───────────────┘                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Request Flow

This diagram shows how a request flows through the system:

```
                                 User Request
                                      │
                                      ▼
                        ┌─────────────────────────┐
                        │   Gateway (FastAPI)     │
                        │   HTTP / WebSocket      │
                        └───────────┬─────────────┘
                                    │
                                    ▼
                        ┌─────────────────────────┐
                        │   Control Tower         │
                        │   (Lifecycle Manager)   │
                        │   - Rate limiting       │
                        │   - Resource allocation │
                        │   - Request tracking    │
                        └───────────┬─────────────┘
                                    │
                                    ▼
                        ┌─────────────────────────┐
                        │   Mission System        │
                        │   (Orchestrator)        │
                        │   - Pipeline config     │
                        │   - Agent coordination  │
                        └───────────┬─────────────┘
                                    │
            ┌───────────────────────┼───────────────────────┐
            │                       │                       │
            ▼                       ▼                       ▼
    ┌───────────────┐       ┌───────────────┐       ┌───────────────┐
    │    Stage A    │ ──→   │    Stage B    │ ──→   │    Stage C    │
    │   (Agent)     │       │   (Agent)     │       │   (Agent)     │
    └───────────────┘       └───────┬───────┘       └───────────────┘
            │                       │                       │
            │                       ▼                       │
            │               ┌───────────────┐               │
            │               │  Tool Calls   │               │
            │               │  (Executor)   │               │
            │               └───────────────┘               │
            │                       │                       │
            └───────────────────────┼───────────────────────┘
                                    │
                                    ▼
                        ┌─────────────────────────┐
                        │   Envelope       │
                        │   (State Container)     │
                        │   - outputs{}           │
                        │   - processing_history  │
                        │   - bounds tracking     │
                        └───────────┬─────────────┘
                                    │
                                    ▼
                        ┌─────────────────────────┐
                        │   Memory Module         │
                        │   - Event persistence   │
                        │   - Session state       │
                        │   - Semantic search     │
                        └───────────┬─────────────┘
                                    │
                                    ▼
                              Response to User
```

---

## Pipeline Execution Flow

The core execution model supports sequential, parallel, and cyclic routing:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         PIPELINE EXECUTION                                   │
│                                                                             │
│   ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐              │
│   │  start  │ ──→ │ Stage A │ ──→ │ Stage B │ ──→ │ Stage C │ ──→ end     │
│   └─────────┘     └────┬────┘     └────┬────┘     └────┬────┘              │
│                        │               │               │                    │
│                        │               │    ┌──────────┘                    │
│                        │               │    │  loop_back (if verdict)       │
│                        │               │    ▼                               │
│                        │               └────────────────────────────┐       │
│                        │                                            │       │
│                        └────────────────────────────────────────────┘       │
│                                   (cyclic routing with EdgeLimits)          │
│                                                                             │
│   Parallel Mode:                                                            │
│   ┌─────────┐     ┌─────────┐                                               │
│   │ Stage A │     │ Stage B │  (run concurrently if no dependencies)        │
│   └────┬────┘     └────┬────┘                                               │
│        └───────┬───────┘                                                    │
│                ▼                                                            │
│         ┌─────────┐                                                         │
│         │ Stage C │  (waits for A and B via JoinStrategy)                   │
│         └─────────┘                                                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Data Flow: Envelope

The `Envelope` is the central state container that flows through all stages:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Envelope                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│  Identity                                                                    │
│  ├── envelope_id: "env-abc123"                                              │
│  ├── request_id: "req-def456"                                               │
│  ├── user_id: "user-789"                                                    │
│  └── session_id: "sess-012"                                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│  Input                                                                       │
│  ├── raw_input: "Analyze the authentication flow"                           │
│  └── received_at: "2026-01-23T10:30:00Z"                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  Outputs (per-agent)                                                         │
│  ├── stage_a: { plan: [...], confidence: 0.9 }                              │
│  ├── stage_b: { findings: [...], tool_results: [...] }                      │
│  └── stage_c: { verdict: "proceed", summary: "..." }                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  Pipeline State                                                              │
│  ├── current_stage: "stage_b"                                               │
│  ├── stage_order: ["stage_a", "stage_b", "stage_c"]                         │
│  └── iteration: 1                                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│  Bounds Tracking                                                             │
│  ├── llm_call_count: 5  (max: 10)                                           │
│  ├── agent_hop_count: 7  (max: 21)                                          │
│  └── terminal_reason: null                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│  Processing History                                                          │
│  └── [ { stage: "stage_a", duration_ms: 1200, status: "success" }, ... ]    │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Memory Architecture (L1-L7)

The memory module implements a seven-layer memory architecture:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  L7: System Introspection                                                   │
│      Tool health metrics, performance tracking                              │
├─────────────────────────────────────────────────────────────────────────────┤
│  L6: Skills & Patterns                                                      │
│      Learned behaviors, usage patterns (extensible stub)                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  L5: Entity Graphs                                                          │
│      Entity relationships, knowledge graphs (extensible stub)               │
├─────────────────────────────────────────────────────────────────────────────┤
│  L4: Working Memory (Session State)                                         │
│      Per-session state, focus tracking, findings                            │
├─────────────────────────────────────────────────────────────────────────────┤
│  L3: Semantic Memory                                                        │
│      Embeddings, vector search (pgvector), code indexing                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  L2: Event Log                                                              │
│      Append-only event sourcing, session-scoped deduplication               │
├─────────────────────────────────────────────────────────────────────────────┤
│  L1: Episodic Memory                                                        │
│      Per-request state (Envelope)                                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Import Boundaries

Strict layer import rules ensure architectural integrity:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Layer          │ May Import From                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│  Capability     │ L4, L3, L2, L1, L0                                        │
│  L4 (mission)   │ L3, L2, L1, L0                                            │
│  L3 (avionics)  │ L2, L1, L0                                                │
│  L2 (memory)    │ L1, L0                                                    │
│  L1 (control)   │ L0 only                                                   │
│  L0 (protocols) │ Nothing (zero dependencies)                               │
└─────────────────────────────────────────────────────────────────────────────┘

FORBIDDEN:
  ❌ Importing from coreengine/ (Go package)
  ❌ Importing internal modules not in exports
  ❌ Higher layers importing from lower layers' internals
```

---

## Key Contracts

### Go is Authoritative (Contract 12)

The Go runtime is the **source of truth** for:
- Envelope creation and state management
- Bounds checking (iterations, LLM calls, agent hops)
- Pipeline orchestration
- Routing decisions

Python code **must** respect Go's decisions:
```python
bounds = grpc_client.check_bounds(envelope)
if not bounds.can_continue:
    # Go says stop - Python must respect this
    return early
```

### Tool Ownership

`ToolId` enums are **CAPABILITY-OWNED**, not defined in core:
```python
# In your capability (NOT in jeeves-core)
class ToolId(str, Enum):
    MY_TOOL = "my_capability.my_tool"
```

### Protocol-First Design

All components depend on protocols, not implementations:
```python
# Good: Depend on protocol
def process(logger: LoggerProtocol, llm: LLMProviderProtocol): ...

# Bad: Depend on implementation
def process(logger: StructLogger, llm: OpenAIProvider): ...
```

---

## Quick Start: Building a Capability

```python
# 1. Define your capability's wiring
from jeeves_protocols import get_capability_resource_registry, CapabilityModeConfig
from jeeves_avionics.capability_registry import get_capability_registry

def register_capability():
    # Register mode
    registry = get_capability_resource_registry()
    registry.register_mode("my_capability", CapabilityModeConfig(...))
    
    # Register agent configs
    llm_registry = get_capability_registry()
    for agent_name, config in AGENT_CONFIGS.items():
        llm_registry.register("my_capability", agent_name, config)
    
    # Register tools
    register_tools()

# 2. Bootstrap your capability
async def main():
    register_capability()  # FIRST
    
    from jeeves_mission_system.adapters import get_logger
    logger = get_logger()
    
    await start_server()

# 3. Define your pipeline
PIPELINE = PipelineConfig(
    name="my_pipeline",
    agents=[STAGE_A, STAGE_B, STAGE_C],
    max_iterations=3,
    clarification_resume_stage="stage_a",
)
```

---

## Documentation Index

### Go Core

| Document | Topics |
|----------|--------|
| [commbus/README.md](commbus/README.md) | Message bus, pub/sub, query/response, middleware |
| [coreengine/README.md](coreengine/README.md) | Core engine overview, package structure |
| [coreengine/runtime.md](coreengine/runtime.md) | Sequential/parallel execution, bounds, interrupts |
| [coreengine/agents.md](coreengine/agents.md) | Agent, outcomes, lifecycle |
| [coreengine/config.md](coreengine/config.md) | PipelineConfig, AgentConfig, routing |
| [coreengine/envelope.md](coreengine/envelope.md) | Envelope structure, state tracking |
| [coreengine/tools.md](coreengine/tools.md) | Tool executor, definitions |
| [coreengine/grpc.md](coreengine/grpc.md) | gRPC server, streaming |
| [cmd/README.md](cmd/README.md) | CLI tools for Python-Go interop |

### Python Packages

| Document | Topics |
|----------|--------|
| [jeeves_protocols/README.md](jeeves_protocols/README.md) | L0 foundation, type contracts |
| [jeeves_shared/README.md](jeeves_shared/README.md) | Utilities, UUID, serialization |
| [jeeves_control_tower/README.md](jeeves_control_tower/README.md) | Kernel, lifecycle, resources, IPC |
| [jeeves_avionics/README.md](jeeves_avionics/README.md) | Infrastructure, gateway, LLM, database |
| [jeeves_memory_module/README.md](jeeves_memory_module/README.md) | Memory layers, event sourcing, semantic |
| [jeeves_mission_system/README.md](jeeves_mission_system/README.md) | Orchestration, API, contracts |

---

## Related Documents

- [CONTRACT.md](../../CONTRACT.md) - Runtime contract for capabilities
- [HANDOFF.md](../../HANDOFF.md) - Complete internal documentation
- [docs/CONTRACTS.md](../CONTRACTS.md) - Internal architectural contracts
- [docs/ARCHITECTURE_REVIEW.md](../ARCHITECTURE_REVIEW.md) - Execution model review

---

*This documentation is auto-generated. Last updated: 2026-01-25*
