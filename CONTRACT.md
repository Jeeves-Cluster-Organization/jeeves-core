# Jeeves Core Runtime Contract

**Version:** 1.3
**Last Updated:** 2026-01-06

This document serves as the **Source of Truth** for capabilities built on top of `jeeves-core`. Capabilities should reference this contract rather than inspecting core internals.

> **Related Documents:**
> - [HANDOFF.md](HANDOFF.md) - Complete internal documentation for capability development
> - [docs/CONTRACTS.md](docs/CONTRACTS.md) - Internal architectural contracts

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture Layers](#architecture-layers)
3. [Import Boundaries](#import-boundaries)
4. [Core Protocols](#core-protocols)
5. [Registration Contract](#registration-contract)
6. [Tool Contract](#tool-contract)
7. [Agent Contract](#agent-contract)
8. [Memory Contract](#memory-contract)
9. [Event Contract](#event-contract)
10. [Configuration Contract](#configuration-contract)
11. [Quick Start](#quick-start)

---

## Overview

`jeeves-core` is a **layered agentic runtime** that provides:

- **Orchestration Framework**: LangGraph-based pipeline execution
- **LLM Provider Abstraction**: Multiple LLM backends (OpenAI, Anthropic, Azure, LlamaServer)
- **Memory Services**: Four-layer memory architecture (L1-L4)
- **Tool Registry**: Capability-defined tool registration and execution
- **Event System**: Real-time event emission and streaming
- **Resource Management**: Quotas, rate limiting, lifecycle management

**What jeeves-core IS NOT:**
- Not a domain-specific application
- Not an agent implementation
- Not a tool provider

Capabilities provide domain-specific logic; core provides the runtime.

---

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────┐
│  CAPABILITY LAYER (Your code)                               │
│  - Domain-specific agents                                   │
│  - Domain-specific tools                                    │
│  - Domain-specific configs                                  │
└─────────────────────────────────────────────────────────────┘
                              │ imports
                              ↓
┌─────────────────────────────────────────────────────────────┐
│  jeeves_mission_system (L2 - Application Layer)             │
│  - Orchestration framework                                  │
│  - HTTP API endpoints                                       │
│  - Capability registration                                  │
│  - contracts.py, contracts_core.py exports                  │
└─────────────────────────────────────────────────────────────┘
                              │ imports
                              ↓
┌─────────────────────────────────────────────────────────────┐
│  jeeves_avionics (L1 - Infrastructure Layer)                │
│  - LLM providers and factory                                │
│  - Database clients                                         │
│  - Memory services                                          │
│  - Settings and feature flags                               │
└─────────────────────────────────────────────────────────────┘
                              │ imports
                              ↓
┌─────────────────────────────────────────────────────────────┐
│  jeeves_control_tower (L1 - Kernel Layer)                   │
│  - Request lifecycle management                             │
│  - Resource quota enforcement                               │
│  - Service dispatch                                         │
│  - Event aggregation                                        │
└─────────────────────────────────────────────────────────────┘
                              │ imports
                              ↓
┌─────────────────────────────────────────────────────────────┐
│  jeeves_protocols, jeeves_shared (L0 - Foundation)          │
│  - Protocol definitions                                     │
│  - Type contracts                                           │
│  - Shared utilities                                         │
└─────────────────────────────────────────────────────────────┘
```

---

## Import Boundaries

### Allowed Imports for Capabilities

```python
# From jeeves_protocols (L0) - Protocol definitions and types
from jeeves_protocols import (
    # Enums
    RiskLevel, ToolAccess, ToolCategory, OperationStatus,
    TerminalReason, LoopVerdict, RiskApproval,

    # Configuration
    AgentConfig, PipelineConfig, RoutingRule, ContextBounds,

    # Envelope
    GenericEnvelope, ProcessingRecord,

    # Agent Runtime
    UnifiedAgent, UnifiedRuntime, create_runtime_from_config,
    create_generic_envelope, LLMProviderFactory,

    # Protocols
    LoggerProtocol, LLMProviderProtocol, ToolRegistryProtocol,
    ToolExecutorProtocol, SettingsProtocol, FeatureFlagsProtocol,

    # Capability Registration
    CapabilityResourceRegistry, CapabilityModeConfig,
    get_capability_resource_registry,

    # Memory
    WorkingMemory, Finding, FocusType,

    # Utilities
    JSONRepairKit, normalize_string_list,
)

# From jeeves_shared (L0) - Shared utilities
from jeeves_shared import (
    uuid_str, uuid_read, get_component_logger,
    to_json, from_json, utc_now,
)

# From jeeves_mission_system (L2) - Framework APIs
from jeeves_mission_system.contracts import (
    LoggerProtocol, ContextBounds, WorkingMemory,
    get_config_registry, ConfigKeys,
    StandardToolResult, ToolErrorDetails,
)

# NOTE: ToolId is CAPABILITY-OWNED - import from your capability's tools/catalog.py
# from my_capability.tools.catalog import ToolId  # Capability-owned enum

from jeeves_mission_system.adapters import (
    get_logger, get_settings, get_feature_flags,
    MissionSystemAdapters,
    create_database_client,
    create_event_emitter, create_embedding_service,
    create_nli_service, create_vector_adapter,
)

from jeeves_mission_system.config import (
    AgentProfile, LLMProfile, ThresholdProfile,
    ConfigRegistry, get_config_registry,
)

# From jeeves_avionics (L1) - Infrastructure (prefer adapters)
from jeeves_avionics.capability_registry import (
    CapabilityLLMConfigRegistry, get_capability_registry,
)

from jeeves_avionics.logging import (
    create_logger, create_capability_logger,
)
```

### Forbidden Imports

```python
# NEVER import from coreengine/ (Go package)
# from coreengine import ...  # FORBIDDEN

# NEVER import core internals that aren't exported
# from jeeves_mission_system.internal import ...  # FORBIDDEN
```

---

## Core Protocols

### LLMProviderProtocol

```python
class LLMProviderProtocol(Protocol):
    """LLM provider interface."""

    async def generate(
        self,
        prompt: str,
        agent_role: str = "default",
        temperature: float = 0.7,
        max_tokens: int = 2048,
    ) -> str:
        """Generate text completion."""
        ...

    async def generate_structured(
        self,
        prompt: str,
        schema: dict,
        agent_role: str = "default",
    ) -> dict:
        """Generate structured JSON output."""
        ...
```

### ToolExecutorProtocol

```python
class ToolExecutorProtocol(Protocol):
    """Tool execution interface."""

    async def execute(
        self,
        tool_name: str,
        params: dict,
    ) -> dict:
        """Execute a tool by name."""
        ...

    def has_tool(self, name: str) -> bool:
        """Check if tool exists."""
        ...

    def list_tools(self) -> list[str]:
        """List available tools."""
        ...
```

### ToolRegistryProtocol

```python
class ToolRegistryProtocol(Protocol):
    """Tool registry interface."""

    def register(
        self,
        tool_id: str,
        handler: Callable,
        description: str,
        parameters: dict,
        category: ToolCategory,
        risk_level: RiskLevel,
    ) -> None:
        """Register a tool."""
        ...
```

---

## Registration Contract

Capabilities MUST implement a registration function:

```python
# my_capability/wiring.py
from jeeves_protocols import (
    get_capability_resource_registry,
    CapabilityModeConfig,
    AgentLLMConfig,
)
from jeeves_avionics.capability_registry import get_capability_registry

def register_capability():
    """Register capability with jeeves-core."""

    # 1. Register capability mode
    resource_registry = get_capability_resource_registry()
    resource_registry.register_mode(
        "my_capability",
        CapabilityModeConfig(
            name="my_capability",
            description="My capability description",
            default_pipeline="standard",
        )
    )

    # 2. Register agent LLM configurations
    llm_registry = get_capability_registry()
    for agent_name, config in MY_AGENT_CONFIGS.items():
        llm_registry.register(
            capability_id="my_capability",
            agent_name=agent_name,
            config=config,
        )

    # 3. Register tools
    register_tools()
```

### Capability Bootstrap Order

```python
# In your capability's entry point (e.g., server.py)
async def bootstrap():
    # 1. Register capability resources FIRST
    from my_capability.wiring import register_capability
    register_capability()

    # 2. Then import and use runtime services
    from jeeves_mission_system.adapters import get_logger
    logger = get_logger()

    # 3. Start your application
    await start_server()
```

---

## Tool Contract

### Tool Registration

Capabilities define their own `ToolCatalog` and `ToolId` enum. The avionics layer provides
generic `ToolExecutor` that works with string tool names.

```python
# In your capability's tools/catalog.py
from enum import Enum
from jeeves_protocols import ToolCategory, RiskLevel

class ToolId(str, Enum):
    """Capability-owned tool identifiers."""
    LOCATE = "locate"
    READ_CODE = "read_code"
    # ... your tools

# Tool registration - capability owns the catalog
from my_capability.tools.catalog import ToolCatalog

catalog = ToolCatalog.get_instance()

@catalog.register(
    tool_id="my_capability.my_tool",  # Namespaced ID
    description="What this tool does",
    parameters={
        "param1": "string",
        "param2": "int?",  # Optional
    },
    category=ToolCategory.COMPOSITE,  # or STANDALONE, RESILIENT
    risk_level=RiskLevel.LOW,  # or MEDIUM, HIGH
)
async def my_tool(param1: str, param2: int = None) -> dict:
    """Tool implementation."""
    return {
        "status": "success",
        "data": {...},
        "citations": [...]  # If applicable
    }
```

**Key Architecture Decision:** `ToolId` enums are CAPABILITY-OWNED, not defined in
avionics or mission_system. This allows each capability to define its own tool set
without modifying core layers.

### Tool Categories

| Category | Description |
|----------|-------------|
| `COMPOSITE` | Multi-step tools with fallback strategies |
| `RESILIENT` | Wrapped base tools with retry logic |
| `STANDALONE` | Simple, single-operation tools |
| `UNIFIED` | High-level unified entry points |

### Tool Result Contract

```python
@dataclass
class StandardToolResult:
    status: str  # "success", "error", "partial"
    data: Any = None
    error: Optional[str] = None
    citations: list[dict] = field(default_factory=list)
    attempt_history: list[dict] = field(default_factory=list)
```

---

## Agent Contract

### Agent Configuration

```python
from jeeves_protocols import AgentConfig, RoutingRule, ToolAccess

agent_config = AgentConfig(
    name="my_agent",
    role="planner",  # or "critic", "traverser", etc.
    capabilities={"planning", "tool_selection"},
    tool_access=ToolAccess.FULL,  # or LIMITED, NONE
    routing_rules=[
        RoutingRule(
            condition="verdict == 'approved'",
            next_agent="integration",
        ),
        RoutingRule(
            condition="verdict == 'loop_back'",
            next_agent="stage_a",
        ),
    ],
)
```

### Agent Implementation Pattern

```python
from jeeves_protocols import UnifiedAgent, GenericEnvelope

class MyAgent(UnifiedAgent):
    """Custom agent implementation."""

    async def run(
        self,
        envelope: GenericEnvelope,
        context: RunContext,
    ) -> AgentResult:
        # 1. Get previous agent outputs
        prev_output = envelope.outputs.get("previous_agent")

        # 2. Execute logic (with LLM if needed)
        response = await context.llm.generate(
            prompt=self.build_prompt(prev_output),
            agent_role=self.role,
        )

        # 3. Return result with state delta
        return AgentResult(
            delta=StateDelta(
                output_key=self.name,
                output_value=MyOutput(**parsed_response),
            )
        )
```

---

## Memory Contract

### Four-Layer Memory Architecture

| Layer | Name | Scope | Storage |
|-------|------|-------|---------|
| L1 | Episodic | Per-request | In-memory |
| L2 | Event Log | Permanent | PostgreSQL |
| L3 | Working Memory | Per-session | PostgreSQL |
| L4 | Persistent Cache | Permanent | PostgreSQL + pgvector |

### Working Memory Interface

```python
from jeeves_protocols import WorkingMemory, Finding, FocusType

# Create working memory
memory = WorkingMemory(
    focus=FocusState(
        focus_type=FocusType.EXPLORATION,
        current_entities=[],
    ),
    findings=[],
    context_summary="",
)

# Add findings
memory.findings.append(Finding(
    content="Found relevant code",
    citations=[{"file": "main.py", "line": 42}],
    confidence=0.9,
))

# Merge memories (for multi-stage)
merged = merge_working_memory(memory1, memory2)
```

---

## Event Contract

### Event Types

```python
from jeeves_mission_system.orchestrator.agent_events import (
    AgentEventType, AgentEvent,
)

# Available event types
AgentEventType.AGENT_STARTED
AgentEventType.AGENT_COMPLETED
AgentEventType.TOOL_STARTED
AgentEventType.TOOL_COMPLETED
AgentEventType.STAGE_STARTED
AgentEventType.STAGE_COMPLETED
AgentEventType.ERROR
AgentEventType.CLARIFICATION_REQUESTED
```

### Event Emission

```python
from jeeves_mission_system.orchestrator.event_context import AgentEventContext

async def my_agent_logic(event_context: AgentEventContext):
    # Emit agent started
    await event_context.emit_agent_started("my_agent")

    # Emit tool execution
    await event_context.emit_tool_started("my_tool", params={...})
    result = await execute_tool()
    await event_context.emit_tool_completed("my_tool", status="success")

    # Emit agent completed
    await event_context.emit_agent_completed("my_agent", status="success")
```

---

## Configuration Contract

### Context Bounds

```python
from jeeves_protocols import ContextBounds

bounds = ContextBounds(
    max_input_tokens=4096,
    max_output_tokens=2048,
    max_iterations=3,
    max_context_tokens=8192,
    max_llm_calls=10,
    max_agent_hops=21,
)
```

### Agent Profiles

```python
from jeeves_mission_system.config import (
    AgentProfile, LLMProfile, ThresholdProfile,
)

profile = AgentProfile(
    role="planner",
    llm=LLMProfile(
        temperature=0.3,
        max_tokens=2500,
    ),
    thresholds=ThresholdProfile(
        clarification_threshold=0.7,
        approval_threshold=0.8,
    ),
    latency_budget_ms=45000,
)
```

### Config Registry

```python
from jeeves_mission_system.config import (
    get_config_registry, ConfigKeys,
)

# Register capability config
registry = get_config_registry()
registry.register(ConfigKeys.DOMAIN_CONFIG, my_domain_config)

# Retrieve config
config = registry.get(ConfigKeys.DOMAIN_CONFIG)
```

---

## Quick Start

### Minimal Capability Structure

```
my_capability/
├── __init__.py
├── wiring.py          # register_capability()
├── config/
│   ├── __init__.py
│   ├── agents.py      # Agent LLM configs
│   └── tools.py       # Tool access matrix
├── agents/
│   ├── __init__.py
│   └── my_agent.py    # Agent implementations
├── tools/
│   ├── __init__.py
│   └── my_tools.py    # Tool implementations
└── server.py          # Entry point
```

### Minimal wiring.py

```python
from jeeves_protocols import (
    get_capability_resource_registry,
    CapabilityModeConfig,
)
from jeeves_avionics.capability_registry import get_capability_registry
from my_capability.config.agents import AGENT_LLM_CONFIGS
from my_capability.tools import register_tools

def register_capability():
    """Register my_capability with jeeves-core."""

    # Register mode
    resource_registry = get_capability_resource_registry()
    resource_registry.register_mode(
        "my_capability",
        CapabilityModeConfig(
            name="my_capability",
            description="My awesome capability",
        )
    )

    # Register agent configs
    llm_registry = get_capability_registry()
    for agent_name, config in AGENT_LLM_CONFIGS.items():
        llm_registry.register(
            capability_id="my_capability",
            agent_name=agent_name,
            config=config,
        )

    # Register tools
    register_tools()
```

### Minimal server.py

```python
import asyncio
from my_capability.wiring import register_capability

async def main():
    # 1. Bootstrap capability
    register_capability()

    # 2. Import runtime after registration
    from jeeves_mission_system.adapters import get_logger
    logger = get_logger()
    logger.info("Capability registered")

    # 3. Start your server
    # ...

if __name__ == "__main__":
    asyncio.run(main())
```

---

## Docker Configuration

### Repository Structure

```
jeeves-core/
├── cmd/
│   └── envelope/              # Go CLI binary source
│       └── main.go
├── commbus/                   # Go: Message bus (L0)
├── coreengine/                # Go: Core engine (L0)
│   ├── agents/
│   ├── config/
│   ├── envelope/
│   ├── runtime/
│   └── tools/
├── jeeves_protocols/          # Python: L0 Foundation - Type contracts
├── jeeves_shared/             # Python: L0 Foundation - Shared utilities
├── jeeves_avionics/           # Python: L1 Infrastructure
│   ├── gateway/               # HTTP gateway (FastAPI)
│   ├── llm/                   # LLM providers
│   ├── database/              # Database clients
│   └── logging/               # Logging module
├── jeeves_control_tower/      # Python: L1 Kernel - Request lifecycle
├── jeeves_memory_module/      # Python: Memory services
├── jeeves_mission_system/     # Python: L2 Application
│   ├── api/                   # REST API (server.py)
│   ├── orchestrator/          # Orchestration framework
│   └── proto/                 # gRPC proto files
├── docker/
│   ├── Dockerfile             # Multi-stage build
│   ├── docker-compose.yml     # Service orchestration
│   └── init-db.sql            # PostgreSQL initialization
├── tests/                     # Root-level tests
├── go.mod                     # Go module definition
├── go.sum                     # Go dependencies
├── Makefile                   # Build and test targets
└── .env                       # Environment configuration
```

### Docker Targets

The Dockerfile provides three targets:

| Target | Image | Entry Point | Purpose |
|--------|-------|-------------|---------|
| `gateway` | `jeeves-gateway:latest` | `jeeves_avionics.gateway.main:app` | HTTP gateway |
| `orchestrator` | `jeeves-core:latest` | `jeeves_mission_system.api.server:app` | Full runtime |
| `test` | `jeeves-core:test` | `pytest` | Test runner |

### Building Images

```bash
# Build all images
make docker-build

# Build specific targets
docker build -t jeeves-gateway:latest -f docker/Dockerfile --target gateway .
docker build -t jeeves-core:latest -f docker/Dockerfile --target orchestrator .
docker build -t jeeves-core:test -f docker/Dockerfile --target test .
```

### Running Services

```bash
# Start all services (PostgreSQL, llama-server, orchestrator)
docker compose -f docker/docker-compose.yml up -d

# Start infrastructure only
docker compose -f docker/docker-compose.yml up -d postgres llama-server

# Run tests in Docker
docker compose -f docker/docker-compose.yml run --rm test pytest -v

# View logs
docker compose -f docker/docker-compose.yml logs -f

# Stop all services
docker compose -f docker/docker-compose.yml down
```

### Environment Configuration

The `.env` file at repository root configures all services:

```bash
# Database
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_DATABASE=assistant
POSTGRES_USER=assistant
POSTGRES_PASSWORD=dev_password_change_in_production

# LLM Provider
LLM_PROVIDER=llamaserver
LLAMASERVER_HOST=http://localhost:8080
DEFAULT_MODEL=qwen2.5-3b-instruct
LLAMA_CTX_SIZE=16384

# API Server
API_HOST=0.0.0.0
API_PORT=8000
LOG_LEVEL=DEBUG
```

### Using jeeves-core as a Submodule

When using `jeeves-core` as a git submodule in a capability repository:

```bash
# Initialize submodule
git submodule update --init --recursive

# Verify structure
ls jeeves-core/
```

Capability Dockerfile paths should reference the submodule:

```dockerfile
# Copy from submodule
COPY jeeves-core/jeeves_protocols/ ./jeeves_protocols/
COPY jeeves-core/jeeves_shared/ ./jeeves_shared/
COPY jeeves-core/jeeves_avionics/ ./jeeves_avionics/
COPY jeeves-core/jeeves_mission_system/ ./jeeves_mission_system/
```

### Required Services

| Service | Port | Purpose |
|---------|------|---------|
| PostgreSQL (pgvector) | 5432 | Database with vector extension |
| llama-server | 8080 | Local LLM inference |
| Gateway | 8000 | HTTP API gateway |
| Orchestrator | 8000, 50051 | REST + gRPC endpoints |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.3 | 2026-01-06 | ToolId is now CAPABILITY-OWNED; removed ToolId/ToolCatalog from contracts_core exports |
| 1.2 | 2025-12-16 | Updated adapters exports (removed deprecated factory), added doc references |
| 1.1 | 2025-12-13 | Added Docker configuration section |
| 1.0 | 2025-12-13 | Initial contract document |

---

*This contract is the authoritative source for capability integration. For implementation details, see [docs/CAPABILITY_LAYER_INTEGRATION.md](docs/CAPABILITY_LAYER_INTEGRATION.md). For internal core contracts, see [docs/CONTRACTS.md](docs/CONTRACTS.md).*
