"""LLM Providers Module.

Split from provider.py per Module Bloat Audit (2025-12-09).

Structure:
- base.py: LLMProvider abstract base class
- openai.py: OpenAIProvider
- anthropic.py: AnthropicProvider
- azure.py: AzureAIFoundryProvider
- mock.py: MockProvider
- llamaserver_provider.py: LlamaServerProvider (llama.cpp)

All classes are re-exported here for backwards compatibility.
"""

from .base import LLMProvider, TokenChunk
from .openai import OpenAIProvider, OPENAI_AVAILABLE
from .anthropic import AnthropicProvider, ANTHROPIC_AVAILABLE
from .azure import AzureAIFoundryProvider
from .mock import MockProvider
from .llamaserver_provider import LlamaServerProvider
from .llamacpp_provider import LlamaCppProvider


__all__ = [
    "LLMProvider",
    "TokenChunk",
    "OpenAIProvider",
    "OPENAI_AVAILABLE",
    "AnthropicProvider",
    "ANTHROPIC_AVAILABLE",
    "AzureAIFoundryProvider",
    "MockProvider",
    "LlamaServerProvider",
    "LlamaCppProvider",
]
