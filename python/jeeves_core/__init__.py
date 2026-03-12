"""Jeeves Core — orchestration framework for building capabilities.

Install: pip install jeeves-core
Usage:   from jeeves_core import register_capability, PipelineConfig, stage, eq
"""

__version__ = "0.0.1"

# Pipeline definition
from jeeves_core.protocols import (
    PipelineConfig, AgentConfig, stage, Edge, RoutingRule,
    RunMode, JoinStrategy, TokenStreamMode, GenerationParams, EdgeLimit,
    # Wiring
    ModeConfig, ToolsConfig, ToolCatalog,
    # Context
    RequestContext, AgentContext,
    # Enums
    TerminalReason,
    # Interfaces (for typing)
    AppContextProtocol, LLMProviderProtocol, ToolRegistryProtocol,
)
# Routing expression builders
from jeeves_core.protocols.routing import (
    eq, neq, gt, lt, gte, lte, contains, exists, not_exists,
    and_, or_, not_, always, agent, meta, state, interrupt, current,
)
# Registration
from jeeves_core.capability_wiring import register_capability
# Runtime
from jeeves_core.runtime import (
    Agent, PipelineRunner, create_pipeline_runner,
    CapabilityService, CapabilityResult,
)
from jeeves_core.runtime.deterministic import DeterministicAgent
# Bootstrap
from jeeves_core.bootstrap import create_app_context

__all__ = [
    # Registration
    "register_capability",
    # Pipeline definition
    "PipelineConfig", "AgentConfig", "stage", "Edge", "RoutingRule",
    "RunMode", "JoinStrategy", "GenerationParams", "TokenStreamMode", "EdgeLimit",
    # Wiring
    "ModeConfig", "ToolsConfig", "ToolCatalog",
    # Runtime
    "Agent", "DeterministicAgent",
    "PipelineRunner", "create_pipeline_runner",
    "CapabilityService", "CapabilityResult",
    # Context
    "AgentContext", "RequestContext",
    # Enums
    "TerminalReason",
    # Interfaces (for typing)
    "AppContextProtocol", "LLMProviderProtocol", "ToolRegistryProtocol",
    # Bootstrap
    "create_app_context",
    # Routing DSL
    "eq", "neq", "gt", "lt", "gte", "lte", "contains",
    "exists", "not_exists",
    "and_", "or_", "not_", "always",
    "agent", "meta", "state", "interrupt", "current",
]
