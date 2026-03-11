"""LLM Provider Base Class.

Split from provider.py per Module Bloat Audit (2025-12-09).
Constitutional Reference: Constitution R1 (Adapter Pattern)
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Any, AsyncIterator, Dict, List, Optional


@dataclass
class TokenChunk:
    """A chunk of streamed tokens from an LLM.

    Represents incremental output during streaming generation.
    """
    text: str
    """The text content of this chunk"""

    is_final: bool = False
    """True if this is the last chunk in the stream"""

    token_count: int = 0
    """Estimated tokens in this chunk (0 if unknown)"""

    metadata: Optional[Dict[str, Any]] = None
    """Provider-specific metadata"""


class LLMProvider(ABC):
    """Abstract base class for LLM providers.

    Message-based chat interface with native tool calling support.
    All providers must implement chat() which accepts messages + tools
    and returns structured responses with optional tool_calls.
    """

    @abstractmethod
    async def chat(
        self,
        model: str,
        messages: List[Dict[str, Any]],
        options: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Chat completion with messages and optional tools.

        Args:
            model: Model identifier (e.g., "gpt-4", "llama3.1:8b")
            messages: OpenAI-format message list [{"role": ..., "content": ...}]
            options: Provider-specific options (temperature, max_tokens, tools, tool_choice, etc.)

        Returns:
            {"content": str, "tool_calls": [{"id": str, "function": {"name": str, "arguments": str}}]}

        Raises:
            Exception: If generation fails
        """
        pass

    async def chat_stream(
        self,
        model: str,
        messages: List[Dict[str, Any]],
        options: Optional[Dict[str, Any]] = None,
    ) -> AsyncIterator[TokenChunk]:
        """Chat completion with streaming output.

        Default implementation falls back to non-streaming chat().
        Providers should override this for true streaming support.

        Args:
            model: Model identifier
            messages: OpenAI-format message list
            options: Provider-specific options

        Yields:
            TokenChunk objects containing incremental text

        Raises:
            Exception: If generation fails
        """
        # Default fallback: call chat() and yield entire result
        result = await self.chat(model, messages, options)
        content = result.get("content", "")
        yield TokenChunk(
            text=content,
            is_final=True,
            token_count=len(content) // 4,  # Rough estimate
        )

    async def chat_with_usage(
        self,
        model: str,
        messages: List[Dict[str, Any]],
        options: Optional[Dict[str, Any]] = None,
    ) -> tuple[Dict[str, Any], Optional[Dict[str, int]]]:
        """Chat completion with optional token usage metadata.

        Returns:
            Tuple of (response_dict, usage). `usage` should contain `prompt_tokens` and
            `completion_tokens` when the provider can report them.
        """
        result = await self.chat(model, messages, options)
        return result, None

    @abstractmethod
    async def health_check(self) -> bool:
        """Check if the provider is available and responding.

        Returns:
            True if provider is healthy, False otherwise
        """
        pass

    @property
    def supports_streaming(self) -> bool:
        """Check if this provider supports true streaming.

        Returns:
            True if provider has native streaming support
        """
        # Subclasses with native streaming should override this
        return False


__all__ = ["LLMProvider", "TokenChunk"]
