"""Jeeves Protocols Package - Core type contracts for all layers.

This package provides the canonical type definitions, protocols, and data classes
that form the contract between layers in the Jeeves architecture.

Constitutional Reference:
    - All layers import from jeeves_protocols for type definitions
    - jeeves_protocols sits at L0 (no dependencies on other Jeeves packages)
    - Protocols define interfaces; implementations are in other packages

Package Structure:
    - core.py: Core enums (RiskLevel, ToolAccess, OperationStatus, etc.)
    - config.py: Configuration types (AgentConfig, PipelineConfig, etc.)
    - envelope.py: GenericEnvelope and ProcessingRecord
    - agents.py: UnifiedAgent, Runtime, factories
    - protocols.py: Protocol definitions (LoggerProtocol, etc.)
    - capability.py: CapabilityResourceRegistry for dynamic registration
    - memory.py: WorkingMemory, Finding, and memory operations
    - events.py: Unified event schema (UnifiedEvent, EventCategory, EventSeverity)
    - utils.py: JSON utilities (JSONRepairKit, normalize_string_list)

Usage by Capability Layers:
    from jeeves_protocols import (
        AgentConfig, PipelineConfig, GenericEnvelope, Runtime,
        RiskLevel, ToolAccess, ToolCategory, OperationStatus,
        CapabilityResourceRegistry, get_capability_resource_registry,
        LoggerProtocol, NodeProfile, AgentLLMConfig
    )
"""

# =============================================================================
# CORE ENUMS AND TYPES
# =============================================================================
from jeeves_protocols.core import (
    # Risk and access levels
    RiskLevel,
    ToolAccess,
    ToolCategory,
    OperationStatus,
    HealthStatus,
    # Control flow enums
    TerminalReason,
    LoopVerdict,
    RiskApproval,
    # Operation result
    OperationResult,
)

# =============================================================================
# CONFIGURATION TYPES
# =============================================================================
from jeeves_protocols.config import (
    AgentConfig,
    PipelineConfig,
    RoutingRule,
    ContextBounds,
    CoreConfig,
    OrchestrationFlags,
)

# =============================================================================
# ENVELOPE TYPES
# =============================================================================
from jeeves_protocols.envelope import (
    GenericEnvelope,
    ProcessingRecord,
)

# =============================================================================
# AGENT RUNTIME
# =============================================================================
from jeeves_protocols.agents import (
    # Agents
    UnifiedAgent,
    AgentCapability,
    # Runtime
    Runtime,
    # Factories
    create_runtime_from_config,
    create_generic_envelope,
    # Protocols defined in agents.py
    LLMProvider,
    ToolExecutor,
    Logger,
    Persistence,
    PromptRegistry,
    EventContext,
    # Type aliases
    LLMProviderFactory,
    PreProcessHook,
    PostProcessHook,
    MockHandler,
    # Checkpoint
    OptionalCheckpoint,
)

# =============================================================================
# PROTOCOL DEFINITIONS
# =============================================================================
from jeeves_protocols.protocols import (
    # Request context
    RequestContext,
    # Core protocols
    LoggerProtocol,
    PersistenceProtocol,
    DatabaseClientProtocol,
    VectorStorageProtocol,
    LLMProviderProtocol,
    ToolProtocol,
    ToolDefinitionProtocol,
    ToolRegistryProtocol,
    ToolExecutorProtocol,
    # App context protocols
    SettingsProtocol,
    FeatureFlagsProtocol,
    ClockProtocol,
    AppContextProtocol,
    # Memory protocols
    MemoryServiceProtocol,
    SemanticSearchProtocol,
    SessionStateProtocol,
    SearchResult,
    # Event protocols
    EventBusProtocol,
    EventContextProtocol,
    # NLI protocol
    NLIServiceProtocol,
    # Checkpoint protocol
    CheckpointProtocol,
    CheckpointRecord,
    # Distributed protocols
    DistributedBusProtocol,
    DistributedTask,
    QueueStats,
    # Config registry
    ConfigRegistryProtocol,
    IdGeneratorProtocol,
    # Distributed node profiles
    NodeProfile,
    NodeProfilesProtocol,
    # Capability LLM configuration
    AgentLLMConfig,
    CapabilityLLMConfigRegistryProtocol,
    # Feature flags provider
    FeatureFlagsProviderProtocol,
    # Agent tool access
    AgentToolAccessProtocol,
    # Language config
    LanguageConfigProtocol,
)

# =============================================================================
# CAPABILITY REGISTRATION
# =============================================================================
from jeeves_protocols.capability import (
    # Tool catalog (capability-scoped)
    ToolCatalogEntry,
    ToolDefinition,
    CapabilityToolCatalog,
    # Config types
    CapabilityServiceConfig,
    CapabilityModeConfig,
    CapabilityAgentConfig,
    CapabilityPromptConfig,
    CapabilityToolsConfig,
    CapabilityOrchestratorConfig,
    CapabilityContractsConfig,
    # Registry protocol and implementation
    CapabilityResourceRegistryProtocol,
    CapabilityResourceRegistry,
    get_capability_resource_registry,
    reset_capability_resource_registry,
)

# =============================================================================
# MEMORY TYPES
# =============================================================================
from jeeves_protocols.memory import (
    # Types
    FocusType,
    EntityRef,
    FocusState,
    Finding,
    WorkingMemory,
    MemoryItem,
    ClarificationContext,
    # Factory functions
    create_working_memory,
    merge_working_memory,
    # Focus operations
    set_focus,
    clear_focus,
    # Entity operations
    add_entity_ref,
    get_recent_entities,
    # Clarification operations
    set_clarification,
    clear_clarification,
    # Serialization
    serialize_working_memory,
    deserialize_working_memory,
)

# =============================================================================
# UNIFIED EVENTS
# =============================================================================
from jeeves_protocols.events import (
    # Event schema
    UnifiedEvent,
    EventCategory,
    EventSeverity,
    StandardEventTypes,
    # Protocol
    EventEmitterProtocol,
)

# =============================================================================
# UTILITIES
# =============================================================================
from jeeves_protocols.utils import (
    JSONRepairKit,
    normalize_string_list,
    truncate_string,
)

# =============================================================================
# INTERRUPTS AND RATE LIMITING
# =============================================================================
from jeeves_protocols.interrupts import (
    # Interrupt types
    InterruptKind,
    InterruptStatus,
    InterruptResponse,
    FlowInterrupt,
    InterruptServiceProtocol,
    # Rate limit types
    RateLimitConfig,
    RateLimitResult,
    RateLimiterProtocol,
)

# =============================================================================
# PUBLIC API
# =============================================================================
__all__ = [
    # ─── Core Enums ───
    "RiskLevel",
    "ToolAccess",
    "ToolCategory",
    "OperationStatus",
    "HealthStatus",
    "TerminalReason",
    "LoopVerdict",
    "RiskApproval",
    "OperationResult",

    # ─── Configuration ───
    "AgentConfig",
    "PipelineConfig",
    "RoutingRule",
    "ContextBounds",
    "CoreConfig",
    "OrchestrationFlags",

    # ─── Envelope ───
    "GenericEnvelope",
    "ProcessingRecord",

    # ─── Agent Runtime ───
    "UnifiedAgent",
    "AgentCapability",
    "Runtime",
    "create_runtime_from_config",
    "create_generic_envelope",
    "LLMProvider",
    "ToolExecutor",
    "Logger",
    "Persistence",
    "PromptRegistry",
    "EventContext",
    "LLMProviderFactory",
    "PreProcessHook",
    "PostProcessHook",
    "MockHandler",
    "OptionalCheckpoint",

    # ─── Protocols ───
    "RequestContext",
    "LoggerProtocol",
    "PersistenceProtocol",
    "DatabaseClientProtocol",
    "VectorStorageProtocol",
    "LLMProviderProtocol",
    "ToolProtocol",
    "ToolDefinitionProtocol",
    "ToolRegistryProtocol",
    "ToolExecutorProtocol",
    "SettingsProtocol",
    "FeatureFlagsProtocol",
    "ClockProtocol",
    "AppContextProtocol",
    "MemoryServiceProtocol",
    "SemanticSearchProtocol",
    "SessionStateProtocol",
    "SearchResult",
    "EventBusProtocol",
    "EventContextProtocol",
    "NLIServiceProtocol",
    "CheckpointProtocol",
    "CheckpointRecord",
    "DistributedBusProtocol",
    "DistributedTask",
    "QueueStats",
    "ConfigRegistryProtocol",
    "IdGeneratorProtocol",
    "NodeProfile",
    "NodeProfilesProtocol",
    "AgentLLMConfig",
    "CapabilityLLMConfigRegistryProtocol",
    "FeatureFlagsProviderProtocol",
    "AgentToolAccessProtocol",
    "LanguageConfigProtocol",

    # ─── Capability Registration ───
    "ToolCatalogEntry",
    "ToolDefinition",
    "CapabilityToolCatalog",
    "CapabilityServiceConfig",
    "CapabilityModeConfig",
    "CapabilityAgentConfig",
    "CapabilityPromptConfig",
    "CapabilityToolsConfig",
    "CapabilityOrchestratorConfig",
    "CapabilityContractsConfig",
    "CapabilityResourceRegistryProtocol",
    "CapabilityResourceRegistry",
    "get_capability_resource_registry",
    "reset_capability_resource_registry",

    # ─── Memory Types ───
    "FocusType",
    "EntityRef",
    "FocusState",
    "Finding",
    "WorkingMemory",
    "MemoryItem",
    "ClarificationContext",
    "create_working_memory",
    "merge_working_memory",
    "set_focus",
    "clear_focus",
    "add_entity_ref",
    "get_recent_entities",
    "set_clarification",
    "clear_clarification",
    "serialize_working_memory",
    "deserialize_working_memory",

    # ─── Unified Events ───
    "UnifiedEvent",
    "EventCategory",
    "EventSeverity",
    "StandardEventTypes",
    "EventEmitterProtocol",

    # ─── Utilities ───
    "JSONRepairKit",
    "normalize_string_list",
    "truncate_string",

    # ─── Interrupts & Rate Limiting ───
    "InterruptKind",
    "InterruptStatus",
    "InterruptResponse",
    "FlowInterrupt",
    "InterruptServiceProtocol",
    "RateLimitConfig",
    "RateLimitResult",
    "RateLimiterProtocol",
]
