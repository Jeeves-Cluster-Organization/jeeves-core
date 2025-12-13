"""Anthropic LLM Provider.

Split from provider.py per Module Bloat Audit (2025-12-09).
Constitutional Reference: Avionics R4 (Swappable Implementations)
"""

from typing import Any, AsyncIterator, Dict, Optional

from .base import LLMProvider, TokenChunk

try:
    import anthropic
    ANTHROPIC_AVAILABLE = True
except ImportError:
    ANTHROPIC_AVAILABLE = False


class AnthropicProvider(LLMProvider):
    """Provider for Anthropic API (Claude models).

    Enables testing with Claude models for comparative analysis and
    framework validation.
    """

    def __init__(
        self,
        api_key: Optional[str] = None,
        timeout: int = 60,
        max_retries: int = 3,
    ):
        """Initialize Anthropic provider.

        Args:
            api_key: Anthropic API key (or set ANTHROPIC_API_KEY env var)
            timeout: Request timeout in seconds
            max_retries: Number of retries on failure

        Raises:
            RuntimeError: If anthropic package is not installed
        """
        if not ANTHROPIC_AVAILABLE:
            raise RuntimeError(
                "Anthropic package not installed. Install with: pip install anthropic"
            )

        self.client = anthropic.AsyncAnthropic(
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
        """Generate text using Anthropic API.

        Options mapping:
        - temperature: Controls randomness (0.0-1.0)
        - num_predict: Maps to max_tokens
        - num_ctx: Ignored (Claude handles context automatically)
        """
        if options is None:
            options = {}

        # Map llama-server-style options to Anthropic format
        temperature = options.get("temperature", 0.7)
        max_tokens = options.get("num_predict", 400)

        response = await self.client.messages.create(
            model=model,
            max_tokens=max_tokens,
            temperature=temperature,
            messages=[{"role": "user", "content": prompt}],
        )

        # Extract text from response
        if response.content and len(response.content) > 0:
            return response.content[0].text
        return ""

    async def generate_stream(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> AsyncIterator[TokenChunk]:
        """Generate text with streaming using Anthropic API.

        Yields TokenChunk objects as tokens are generated.

        Options mapping:
        - temperature: Controls randomness (0.0-1.0)
        - num_predict: Maps to max_tokens
        """
        if options is None:
            options = {}

        temperature = options.get("temperature", 0.7)
        max_tokens = options.get("num_predict", 400)

        async with self.client.messages.stream(
            model=model,
            max_tokens=max_tokens,
            temperature=temperature,
            messages=[{"role": "user", "content": prompt}],
        ) as stream:
            async for text in stream.text_stream:
                yield TokenChunk(
                    text=text,
                    is_final=False,
                    token_count=1,  # Approximate: 1 chunk ≈ 1 token
                )

        # Yield final chunk to signal completion
        yield TokenChunk(
            text="",
            is_final=True,
            token_count=0,
        )

    @property
    def supports_streaming(self) -> bool:
        """Anthropic supports native streaming."""
        return True

    async def health_check(self) -> bool:
        """Check if Anthropic API is accessible."""
        try:
            # Make a minimal request to verify connectivity
            await self.client.messages.create(
                model="claude-3-5-sonnet-20241022",
                max_tokens=10,
                messages=[{"role": "user", "content": "Hi"}],
            )
            return True
        except Exception:
            return False


__all__ = ["AnthropicProvider", "ANTHROPIC_AVAILABLE"]
