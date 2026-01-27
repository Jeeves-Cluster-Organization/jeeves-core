"""Factory for creating LLM providers based on configuration.

This module provides a factory function that instantiates the appropriate
LLM provider based on application settings and the capability registry.

Constitutional Reference:
    - Avionics R3: No Domain Logic - infrastructure provides transport, not business logic
    - Capability Constitution R6: Domain Config Ownership

Per-Agent Configuration:
    Agent-specific LLM settings are now queried from DomainLLMRegistry.
    Capabilities register their agent configurations at startup, and the factory
    queries the registry instead of having hardcoded agent names.

    See: avionics/capability_registry.py

**Distributed Multi-Node Support:**
When DEPLOYMENT_MODE=distributed, this factory automatically routes agents
to their assigned nodes based on config/node_profiles.py configuration.

**v1.0 Migration:**
Default provider is now llamaserver (llama.cpp server with OpenAI-compatible API).
"""

from __future__ import annotations

import os
from typing import Optional, TYPE_CHECKING

from avionics.llm.providers import (
    LLMProvider,
    OpenAIProvider,
    AnthropicProvider,
    AzureAIFoundryProvider,
    MockProvider,
)
from avionics.llm.providers import LlamaCppProvider, LlamaServerProvider, LiteLLMProvider
from protocols import InferenceEndpointsProtocol, LoggerProtocol
from avionics.logging import get_current_logger
from avionics.capability_registry import get_capability_registry

if TYPE_CHECKING:
    from avionics.settings import Settings


def create_llm_provider(
    provider_type: str,
    settings: Settings,
    agent_name: Optional[str] = None,
) -> LLMProvider:
    """Create an LLM provider based on configuration.

    Args:
        provider_type: Type of provider ("llamaserver", "openai", "anthropic", "azure", "llamacpp", "mock")
        settings: Application settings object
        agent_name: Optional agent name for per-agent configuration

    Returns:
        Configured LLM provider instance

    Raises:
        ValueError: If provider_type is unknown
    """
    provider_type = provider_type.lower()

    if provider_type == "openai":
        return OpenAIProvider(
            api_key=settings.openai_api_key if hasattr(settings, "openai_api_key") else None,
            timeout=settings.llm_timeout,
            max_retries=getattr(settings, "llm_max_retries", 3),
        )

    elif provider_type == "anthropic":
        return AnthropicProvider(
            api_key=settings.anthropic_api_key if hasattr(settings, "anthropic_api_key") else None,
            timeout=settings.llm_timeout,
            max_retries=getattr(settings, "llm_max_retries", 3),
        )

    elif provider_type == "azure":
        return AzureAIFoundryProvider(
            endpoint=settings.azure_endpoint if hasattr(settings, "azure_endpoint") else None,
            api_key=settings.azure_api_key if hasattr(settings, "azure_api_key") else None,
            deployment_name=settings.azure_deployment_name if hasattr(settings, "azure_deployment_name") else None,
            api_version=getattr(settings, "azure_api_version", "2024-02-01"),
            timeout=settings.llm_timeout,
            max_retries=getattr(settings, "llm_max_retries", 3),
        )

    elif provider_type == "llamacpp":
        return LlamaCppProvider(
            model_path=settings.llamacpp_model_path,
            n_ctx=settings.llamacpp_n_ctx,
            n_gpu_layers=settings.llamacpp_n_gpu_layers,
            n_threads=settings.llamacpp_n_threads
        )

    elif provider_type == "llamaserver":
        # Get base URL from capability registry or settings
        base_url = settings.llamaserver_host

        # Check capability registry for agent-specific server URL
        if agent_name:
            registry = get_capability_registry()
            config = registry.get_agent_config(agent_name)
            if config and config.server_url:
                base_url = config.server_url

        # Fallback to environment variable
        if not base_url:
            base_url = os.getenv("LLAMASERVER_HOST", "http://localhost:8080")

        # Get API type
        api_type = getattr(settings, "llamaserver_api_type", "native")

        return LlamaServerProvider(
            base_url=base_url,
            timeout=settings.llm_timeout,
            max_retries=getattr(settings, "llm_max_retries", 3),
            api_type=api_type
        )

    elif provider_type == "litellm":
        # LiteLLM unified provider - supports 100+ backends
        model = getattr(settings, "litellm_model", None)
        if not model:
            model = os.getenv("LITELLM_MODEL", "gpt-4-turbo")

        api_base = getattr(settings, "litellm_api_base", None)
        api_key = getattr(settings, "litellm_api_key", None)

        return LiteLLMProvider(
            model=model,
            api_base=api_base,
            api_key=api_key,
            timeout=settings.llm_timeout,
            max_retries=getattr(settings, "llm_max_retries", 3),
        )

    elif provider_type == "mock":
        return MockProvider()

    else:
        raise ValueError(
            f"Unknown provider type: {provider_type}. "
            f"Supported: 'llamaserver', 'litellm', 'openai', 'anthropic', 'azure', 'llamacpp', 'mock'"
        )


def create_agent_provider(
    settings: Settings,
    agent_name: str,
    override_provider: Optional[str] = None,
) -> LLMProvider:
    """Create a provider for a specific agent with override support.

    This function queries the capability registry for agent-specific
    provider overrides before falling back to the default provider.

    Args:
        settings: Application settings object
        agent_name: Agent name (e.g., "planner", "critic")
        override_provider: Explicit provider type to use

    Returns:
        Configured LLM provider for the agent
    """
    # Direct override takes precedence
    if override_provider:
        return create_llm_provider(override_provider, settings, agent_name=agent_name)

    # Check capability registry for agent-specific provider
    registry = get_capability_registry()
    config = registry.get_agent_config(agent_name)
    if config and config.provider:
        return create_llm_provider(config.provider, settings, agent_name=agent_name)

    # Fall back to default provider from settings
    default_provider = getattr(settings, "llm_provider", "llamaserver")
    return create_llm_provider(default_provider, settings, agent_name=agent_name)


# ==============================================================================
# Distributed Multi-Node Support
# ==============================================================================

def create_agent_provider_with_node_awareness(
    settings: Settings,
    agent_name: str,
    node_profiles: Optional[InferenceEndpointsProtocol] = None,
) -> LLMProvider:
    """Create provider for agent with node-aware routing (distributed mode).

    This is the recommended function that supports:
    - Single-node mode (backward compatible)
    - Distributed multi-node mode (with node profiles)
    - Hybrid cloud + local deployments
    - Capability registry lookups

    Args:
        settings: Application settings object
        agent_name: Agent name (e.g., "planner", "executor")
        node_profiles: Optional InferenceEndpointsProtocol for distributed routing

    Returns:
        LLM provider configured for the agent's assigned node
    """
    deployment_mode = os.getenv("DEPLOYMENT_MODE", "single_node").lower()

    _logger = get_current_logger()

    # Check for mock mode (testing)
    if os.getenv("MOCK_LLM_ENABLED", "").lower() in ("true", "1", "yes"):
        _logger.info("llm_factory_mock_mode", agent=agent_name)
        return MockProvider()

    # Check capability registry for agent-specific provider override
    registry = get_capability_registry()
    config = registry.get_agent_config(agent_name)
    if config and config.provider and config.provider != "llamaserver":
        # Use cloud provider (OpenAI, Anthropic, Azure, etc.)
        _logger.info(
            "llm_factory_provider_override",
            agent=agent_name,
            provider=config.provider,
            source="capability_registry"
        )
        return create_llm_provider(config.provider, settings, agent_name=agent_name)

    # Distributed mode: use node profiles with LlamaServerProvider
    if deployment_mode == "distributed" and node_profiles is not None:
        try:
            profile = node_profiles.get_profile_for_agent(agent_name)

            # Get API type
            api_type = getattr(settings, "llamaserver_api_type", "native")

            _logger.info(
                "llm_factory_distributed_routing",
                agent=agent_name,
                node=profile.name,
                base_url=profile.base_url,
                model=profile.model_name,
                backend="llama-server",
                api_type=api_type
            )

            return LlamaServerProvider(
                base_url=profile.base_url,
                timeout=settings.llm_timeout,
                max_retries=getattr(settings, "llm_max_retries", 3),
                api_type=api_type
            )

        except Exception as e:
            _logger.warning(
                "llm_factory_node_profile_error",
                agent=agent_name,
                error=str(e),
                fallback="single_node"
            )
            # Fallback to single-node behavior
            pass

    # Single-node mode (default): use capability registry or defaults
    return create_agent_provider(settings, agent_name)


class LLMFactory:
    """Centralized LLM provider factory with caching.

    This class provides a high-level interface for creating LLM providers
    with support for:
    - Provider caching (reuse connections)
    - Deployment mode awareness
    - Capability registry integration
    - Health tracking per node
    """

    def __init__(
        self,
        settings: Settings,
        node_profiles: Optional[InferenceEndpointsProtocol] = None,
    ):
        """Initialize factory.

        Args:
            settings: Application settings
            node_profiles: Optional InferenceEndpointsProtocol for distributed routing
        """
        self.settings = settings
        self._node_profiles = node_profiles
        self._provider_cache: dict[str, LLMProvider] = {}
        self._node_health: dict[str, bool] = {}

    def get_provider_for_agent(
        self,
        agent_name: str,
        use_cache: bool = True
    ) -> LLMProvider:
        """Get provider for agent (with optional caching).

        Args:
            agent_name: Agent name
            use_cache: Whether to use cached provider (default: True)

        Returns:
            LLM provider instance
        """
        if use_cache and agent_name in self._provider_cache:
            return self._provider_cache[agent_name]

        provider = create_agent_provider_with_node_awareness(
            self.settings,
            agent_name,
            node_profiles=self._node_profiles,
        )

        if use_cache:
            self._provider_cache[agent_name] = provider

        return provider

    def clear_cache(self):
        """Clear provider cache (forces recreation on next access)."""
        self._provider_cache.clear()
        get_current_logger().info("llm_factory_cache_cleared")

    def mark_node_healthy(self, node_name: str, is_healthy: bool):
        """Mark node health status (for future load balancing).

        Args:
            node_name: Node name (e.g., "node1")
            is_healthy: Health status
        """
        self._node_health[node_name] = is_healthy

    def get_node_health(self, node_name: str) -> bool:
        """Get health status for node (default: True)."""
        return self._node_health.get(node_name, True)
