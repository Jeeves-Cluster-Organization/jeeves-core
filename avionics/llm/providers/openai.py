"""OpenAI LLM Provider.

Split from provider.py per Module Bloat Audit (2025-12-09).
Constitutional Reference: Avionics R4 (Swappable Implementations)
"""

from typing import Any, AsyncIterator, Dict, Optional

from .base import LLMProvider, TokenChunk

try:
    import openai
    OPENAI_AVAILABLE = True
except ImportError:
    OPENAI_AVAILABLE = False


class OpenAIProvider(LLMProvider):
    """Provider for OpenAI API (GPT-4, GPT-3.5, etc.).

    This provider enables testing with state-of-the-art models to establish
    baseline performance and identify framework vs model limitations.

    Supports both standard and streaming generation.
    """

    def __init__(
        self,
        api_key: Optional[str] = None,
        timeout: int = 60,
        max_retries: int = 3,
    ):
        """Initialize OpenAI provider.

        Args:
            api_key: OpenAI API key (or set OPENAI_API_KEY env var)
            timeout: Request timeout in seconds
            max_retries: Number of retries on failure

        Raises:
            RuntimeError: If openai package is not installed
        """
        if not OPENAI_AVAILABLE:
            raise RuntimeError(
                "OpenAI package not installed. Install with: pip install openai"
            )

        self.client = openai.AsyncOpenAI(
            api_key=api_key,
            timeout=timeout,
            max_retries=max_retries,
        )

    async def generate(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> str:
        """Generate text using OpenAI API.

        Options mapping:
        - temperature: Controls randomness (0.0-2.0)
        - num_predict: Maps to max_tokens
        - num_ctx: Ignored (OpenAI handles context automatically)
        - json_mode: Enable JSON output format (response_format={"type": "json_object"})
        """
        if options is None:
            options = {}

        # Map llama-server-style options to OpenAI format
        temperature = options.get("temperature", 0.7)
        max_tokens = options.get("num_predict", 400)
        json_mode = options.get("json_mode", False)

        # Build request kwargs
        kwargs: Dict[str, Any] = {
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": temperature,
            "max_tokens": max_tokens,
        }

        # Add JSON mode if requested
        if json_mode:
            kwargs["response_format"] = {"type": "json_object"}

        # Use chat completion API (works for all modern OpenAI models)
        response = await self.client.chat.completions.create(**kwargs)

        return response.choices[0].message.content or ""

    async def generate_stream(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> AsyncIterator[TokenChunk]:
        """Generate text with streaming using OpenAI API.

        Yields TokenChunk objects as tokens are generated.

        Options mapping:
        - temperature: Controls randomness (0.0-2.0)
        - num_predict: Maps to max_tokens
        - json_mode: Enable JSON output format
        """
        if options is None:
            options = {}

        temperature = options.get("temperature", 0.7)
        max_tokens = options.get("num_predict", 400)
        json_mode = options.get("json_mode", False)

        kwargs: Dict[str, Any] = {
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": True,
        }

        if json_mode:
            kwargs["response_format"] = {"type": "json_object"}

        stream = await self.client.chat.completions.create(**kwargs)

        async for chunk in stream:
            if chunk.choices and chunk.choices[0].delta.content:
                content = chunk.choices[0].delta.content
                finish_reason = chunk.choices[0].finish_reason

                yield TokenChunk(
                    text=content,
                    is_final=(finish_reason is not None),
                    token_count=1,  # Approximate: 1 chunk ≈ 1 token
                    metadata={
                        "finish_reason": finish_reason,
                        "model": chunk.model,
                    } if finish_reason else None,
                )

    async def health_check(self) -> bool:
        """Check if OpenAI API is accessible."""
        try:
            # Simple model list call to verify connectivity
            await self.client.models.list()
            return True
        except Exception:
            return False

    @property
    def supports_streaming(self) -> bool:
        """OpenAI supports native streaming."""
        return True


__all__ = ["OpenAIProvider", "OPENAI_AVAILABLE"]
