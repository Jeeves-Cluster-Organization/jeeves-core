"""LLM provider abstraction layer for modular model execution.

This module provides a unified interface for different LLM backends:
- LlamaServerProvider: Local execution via llama.cpp server (default)
- OpenAIProvider: OpenAI API (GPT-4, GPT-3.5, etc.)
- AnthropicProvider: Anthropic API (Claude models)
- MockProvider: Deterministic responses for testing

Usage:
    from avionics.llm.factory import create_llm_provider
    from avionics.settings import settings

    provider = create_llm_provider(settings.llm_provider, settings)
    response = await provider.generate(
        model="qwen2.5-7b",
        prompt="Hello, world!",
        options={"temperature": 0.7}
    )
"""

from avionics.llm.providers import (
    LLMProvider,
    OpenAIProvider,
    AnthropicProvider,
    AzureAIFoundryProvider,
    MockProvider,
    LlamaServerProvider,
)
from avionics.llm.factory import create_llm_provider

__all__ = [
    "LLMProvider",
    "LlamaServerProvider",
    "OpenAIProvider",
    "AnthropicProvider",
    "AzureAIFoundryProvider",
    "MockProvider",
    "create_llm_provider",
]
