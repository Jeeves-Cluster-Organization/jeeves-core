# jeeves_protocols Package

**Layer**: L0 (Foundation Layer - Zero Dependencies on Other Jeeves Packages)

## Overview

The `jeeves_protocols` package provides the canonical type definitions, protocols, and data classes that form the contract between all layers in the Jeeves architecture. This is the foundation layer that all other Jeeves packages depend on.

## Constitutional Authority

- All layers import from `jeeves_protocols` for type definitions
- `jeeves_protocols` sits at L0 (no dependencies on other Jeeves packages)
- Protocols define interfaces; implementations are in other packages

## Package Structure

| Module | Purpose |
|--------|---------|
| [enums.py](enums.md) | Core enums (RiskLevel, ToolAccess, OperationStatus, etc.) |
| [config.py](config.md) | Configuration types (AgentConfig, PipelineConfig, etc.) |
| [envelope.py](envelope.md) | Envelope and ProcessingRecord |
| [agents.py](agents.md) | Agent, Runtime, factories |
| [protocols.py](protocols.md) | Protocol definitions (LoggerProtocol, etc.) |
| [capability.py](capability.md) | CapabilityResourceRegistry for dynamic registration |
| [memory.py](memory.md) | WorkingMemory, Finding, and memory operations |
| [events.py](events.md) | Unified event schema (Event, EventCategory, EventSeverity) |
| [interrupts.py](interrupts.md) | Flow interrupts and rate limiting types |
| [grpc_client.py](grpc.md) | gRPC client for Go runtime |
| [utils.py](utils.md) | JSON utilities (JSONRepairKit, normalize_string_list) |

## Import Guidelines

### Recommended Imports for Capability Layers

```python
from jeeves_protocols import (
    # Core Enums
    RiskLevel,
    ToolAccess,
    ToolCategory,
    OperationStatus,
    HealthStatus,
    TerminalReason,
    LoopVerdict,
    RiskApproval,
    OperationResult,
    
    # Configuration
    AgentConfig,
    PipelineConfig,
    RoutingRule,
    EdgeLimit,
    JoinStrategy,
    ContextBounds,
    ExecutionConfig,
    OrchestrationFlags,
    
    # Envelope
    Envelope,
    ProcessingRecord,
    
    # Agent Runtime
    Agent,
    AgentCapability,
    Runtime,
    create_pipeline_runner,
    create_envelope,
    LLMProvider,
    ToolExecutor,
    Logger,
    Persistence,
    PromptRegistry,
    EventContext,
    
    # Protocols
    LoggerProtocol,
    PersistenceProtocol,
    LLMProviderProtocol,
    ToolProtocol,
    ToolRegistryProtocol,
    ToolExecutorProtocol,
    InferenceEndpoint,
    AgentLLMConfig,
    
    # Capability Registration
    CapabilityResourceRegistry,
    get_capability_resource_registry,
)
```

### Quick Start Example

```python
from jeeves_protocols import (
    Envelope,
    create_envelope,
    RiskLevel,
    OperationResult,
)

# Create an envelope for pipeline processing
envelope = create_envelope(
    raw_input="Analyze the authentication module",
    user_id="user-123",
    session_id="session-456",
)

# Check risk level
if RiskLevel.requires_confirmation(RiskLevel.DESTRUCTIVE):
    # Handle high-risk operation
    pass

# Return operation result
result = OperationResult.success({"items": ["file1.py", "file2.py"]})
if result.is_success:
    process_data(result.data)
```

## Module Documentation

- **[Enums](enums.md)** - Enums and base types for risk levels, tool access, operation status
- **[Configuration](config.md)** - Pipeline and agent configuration dataclasses
- **[Envelope](envelope.md)** - Envelope for pipeline state management
- **[Agents](agents.md)** - Agent and Runtime for agent execution
- **[Protocols](protocols.md)** - Protocol definitions for dependency injection
- **[Capability](capability.md)** - Dynamic capability registration system
- **[Memory](memory.md)** - Working memory types and operations
- **[Events](events.md)** - Unified event schema for pub/sub
- **[Interrupts](interrupts.md)** - Flow control interrupts and rate limiting
- **[gRPC](grpc.md)** - Go runtime gRPC client
- **[Utilities](utils.md)** - JSON repair and string utilities

## Design Principles

1. **Zero Dependencies**: This package has no dependencies on other Jeeves packages
2. **Protocol-First**: Interfaces defined as `typing.Protocol` for static type checking
3. **Immutable Where Possible**: Frozen dataclasses for thread safety
4. **Go Compatibility**: Types mirror Go definitions for cross-language interop
5. **Constitutional Compliance**: Follows architectural principles from CONSTITUTION.md
