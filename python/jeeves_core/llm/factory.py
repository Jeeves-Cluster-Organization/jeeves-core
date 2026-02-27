"""Factory for creating LLM providers.

Config via environment:
    AIRFRAME_LLM_ADAPTER: Provider adapter (openai_http, litellm, mock)
    AIRFRAME_LLM_BASE_URL: API endpoint (e.g., http://localhost:8080/v1)
    AIRFRAME_LLM_MODEL: Model identifier
    AIRFRAME_LLM_API_KEY: API key (optional for local servers)

Constitutional Reference:
    - Constitution R3: No Domain Logic - infrastructure provides transport, not business logic
"""

from __future__ import annotations

import os
from typing import Callable, Dict, Optional, TYPE_CHECKING

from jeeves_core.logging import get_current_logger

if TYPE_CHECKING:
    from jeeves_core.protocols import LLMProviderProtocol
    from jeeves_core.settings import Settings

# Provider registry - populated lazily
_REGISTRY: Dict[str, Callable[..., "LLMProviderProtocol"]] = {}
_loaded = False


def _load_providers() -> None:
    """Lazy-load available providers. Called once on first use."""
    global _loaded
    if _loaded:
        return
    _loaded = True

    # MockProvider - always available (no external deps)
    from .providers.mock import MockProvider
    _REGISTRY["mock"] = lambda **kw: MockProvider()

    # OpenAI HTTP - always available (uses httpx, already in deps)
    try:
        from .providers.openai_http_provider import OpenAIHTTPProvider
        _REGISTRY["openai_http"] = lambda **kw: OpenAIHTTPProvider(**kw)
    except ImportError:
        pass

    # LiteLLM - optional
    try:
        from .providers.litellm_provider import LiteLLMProvider
        _REGISTRY["litellm"] = lambda **kw: LiteLLMProvider(**kw)
    except ImportError:
        pass


def create_llm_provider(
    settings: "Settings",
    agent_name: Optional[str] = None,
) -> "LLMProviderProtocol":
    """Create an LLM provider based on configuration.

    Priority:
    1. MOCK_LLM_ENABLED=true -> MockProvider
    2. AIRFRAME_LLM_ADAPTER env var -> specified adapter
    3. Default: openai_http (zero external deps)

    Args:
        settings: Application settings object
        agent_name: Optional agent name (for logging)

    Returns:
        Configured LLM provider instance

    Raises:
        ImportError: If requested adapter is not available
    """
    logger = get_current_logger()
    _load_providers()

    # Check for mock mode first
    if os.getenv("MOCK_LLM_ENABLED", "").lower() in ("true", "1", "yes"):
        logger.info("llm_factory_mock_mode", agent=agent_name)
        return _REGISTRY["mock"]()

    # Determine adapter from env or settings
    adapter = (
        os.getenv("AIRFRAME_LLM_ADAPTER") or
        getattr(settings, "airframe_llm_adapter", None) or
        "openai_http"
    )

    if adapter not in _REGISTRY:
        available = list(_REGISTRY.keys())
        raise ImportError(
            f"LLM adapter '{adapter}' not available. "
            f"Installed adapters: {available}. "
            f"Install with: pip install jeeves-core[litellm]"
        )

    # Build config from env vars or settings
    model = (
        os.getenv("AIRFRAME_LLM_MODEL") or
        getattr(settings, "airframe_llm_model", None) or
        getattr(settings, "default_model", None)
    )
    if not model:
        raise ValueError(
            "No LLM model configured. Set AIRFRAME_LLM_MODEL env var "
            "or configure default_model in settings."
        )
    api_base = (
        os.getenv("AIRFRAME_LLM_BASE_URL") or
        getattr(settings, "airframe_llm_base_url", None)
    )
    api_key = (
        os.getenv("AIRFRAME_LLM_API_KEY") or
        getattr(settings, "airframe_llm_api_key", None)
    )
    timeout = float(os.getenv("AIRFRAME_LLM_TIMEOUT", str(getattr(settings, "llm_timeout", 120))))
    max_retries = int(os.getenv("AIRFRAME_LLM_MAX_RETRIES", str(getattr(settings, "llm_max_retries", 3))))

    logger.info(
        "llm_factory_creating",
        adapter=adapter,
        agent=agent_name,
        model=model,
        api_base=api_base,
    )

    return _REGISTRY[adapter](
        model=model,
        api_base=api_base,
        api_key=api_key,
        timeout=timeout,
        max_retries=max_retries,
    )


def get_available_adapters() -> list[str]:
    """Return list of available adapter names."""
    _load_providers()
    return list(_REGISTRY.keys())


def create_llm_provider_factory(
    settings: "Settings",
) -> Callable[[str], "LLMProviderProtocol"]:
    """Create a factory function for LLM providers.

    Args:
        settings: Application settings

    Returns:
        Factory function: (agent_name: str) -> LLMProviderProtocol
    """
    def factory(agent_name: str) -> "LLMProviderProtocol":
        return create_llm_provider(settings, agent_name=agent_name)

    return factory
