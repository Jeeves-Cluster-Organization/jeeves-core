# jeeves_mission_system.contracts - Contract Definitions

**Package:** `jeeves_mission_system.contracts`  
**Purpose:** Type contracts and protocol re-exports for capability integration  
**Updated:** 2026-01-23

---

## Overview

The `contracts` package re-exports core types from `jeeves_protocols` and provides the stable API that capabilities build upon. This is the **canonical import location** for capabilities.

### Architecture

```
Capabilities → mission_system.contracts (types only)
            → mission_system.adapters (infrastructure)
```

---

## Module Index

| Module | Description |
|--------|-------------|
| `__init__.py` | Re-exports from `contracts_core.py` |
| `contracts_core.py` | Main contract definitions and re-exports |

---

## Key Exports

### Centralized Agent Architecture

From `jeeves_protocols`:

```python
# Agent configuration
AgentConfig        # Declarative agent definition
PipelineConfig     # Pipeline with ordered agents
RoutingRule        # Conditional routing rules
ToolAccess         # Agent tool access levels
AgentCapability    # Agent capability declarations

# Unified agent
Agent       # Single agent class driven by configuration

# Generic envelope
Envelope    # Dynamic output slots (outputs dict)
ProcessingRecord   # Agent processing record
create_envelope  # Factory function

# Unified runtime
Runtime            # Executes pipelines from config
create_pipeline_runner  # Factory function

# Enums
TerminalReason     # Termination reasons
LoopVerdict        # Critic verdicts (APPROVED, REINTENT, NEXT_STAGE)
RiskApproval       # Risk approval levels
```

### Protocols

From `jeeves_protocols`:

```python
# App context protocols
SettingsProtocol
FeatureFlagsProtocol
AppContextProtocol
LoggerProtocol
ClockProtocol

# Infrastructure protocols
PersistenceProtocol
LLMProviderProtocol
ToolRegistryProtocol
IntentParsingProtocol
ClaimVerificationProtocol

# Core protocols
ToolProtocol
EventBusProtocol
MemoryServiceProtocol
IdGeneratorProtocol
ToolExecutorProtocol
ConfigRegistryProtocol

# Risk level
RiskLevel
```

### Working Memory

```python
WorkingMemory      # Base class for session state
Finding            # Individual finding/result
```

### Configuration

```python
ContextBounds      # Resource limits (access via AppContext)
```

### JSON Utilities

```python
JSONRepairKit      # JSON parsing with repair
normalize_string_list  # List normalization
```

### Config Registry (from mission_system)

```python
ConfigRegistry     # Config injection registry
ConfigKeys         # Standard config key constants
get_config_registry
set_config_registry
```

### Agent Profiles (from mission_system)

```python
LLMProfile         # Per-agent LLM configuration
ThresholdProfile   # Per-agent confidence thresholds
AgentProfile       # Complete agent profile
get_agent_profile
get_llm_profile
get_thresholds
get_latency_budget
```

### Tool Types

```python
ToolCategory       # Tool categorization (from jeeves_protocols)
```

**Note:** `ToolId` and `ToolCatalog` are **capability-owned** and NOT re-exported here.

---

## Usage Pattern

### Capability Import

```python
# ✅ CORRECT - Import from contracts
from jeeves_mission_system.contracts import (
    AgentConfig,
    PipelineConfig,
    Envelope,
    Runtime,
    LLMProviderProtocol,
    ToolProtocol,
)

# ❌ INCORRECT - Do not import from core_engine directly
from jeeves_core_engine import AgentConfig  # WRONG
```

### Creating a Runtime

```python
from jeeves_mission_system.contracts import (
    PipelineConfig,
    AgentConfig,
    Runtime,
    create_pipeline_runner,
)

# Define pipeline
config = PipelineConfig(
    agents=[
        AgentConfig(name="perception", stage_order=0),
        AgentConfig(name="intent", stage_order=1),
        AgentConfig(name="planner", stage_order=2),
        # ...
    ],
    routing_rules=[...],
)

# Create runtime
runtime = create_pipeline_runner(config, llm_factory, tool_executor)

# Execute pipeline
result = await runtime.run(envelope)
```

### Working with Envelope

```python
from jeeves_mission_system.contracts import (
    Envelope,
    create_envelope,
)

# Create envelope
envelope = create_envelope(
    user_message="How does authentication work?",
    user_id="user123",
    session_id="session456",
)

# Access outputs
intent = envelope.outputs.get("intent")
plan = envelope.outputs.get("plan")
```

---

## Removed Types (v4.0)

The following types have been removed in the centralized architecture:

```python
# ❌ No longer exported - use AgentConfig instead
Agent, AgentResult, StateDelta, AgentStage
CoreEnvelope, EnvelopeStage
PerceptionOutput, IntentOutput, PlanOutput, etc.

# ❌ No longer exported - context flows through Envelope
AgentContext

# ❌ No longer exported - use Runtime + PipelineConfig
AgentRuntime
```

---

## Full Export List

```python
__all__ = [
    # Centralized Agent Architecture
    "AgentConfig", "PipelineConfig", "RoutingRule", "ToolAccess", "AgentCapability",
    "Agent",
    "Envelope", "ProcessingRecord", "create_envelope",
    "Runtime", "create_pipeline_runner",
    "TerminalReason", "LoopVerdict", "RiskApproval",
    
    # Working memory
    "WorkingMemory", "Finding",
    
    # Canonical Protocols
    "SettingsProtocol", "FeatureFlagsProtocol", "AppContextProtocol",
    "LoggerProtocol", "ClockProtocol",
    "PersistenceProtocol", "LLMProviderProtocol", "ToolRegistryProtocol",
    "IntentParsingProtocol", "ClaimVerificationProtocol", "RiskLevel",
    
    # Core protocols
    "ToolProtocol", "EventBusProtocol", "MemoryServiceProtocol",
    "IdGeneratorProtocol", "ToolExecutorProtocol", "ConfigRegistryProtocol",
    
    # Configuration
    "ContextBounds",
    
    # JSON utilities
    "JSONRepairKit", "normalize_string_list",
    
    # Config Registry
    "ConfigRegistry", "ConfigKeys", "get_config_registry", "set_config_registry",
    
    # Agent Profiles
    "LLMProfile", "ThresholdProfile", "AgentProfile",
    "get_agent_profile", "get_llm_profile", "get_thresholds", "get_latency_budget",
    
    # Tool Types
    "ToolCategory",
]
```

---

## Navigation

- [← Back to README](README.md)
- [← Config Documentation](config.md)
- [→ Events Documentation](events.md)
