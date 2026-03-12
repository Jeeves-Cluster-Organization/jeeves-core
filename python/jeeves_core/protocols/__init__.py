# Jeeves Protocol Types
#
# Code is the contract. Python dataclasses define the schema.
# No proto dependency.

# =============================================================================
# PYTHON DATACLASS TYPES (with methods - use these in application code)
# =============================================================================

from jeeves_core.protocols.types import (
    # LLM result types
    LLMToolCall,
    LLMUsage,
    LLMResult,
    # Enums (string-based)
    TerminalReason,
    InterruptKind,
    InterruptStatus,
    RiskSemantic,
    RiskSeverity,
    ToolCategory,
    HealthStatus,
    LoopVerdict,
    RiskApproval,
    OperationStatus,
    RunMode,
    JoinStrategy,
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
    stage,
    Edge,
    PipelineConfig,
    InstructionContext,
    InstructionConfig,
    AgentContext,
    # Protocols
    InterruptServiceProtocol,
    # Retrieval types
    RetrievedContext,
    ClassificationResult,
)

# =============================================================================
# PYTHON PROTOCOL INTERFACES
# =============================================================================

from jeeves_core.protocols.interfaces import (
    RequestContext,
    LoggerProtocol,
    DatabaseClientProtocol,
    LLMProviderProtocol,
    ToolRegistryProtocol,
    ClockProtocol,
    AppContextProtocol,
    DistributedTask,
    QueueStats,
    DistributedBusProtocol,
    ToolExecutorProtocol,
    ConfigRegistryProtocol,
    AgentLLMConfig,
    ContextRetrieverProtocol,
    EmbeddingProviderProtocol,
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
    # LLM result types
    "LLMToolCall",
    "LLMUsage",
    "LLMResult",
    # Enums
    "TerminalReason",
    "InterruptKind",
    "InterruptStatus",
    "RiskSemantic",
    "RiskSeverity",
    "ToolCategory",
    "HealthStatus",
    "LoopVerdict",
    "RiskApproval",
    "OperationStatus",
    "RunMode",
    "JoinStrategy",
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
    "stage",
    "Edge",
    "PipelineConfig",
    "InstructionContext",
    "InstructionConfig",
    "AgentContext",
    # Protocol interfaces
    "RequestContext",
    "LoggerProtocol",
    "DatabaseClientProtocol",
    "LLMProviderProtocol",
    "ToolRegistryProtocol",
    "ClockProtocol",
    "AppContextProtocol",
    "DistributedTask",
    "QueueStats",
    "DistributedBusProtocol",
    "ToolExecutorProtocol",
    "ConfigRegistryProtocol",
    "AgentLLMConfig",
    "InterruptServiceProtocol",
    # Retrieval types
    "RetrievedContext",
    "ClassificationResult",
    "ContextRetrieverProtocol",
    "EmbeddingProviderProtocol",
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
