"""
Mission System Contracts - Static Type Definitions for App Integration.

Centralized Architecture (v4.0):
This module re-exports unified agent types from jeeves_protocols:
- AgentConfig, PipelineConfig: Declarative agent definitions
- UnifiedAgent: Single agent class driven by configuration
- GenericEnvelope: Dynamic output slots
- ToolAccess: Agent tool access levels

All types come from jeeves_protocols (Python bridge to Go core).
"""

# =============================================================================
# CANONICAL PROTOCOLS (from jeeves_protocols)
# =============================================================================
from jeeves_protocols import (
    # App context protocols
    SettingsProtocol,
    FeatureFlagsProtocol,
    AppContextProtocol,
    LoggerProtocol,
    ClockProtocol,
    # Infrastructure protocols
    PersistenceProtocol,
    LLMProviderProtocol,
    ToolRegistryProtocol,
    NLIServiceProtocol,
    # Risk level
    RiskLevel,
)

# =============================================================================
# CENTRALIZED AGENT ARCHITECTURE
# =============================================================================
from jeeves_protocols import (
    # Agent configuration
    AgentConfig,
    PipelineConfig,
    RoutingRule,
    ToolAccess,
    AgentCapability,
    # Unified agent
    UnifiedAgent,
    # Generic envelope
    GenericEnvelope,
    ProcessingRecord,
    create_generic_envelope,
    # Enums
    TerminalReason,
    CriticVerdict,
    RiskApproval,
)

# Unified runtime
from jeeves_protocols import (
    UnifiedRuntime,
    create_runtime_from_config,
)

# Core protocols from core_engine
from jeeves_protocols import (
    ToolProtocol,
    EventBusProtocol,
    MemoryServiceProtocol,
    IdGeneratorProtocol,
    ToolExecutorProtocol,
    EventContextProtocol,
    ConfigRegistryProtocol,
)

# Working memory base class
from jeeves_protocols import WorkingMemory, Finding

# Configuration types (ContextBounds only - access bounds via AppContext.get_context_bounds())
from jeeves_protocols import ContextBounds

# JSON utilities
from jeeves_protocols import (
    JSONRepairKit,
    normalize_string_list,
)

# NOTE: AgentRuntime removed in v4.0 - use UnifiedRuntime + UnifiedAgent
# UnifiedAgent handles all agent-level services (logging, events, persistence)

# Config registry
from jeeves_mission_system.config.registry import (
    ConfigRegistry,
    ConfigKeys,
    get_config_registry,
    set_config_registry,
)

# Agent profiles (generic types - capability-specific profiles in capability layer)
# See: jeeves-capability-code-analyser/config/llm_config.py
from jeeves_mission_system.config.agent_profiles import (
    LLMProfile,
    ThresholdProfile,
    AgentProfile,
    get_agent_profile,
    get_llm_profile,
    get_thresholds,
    get_latency_budget,
)

# NOTE: Tool catalog types (ToolId, ToolCatalog, etc.) are OWNED BY CAPABILITIES
# NOT re-exported here. Capability layers should import from their own tools/catalog.py.
# See: jeeves-capability-code-analyser/tools/catalog.py for code_analysis capability.
# ToolCategory and RiskLevel come from jeeves_protocols.

# ToolCategory from jeeves_protocols (canonical location)
from jeeves_protocols import ToolCategory

# NOTE: AgentContext removed in v4.0 - context flows through GenericEnvelope

__all__ = [
    # ─── Centralized Agent Architecture ───
    # Agent configuration
    "AgentConfig",
    "PipelineConfig",
    "RoutingRule",
    "ToolAccess",
    "AgentCapability",
    # Unified agent
    "UnifiedAgent",
    # Generic envelope
    "GenericEnvelope",
    "ProcessingRecord",
    "create_generic_envelope",
    # Unified runtime
    "UnifiedRuntime",
    "create_runtime_from_config",
    # Enums
    "TerminalReason",
    "CriticVerdict",
    "RiskApproval",

    # ─── Working memory ───
    "WorkingMemory",
    "Finding",

    # ─── Canonical Protocols (from jeeves_commbus) ───
    "SettingsProtocol",
    "FeatureFlagsProtocol",
    "AppContextProtocol",
    "LoggerProtocol",
    "ClockProtocol",
    "PersistenceProtocol",
    "LLMProviderProtocol",
    "ToolRegistryProtocol",
    "NLIServiceProtocol",
    "RiskLevel",

    # ─── Core protocols (from core_engine) ───
    "ToolProtocol",
    "EventBusProtocol",
    "MemoryServiceProtocol",
    "IdGeneratorProtocol",
    "ToolExecutorProtocol",
    "EventContextProtocol",
    "ConfigRegistryProtocol",

    # ─── Configuration (access bounds via AppContext.get_context_bounds()) ───
    "ContextBounds",

    # ─── JSON utilities ───
    "JSONRepairKit",
    "normalize_string_list",

    # ─── Config Registry ───
    "ConfigRegistry",
    "ConfigKeys",
    "get_config_registry",
    "set_config_registry",

    # ─── Agent Profiles (generic types) ───
    # Capability-specific profiles in: jeeves-capability-code-analyser/config/llm_config.py
    "LLMProfile",
    "ThresholdProfile",
    "AgentProfile",
    "get_agent_profile",
    "get_llm_profile",
    "get_thresholds",
    "get_latency_budget",

    # ─── Tool Types (from jeeves_protocols) ───
    # NOTE: ToolId, ToolCatalog, etc. are CAPABILITY-OWNED (not re-exported here)
    # Capabilities import from their own tools/catalog.py
    "ToolCategory",  # From jeeves_protocols

]
