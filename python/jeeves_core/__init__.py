"""Jeeves Infrastructure & Orchestration Framework.

This package provides infrastructure abstractions, orchestration framework,
and protocol-based plumbing for the jeeves-core microkernel. Concrete database
implementations are owned by capabilities and registered via the backend registry.

Sub-packages:
- gateway/       - HTTP/WebSocket/SSE translation (FastAPI)
- llm/           - LLM providers (LiteLLM, OpenAI, Mock)
- database/      - Protocol-based database factory and registry
- redis/         - Distributed state backend
- memory/        - Memory handlers, messages, tool metrics/health
- runtime/       - Python agent/pipeline execution (LLM calls, tool execution)
- config/        - Agent profiles, registry, constants
- orchestrator/  - Event context, governance, flow
- events/        - Event bridge for kernel <-> gateway
- services/      - Debug API
- utils/         - JSON repair, string helpers, prompt builder

Top-level modules:
- bootstrap      - AppContext creation, composition root
- health         - Kubernetes liveness/readiness probes
- capability_wiring - Registration, discovery, router mounting

Architecture:
    Capabilities (User Space) - own concrete DB, prompts, tools, agents
           |
           v
    jeeves-core (Infrastructure + Orchestration)  <- THIS PACKAGE
           |
           v
    jeeves-core (Microkernel - Rust)

Usage:
    from jeeves_core.protocols import PipelineConfig, AgentConfig, Envelope
    from jeeves_core.bootstrap import create_app_context
    from jeeves_core.wiring import create_tool_executor
    from jeeves_core.settings import get_settings

    # Bootstrap eagerly provisions: kernel_client, llm_provider_factory, config_registry
    app_context = create_app_context()
"""

__version__ = "1.0.0"

# =============================================================================
# Top-level re-exports for capability authors
# =============================================================================

from jeeves_core.protocols import (
    # Core types
    PipelineConfig, AgentConfig, Envelope, RoutingRule, EdgeLimit,
    ContextBounds, ExecutionConfig, OrchestrationFlags, GenerationParams,
    # Enums
    TerminalReason, JoinStrategy, AgentOutputMode, RunMode,
    # Routing builders
    routing,
    # Interfaces
    AppContextProtocol, LLMProviderProtocol, ToolRegistryProtocol,
    # Capability registration
    DomainServiceConfig, DomainAgentConfig,
    CapabilityOrchestratorConfig, CapabilityToolsConfig, CapabilityPromptConfig,
)
from jeeves_core.runtime import (
    Agent, PipelineRunner, create_pipeline_runner, create_envelope,
    CapabilityService, CapabilityResult,
)
from jeeves_core.bootstrap import create_app_context

__all__ = [
    "__version__",
    # Core types
    "PipelineConfig", "AgentConfig", "Envelope", "RoutingRule", "EdgeLimit",
    "ContextBounds", "ExecutionConfig", "OrchestrationFlags", "GenerationParams",
    # Enums
    "TerminalReason", "JoinStrategy", "AgentOutputMode", "RunMode",
    # Routing builders
    "routing",
    # Interfaces
    "AppContextProtocol", "LLMProviderProtocol", "ToolRegistryProtocol",
    # Capability registration
    "DomainServiceConfig", "DomainAgentConfig",
    "CapabilityOrchestratorConfig", "CapabilityToolsConfig", "CapabilityPromptConfig",
    # Runtime
    "Agent", "PipelineRunner", "create_pipeline_runner", "create_envelope",
    "CapabilityService", "CapabilityResult",
    # Bootstrap
    "create_app_context",
]
