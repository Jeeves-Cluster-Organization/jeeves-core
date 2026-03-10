# Jeeves Protocol Types
#
# Code is the contract. Python dataclasses define the schema.
# No proto dependency.

# =============================================================================
# PYTHON DATACLASS TYPES (with methods - use these in application code)
# =============================================================================

from jeeves_core.protocols.types import (
    # Enums (string-based)
    TerminalReason,
    InterruptKind,
    InterruptStatus,
    RiskSemantic,
    RiskSeverity,
    ToolCategory,
    ToolAccess,
    HealthStatus,
    LoopVerdict,
    RiskApproval,
    OperationStatus,
    RunMode,
    JoinStrategy,
    AgentOutputMode,
    TokenStreamMode,
    # Dataclass types with methods
    OperationResult,
    InterruptResponse,
    FlowInterrupt,
    RateLimitConfig,
    RateLimitResult,
    ProcessingRecord,
    PipelineEvent,
    RoutingRule,
    EdgeLimit,
    GenerationParams,
    AgentConfig,
    Edge,
    PipelineConfig,
    ContextBounds,
    ExecutionConfig,
    OrchestrationFlags,
    Envelope,
    # Protocols
    InterruptServiceProtocol,
)

# =============================================================================
# PYTHON PROTOCOL INTERFACES
# =============================================================================

from jeeves_core.protocols.interfaces import (
    RequestContext,
    LoggerProtocol,
    DatabaseClientProtocol,
    LLMProviderProtocol,
    ToolProtocol,
    ToolDefinitionProtocol,
    ToolRegistryProtocol,
    ClockProtocol,
    AppContextProtocol,
    SearchResult,
    SemanticSearchProtocol,
    DistributedTask,
    QueueStats,
    DistributedBusProtocol,
    ToolExecutorProtocol,
    ConfigRegistryProtocol,
    AgentLLMConfig,
    AgentToolAccessProtocol,
    WebSocketManagerProtocol,
    EventBridgeProtocol,
)

# =============================================================================
# CAPABILITY REGISTRATION (moved from jeeves-core/protocols)
# =============================================================================

from jeeves_core.protocols.capability import (
    get_capability_resource_registry,
    reset_capability_resource_registry,
    CapabilityResourceRegistry,
    CapabilityResourceRegistryProtocol,
    CapabilityToolCatalog,
    ToolCatalogEntry,
    ToolDefinition,
    DomainAgentConfig,
    CapabilityPromptConfig,
    CapabilityToolsConfig,
    CapabilityOrchestratorConfig,
    CapabilityContractsConfig,
    DomainServiceConfig,
    DomainModeConfig,
)

# =============================================================================
# UTILITIES
# =============================================================================

from jeeves_core.protocols import routing  # noqa: F401 — routing builder module

from jeeves_core.utils.json_repair import JSONRepairKit
from jeeves_core.utils.strings import normalize_string_list

__all__ = [
    # Enums
    "TerminalReason",
    "InterruptKind",
    "InterruptStatus",
    "RiskSemantic",
    "RiskSeverity",
    "ToolCategory",
    "ToolAccess",
    "HealthStatus",
    "LoopVerdict",
    "RiskApproval",
    "OperationStatus",
    "RunMode",
    "JoinStrategy",
    "AgentOutputMode",
    "TokenStreamMode",
    # Dataclass types
    "OperationResult",
    "InterruptResponse",
    "FlowInterrupt",
    "RateLimitConfig",
    "RateLimitResult",
    "ProcessingRecord",
    "PipelineEvent",
    "RoutingRule",
    "EdgeLimit",
    "GenerationParams",
    "AgentConfig",
    "Edge",
    "PipelineConfig",
    "ContextBounds",
    "ExecutionConfig",
    "OrchestrationFlags",
    "Envelope",
    # Protocol interfaces
    "RequestContext",
    "LoggerProtocol",
    "DatabaseClientProtocol",
    "LLMProviderProtocol",
    "ToolProtocol",
    "ToolDefinitionProtocol",
    "ToolRegistryProtocol",
    "ClockProtocol",
    "AppContextProtocol",
    "SearchResult",
    "SemanticSearchProtocol",
    "DistributedTask",
    "QueueStats",
    "DistributedBusProtocol",
    "ToolExecutorProtocol",
    "ConfigRegistryProtocol",
    "AgentLLMConfig",
    "AgentToolAccessProtocol",
    "WebSocketManagerProtocol",
    "EventBridgeProtocol",
    "InterruptServiceProtocol",
    # Capability registration
    "get_capability_resource_registry",
    "reset_capability_resource_registry",
    "CapabilityResourceRegistry",
    "CapabilityResourceRegistryProtocol",
    "CapabilityToolCatalog",
    "ToolCatalogEntry",
    "ToolDefinition",
    "DomainAgentConfig",
    "CapabilityPromptConfig",
    "CapabilityToolsConfig",
    "CapabilityOrchestratorConfig",
    "CapabilityContractsConfig",
    "DomainServiceConfig",
    "DomainModeConfig",
    # Utilities
    "JSONRepairKit",
    "normalize_string_list",
]
