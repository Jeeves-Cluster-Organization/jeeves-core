# Capability Layer Integration Guide

This document describes how capability layer repositories integrate with `jeeves-core` when used as a git submodule. See [CONTRACT.md](../CONTRACT.md) for the authoritative capability integration contract.

## Package Import Hierarchy

Capability layers follow this import boundary pattern:

```
capability → mission_system → avionics → protocols
                                ↓
                            jeeves_shared (L0)
```

**Important:** Capabilities MUST NOT import directly from `coreengine/` (Go package).

---

## 1. jeeves_protocols Exports

The `jeeves_protocols` package provides all core type contracts. Import directly:

```python
from jeeves_protocols import (
    # Core Enums
    RiskLevel, ToolAccess, ToolCategory, OperationStatus, HealthStatus,
    TerminalReason, CriticVerdict, RiskApproval, OperationResult,

    # Configuration
    AgentConfig, PipelineConfig, RoutingRule, ContextBounds,
    CoreConfig, OrchestrationFlags,

    # Envelope
    GenericEnvelope, ProcessingRecord,

    # Agent Runtime
    UnifiedAgent, AgentCapability, UnifiedRuntime,
    create_runtime_from_config, create_generic_envelope,
    LLMProviderFactory, PreProcessHook, PostProcessHook,

    # Protocols
    LoggerProtocol, PersistenceProtocol, DatabaseClientProtocol,
    LLMProviderProtocol, ToolRegistryProtocol, ToolExecutorProtocol,
    SettingsProtocol, FeatureFlagsProtocol, AppContextProtocol,
    NodeProfile, NodeProfilesProtocol, AgentLLMConfig,
    CapabilityLLMConfigRegistryProtocol, AgentToolAccessProtocol,

    # Capability Registration
    CapabilityResourceRegistry, CapabilityModeConfig,
    get_capability_resource_registry, reset_capability_resource_registry,

    # Memory Types
    WorkingMemory, Finding, FocusType, EntityRef, FocusState,
    create_working_memory, merge_working_memory,

    # Utilities
    JSONRepairKit, normalize_string_list, truncate_string,
)
```

### Key Types for Capabilities

| Type | Purpose |
|------|---------|
| `AgentConfig` | Configuration for individual agents |
| `PipelineConfig` | Pipeline configuration including routing rules |
| `GenericEnvelope` | Request/response envelope for agent communication |
| `UnifiedRuntime` | Runtime environment for agent execution |
| `CapabilityResourceRegistry` | Dynamic registration of capability resources |
| `WorkingMemory` | Ephemeral L1 memory within a single request |
| `LoggerProtocol` | Logging interface (dependency injection) |
| `AgentLLMConfig` | Per-agent LLM configuration |

---

## 2. jeeves_mission_system Exports

### contracts Module

```python
from jeeves_mission_system.contracts import (
    # Re-exported from jeeves_protocols
    LoggerProtocol, ContextBounds, WorkingMemory, RiskLevel,
    PersistenceProtocol, ToolCategory,

    # Tool catalog
    tool_catalog, ToolId,

    # Config registry
    get_config_registry, ConfigKeys,

    # Tool result types
    StandardToolResult, ToolErrorDetails,
)
```

### contracts_core Module

```python
from jeeves_mission_system.contracts_core import (
    # Re-exported from jeeves_protocols
    AgentConfig, PipelineConfig, GenericEnvelope, UnifiedRuntime,
    ContextBounds, WorkingMemory, RiskLevel, ToolCategory,
    OperationStatus, ToolAccess, ToolId,
)
```

### adapters Module

```python
from jeeves_mission_system.adapters import (
    # Logging facade
    get_logger,

    # Settings facade
    get_settings, get_feature_flags,

    # Adapter class
    MissionSystemAdapters,

    # Factory functions
    create_database_client,
    create_llm_provider_factory,
    create_event_emitter,
    create_graph_repository,
    create_embedding_service,
    create_code_indexer,
    create_nli_service,
    create_vector_adapter,
)
```

### orchestrator Module

```python
from jeeves_mission_system.orchestrator.state import (
    JeevesState, create_initial_state,
)

from jeeves_mission_system.orchestrator.agent_events import (
    AgentEvent, AgentEventType, AgentEventEmitter,
)
```

### prompts Module

```python
from jeeves_mission_system.prompts.core.registry import (
    register_prompt, PromptRegistry,
)
```

### config Module

```python
from jeeves_mission_system.config.agent_profiles import (
    LLMProfile, ThresholdProfile, AgentProfile,
    get_agent_profile, get_llm_profile, get_thresholds, get_latency_budget,
)

from jeeves_mission_system.config.registry import (
    ConfigRegistry, ConfigKeys,
    get_config_registry, set_config_registry, reset_config_registry,
)
```

---

## 3. jeeves_avionics Exports

### logging Module

```python
from jeeves_avionics.logging import (
    # Configuration
    configure_logging, configure_from_flags,

    # Logger creation
    create_logger, create_agent_logger, create_capability_logger,
    create_tool_logger, get_component_logger,

    # Context
    get_current_logger, set_current_logger,
    get_request_context, set_request_context, request_scope,

    # Types
    JeevesLogger, Span,

    # Tracing
    trace_agent, trace_tool, get_current_span,

    # Adapters
    StructlogAdapter, create_structlog_adapter,
)
```

### settings Module

```python
from jeeves_avionics.settings import (
    Settings, get_settings,
)
```

### capability_registry Module

```python
from jeeves_avionics.capability_registry import (
    CapabilityLLMConfigRegistry,
    get_capability_registry, set_capability_registry, reset_capability_registry,
)
```

### llm Module

```python
from jeeves_avionics.llm.factory import (
    create_llm_provider, create_agent_provider,
    create_agent_provider_with_node_awareness, LLMFactory,
)

from jeeves_avionics.llm.providers import (
    LLMProvider, MockProvider, LlamaServerProvider,
    OpenAIProvider, AnthropicProvider, AzureAIFoundryProvider,
)
```

### database Module

```python
from jeeves_avionics.database.client import (
    DatabaseClientProtocol, create_database_client,
)
```

### wiring Module

```python
from jeeves_avionics.wiring import (
    ToolExecutor, AgentContext,
    create_tool_executor, create_llm_provider_factory,
    get_database_client,
)
```

---

## 4. jeeves_shared Exports

```python
from jeeves_shared import (
    # UUID utilities
    uuid_str, uuid_read, convert_uuids_to_strings, UUIDStr,

    # Logging
    get_current_logger, create_logger, get_component_logger,

    # Serialization
    to_json, from_json, utc_now,
)

from jeeves_shared.uuid_utils import (
    uuid_str, uuid_read, convert_uuids_to_strings, UUIDStr, OptionalUUIDStr,
)

from jeeves_shared.logging import (
    configure_logging, create_logger, create_agent_logger,
    JeevesLogger, get_current_logger, request_scope,
)
```

---

## 5. Capability Registration Pattern

Capabilities MUST implement a registration function that registers their resources with the core registry.

### Example Registration

```python
# In capability's wiring.py or __init__.py
from jeeves_protocols import (
    get_capability_resource_registry,
    CapabilityModeConfig,
    AgentLLMConfig,
)
from jeeves_avionics.capability_registry import get_capability_registry

def register_capability():
    """Register capability resources. Replace 'my_capability' with your capability ID."""

    # 1. Register capability mode config
    resource_registry = get_capability_resource_registry()
    resource_registry.register_mode(
        "my_capability",  # Your unique capability identifier
        CapabilityModeConfig(
            name="my_capability",
            description="Description of your capability",
            # ... other config
        )
    )

    # 2. Register agent LLM configurations
    llm_registry = get_capability_registry()
    llm_registry.register(
        capability_id="my_capability",
        agent_name="planner",
        config=AgentLLMConfig(
            agent_name="planner",
            model="qwen2.5-7b-instruct-q4_k_m",
            temperature=0.3,
            timeout_seconds=60,
        )
    )
    llm_registry.register(
        capability_id="my_capability",
        agent_name="critic",
        config=AgentLLMConfig(
            agent_name="critic",
            model="qwen2.5-7b-instruct-q4_k_m",
            temperature=0.1,
            timeout_seconds=30,
        )
    )

    # 3. Register tools (via tool catalog)
    from jeeves_mission_system.contracts import tool_catalog
    from jeeves_protocols import ToolCategory, RiskLevel

    @tool_catalog.register(
        tool_id="my_capability.analyze",  # Namespaced tool ID
        description="Your tool description",
        parameters={"query": "string", "context": "dict?"},
        category=ToolCategory.UNIFIED,
        risk_level=RiskLevel.LOW,
    )
    async def analyze(query: str, context: dict = None):
        # Your implementation
        pass
```

---

## 6. Constitution Compliance Checklist

Per the capability layer CONSTITUTION:

| Requirement | Status | Notes |
|-------------|--------|-------|
| Capability can import from `jeeves_protocols` | ✅ | All types available |
| Capability can import from `jeeves_mission_system` | ✅ | All adapters available |
| Capability can import from `jeeves_avionics` | ⚠️ | Should prefer adapters |
| Capability MUST NOT import from `coreengine/` | ✅ | Go package isolation |
| Import boundary: capability → mission_system → avionics | ✅ | Enforced via adapters |

### Layer Rules

1. **L0 (jeeves_shared, jeeves_protocols)**: No dependencies on other Jeeves packages
2. **L1 (jeeves_avionics)**: Can import from L0
3. **L2 (jeeves_mission_system)**: Can import from L0 and L1
4. **L3 (capability layers)**: Can import from L0, L1, L2

---

## 7. Integration Testing

Verify capability integration with:

```python
# test_capability_integration.py
import pytest

def test_protocols_importable():
    """Verify all required protocols are importable."""
    from jeeves_protocols import (
        AgentConfig, PipelineConfig, GenericEnvelope,
        RiskLevel, ToolAccess, OperationStatus,
        CapabilityResourceRegistry, get_capability_resource_registry,
        NodeProfile, AgentLLMConfig
    )
    assert AgentConfig is not None
    assert get_capability_resource_registry() is not None

def test_mission_system_importable():
    """Verify mission system contracts are importable."""
    from jeeves_mission_system.contracts import (
        LoggerProtocol, ContextBounds, tool_catalog
    )
    from jeeves_mission_system.adapters import get_logger
    assert tool_catalog is not None

def test_avionics_importable():
    """Verify avionics services are importable."""
    from jeeves_avionics.logging import create_logger
    from jeeves_avionics.capability_registry import get_capability_registry
    assert create_logger is not None
    assert get_capability_registry() is not None

def test_shared_importable():
    """Verify shared utilities are importable."""
    from jeeves_shared.uuid_utils import uuid_str
    assert uuid_str("test") is not None
```

---

## 8. Dependencies

Required Python packages (specified in pyproject.toml):

```toml
[project.dependencies]
pydantic = ">=2.0"
structlog = ">=24.0"
# Additional runtime dependencies as needed
```

---

## Revision History

- **2025-12-13**: Initial documentation created
- **2025-12-13**: Fixed `jeeves_protocols/__init__.py` exports
