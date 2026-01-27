"""LLM provider abstraction layer.

Supports multiple backends via adapters:
- openai_http: Direct OpenAI-compatible HTTP (default, zero deps)
- litellm: LiteLLM for 100+ providers (optional, pip install avionics[litellm])
- mock: Testing provider

Configuration:
    JEEVES_LLM_ADAPTER=openai_http|litellm|mock
    JEEVES_LLM_BASE_URL=http://localhost:8080/v1
    JEEVES_LLM_MODEL=llama
    JEEVES_LLM_API_KEY=... (optional)
"""

from avionics.llm.providers.base import LLMProvider, TokenChunk
from avionics.llm.providers.mock import MockProvider
from avionics.llm.factory import (
    create_llm_provider,
    create_llm_provider_factory,
    get_available_adapters,
)

__all__ = [
    # Core abstractions (stable)
    "LLMProvider",
    "TokenChunk",
    "MockProvider",
    # Factory (stable)
    "create_llm_provider",
    "create_llm_provider_factory",
    "get_available_adapters",
]
