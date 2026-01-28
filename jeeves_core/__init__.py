"""Jeeves Core - Thin Python bindings to Go kernel.

All types are backed by Go definitions in coreengine/.

Runtime and utils are in jeeves_infra:
    from jeeves_infra.runtime import PipelineRunner, Agent
    from jeeves_infra.utils import JSONRepairKit, utc_now
"""

from jeeves_core.types import (
    HealthStatus,
    LoopVerdict,
    OperationResult,
    OperationStatus,
    RiskApproval,
    RiskLevel,
    TerminalReason,
    ToolAccess,
    ToolCategory,
)

from jeeves_core.events import (
    Event,
    EventCategory,
    EventEmitterProtocol,
    EventSeverity,
    StandardEventTypes,
)

from jeeves_core.protocols import (
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

__all__ = [
    # Enums
    "RiskLevel",
    "ToolCategory",
    "HealthStatus",
    "TerminalReason",
    "LoopVerdict",
    "RiskApproval",
    "ToolAccess",
    "OperationStatus",
    "OperationResult",
    # Events
    "Event",
    "EventCategory",
    "EventEmitterProtocol",
    "EventSeverity",
    "StandardEventTypes",
    # Protocols
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
    "GraphStorageProtocol",
    "SkillStorageProtocol",
    "WebSocketManagerProtocol",
    "EmbeddingServiceProtocol",
    "EventBridgeProtocol",
    "ChunkServiceProtocol",
    "SessionStateServiceProtocol",
]
