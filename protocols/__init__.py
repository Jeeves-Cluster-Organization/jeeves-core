"""Jeeves Protocols Package - Type contracts for all layers.

This package provides protocol definitions and data classes.
Core types are in jeeves_core.types (Go kernel source of truth):
    - Enums: RiskLevel, ToolAccess, TerminalReason, etc.
    - Envelope: Envelope, ProcessingRecord, PipelineEvent
    - Interrupts: InterruptKind, InterruptStatus, FlowInterrupt, etc.
    - Config: AgentConfig, PipelineConfig, ExecutionConfig, etc.

Package Structure:
    - agents.py: Agent, PipelineRunner, factories
    - protocols.py: Protocol definitions (LoggerProtocol, etc.)
    - capability.py: CapabilityResourceRegistry for dynamic registration
    - memory.py: WorkingMemory, Finding, and memory operations
    - events.py: Event schema (Event, EventCategory, EventSeverity)
    - utils.py: JSON utilities (JSONRepairKit, normalize_string_list)

Usage:
    from jeeves_core.types import RiskLevel, ToolAccess, TerminalReason
    from jeeves_core.types import Envelope, ProcessingRecord
    from jeeves_core.types import InterruptKind, FlowInterrupt
    from jeeves_core.types import AgentConfig, PipelineConfig, ExecutionConfig
    from protocols import LoggerProtocol
"""

# =============================================================================
# CONFIGURATION TYPES - Now in jeeves_core.types
# =============================================================================
# from jeeves_core.types import (
#     AgentConfig, PipelineConfig, RoutingRule, EdgeLimit, JoinStrategy,
#     ContextBounds, ExecutionConfig, OrchestrationFlags, RunMode, etc.
# )

# =============================================================================
# ENVELOPE TYPES - Now in jeeves_core.types
# =============================================================================
# from jeeves_core.types import Envelope, ProcessingRecord, PipelineEvent

# =============================================================================
# AGENT RUNTIME
# =============================================================================
from protocols.agents import (
    # Agents
    Agent,
    AgentFeatures,
    # Runtime
    PipelineRunner,
    # Factories
    create_pipeline_runner,
    create_envelope,
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
from protocols.protocols import (
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
    # Intent parsing and claim verification
    IntentParsingProtocol,
    ClaimVerificationProtocol,
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
    # Inference endpoints
    InferenceEndpoint,
    InferenceEndpointsProtocol,
    # Capability LLM configuration
    AgentLLMConfig,
    DomainLLMRegistryProtocol,
    # Feature flags provider
    FeatureFlagsProviderProtocol,
    # Agent tool access
    AgentToolAccessProtocol,
    # Language config
    LanguageConfigProtocol,
    # Memory layer protocols (L5-L6)
    GraphStorageProtocol,
    SkillStorageProtocol,
    # Infrastructure protocols (for jeeves-infra injection)
    WebSocketManagerProtocol,
    EmbeddingServiceProtocol,
    EventBridgeProtocol,
    ChunkServiceProtocol,
    SessionStateServiceProtocol,
)

# =============================================================================
# CAPABILITY SERVICER PROTOCOL
# =============================================================================
from protocols.servicer import (
    CapabilityServicerProtocol,
)

# =============================================================================
# CAPABILITY REGISTRATION
# =============================================================================
from protocols.capability import (
    # Tool catalog (capability-scoped)
    ToolCatalogEntry,
    ToolDefinition,
    CapabilityToolCatalog,
    # Config types
    DomainServiceConfig,
    DomainModeConfig,
    DomainAgentConfig,
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
from protocols.memory import (
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
from protocols.events import (
    # Event schema
    Event,
    EventCategory,
    EventSeverity,
    StandardEventTypes,
    # Protocol
    EventEmitterProtocol,
)

# =============================================================================
# UTILITIES
# =============================================================================
from protocols.utils import (
    JSONRepairKit,
    normalize_string_list,
    truncate_string,
    # Datetime utilities (L0-safe, no shared dependency)
    utc_now,
    utc_now_iso,
    parse_datetime,
)

# =============================================================================
# VALIDATION TYPES
# =============================================================================
from protocols.validation import (
    MetaValidationIssue,
    VerificationReport,
)

# =============================================================================
# INTERRUPTS AND RATE LIMITING - Now in jeeves_core.types
# =============================================================================
# from jeeves_core.types import (
#     InterruptKind, InterruptStatus, InterruptResponse, FlowInterrupt,
#     InterruptServiceProtocol, RateLimitConfig, RateLimitResult, RateLimiterProtocol
# )

# =============================================================================
# PUBLIC API
# =============================================================================
__all__ = [
    # ─── Configuration (now in jeeves_core.types) ───
    # AgentConfig, PipelineConfig, RoutingRule, EdgeLimit, JoinStrategy,
    # ContextBounds, ExecutionConfig, OrchestrationFlags, RunMode, etc.

    # ─── Envelope (now in jeeves_core.types) ───

    # ─── Agent Runtime ───
    "Agent",
    "AgentFeatures",
    "PipelineRunner",
    "create_pipeline_runner",
    "create_envelope",
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
    "IntentParsingProtocol",
    "ClaimVerificationProtocol",
    "CheckpointProtocol",
    "CheckpointRecord",
    "DistributedBusProtocol",
    "DistributedTask",
    "QueueStats",
    "ConfigRegistryProtocol",
    "IdGeneratorProtocol",
    "InferenceEndpoint",
    "InferenceEndpointsProtocol",
    "AgentLLMConfig",
    "DomainLLMRegistryProtocol",
    "FeatureFlagsProviderProtocol",
    "AgentToolAccessProtocol",
    "LanguageConfigProtocol",
    # Memory layer protocols (L5-L6)
    "GraphStorageProtocol",
    "SkillStorageProtocol",

    # ─── Capability Servicer Protocol ───
    "CapabilityServicerProtocol",

    # ─── Capability Registration ───
    "ToolCatalogEntry",
    "ToolDefinition",
    "CapabilityToolCatalog",
    "DomainServiceConfig",
    "DomainModeConfig",
    "DomainAgentConfig",
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
    "Event",
    "EventCategory",
    "EventSeverity",
    "StandardEventTypes",
    "EventEmitterProtocol",

    # ─── Utilities ───
    "JSONRepairKit",
    "normalize_string_list",
    "truncate_string",
    "utc_now",
    "utc_now_iso",
    "parse_datetime",

    # ─── Interrupts & Rate Limiting (now in jeeves_core.types) ───

    # ─── Validation Types ───
    "MetaValidationIssue",
    "VerificationReport",

    # ─── Infrastructure Protocols (jeeves-infra injection) ───
    "WebSocketManagerProtocol",
    "EmbeddingServiceProtocol",
    "EventBridgeProtocol",
    "ChunkServiceProtocol",
    "SessionStateServiceProtocol",
]
