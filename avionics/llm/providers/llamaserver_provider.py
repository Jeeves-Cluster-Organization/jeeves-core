"""
LlamaServer provider - uses LiteLLM to talk to llama.cpp server.

This provider wraps LiteLLM for llama-server communication and adds:
- Cost tracking (tokens → USD)
- Settings integration
- Structured logging with cost metrics

LiteLLM handles:
- HTTP requests and SSE parsing
- Retry logic and error handling
- OpenAI-compatible API translation

Note: llama-server exposes an OpenAI-compatible API at /v1/chat/completions,
so we use LiteLLM's OpenAI provider with a custom api_base.
"""

from typing import AsyncIterator, Dict, Any, Optional

from .base import LLMProvider, TokenChunk
from ..cost_calculator import get_cost_calculator
from avionics.logging import get_current_logger
from protocols import LoggerProtocol

try:
    from litellm import acompletion
    LITELLM_AVAILABLE = True
except ImportError:
    acompletion = None  # type: ignore
    LITELLM_AVAILABLE = False


class LlamaServerProvider(LLMProvider):
    """
    LlamaServer provider using LiteLLM for OpenAI-compatible API.

    Replaces the airframe-based implementation with direct LiteLLM calls.
    llama-server exposes /v1/chat/completions which is OpenAI-compatible.

    Attributes:
        base_url: llama-server endpoint (e.g., "http://node1:8080")
        timeout: Request timeout in seconds
        max_retries: Number of retries on transient failures
        api_type: "native" or "openai" (both use OpenAI-compatible API now)
    """

    def __init__(
        self,
        base_url: str,
        timeout: float = 120.0,
        max_retries: int = 3,
        api_type: str = "native",
        logger: Optional[LoggerProtocol] = None,
    ):
        """
        Initialize LlamaServer provider.

        Args:
            base_url: llama-server endpoint (e.g., "http://node1:8080")
            timeout: Request timeout in seconds (default: 120s for large contexts)
            max_retries: Number of retries on transient failures
            api_type: API type - kept for backward compatibility, but both
                      now use the OpenAI-compatible /v1/chat/completions endpoint
            logger: Logger for DI (uses context logger if not provided)
        """
        if not LITELLM_AVAILABLE:
            raise ImportError("litellm is required: pip install litellm")

        self._logger = logger or get_current_logger()
        self._base_url = base_url.rstrip("/")
        self._timeout = timeout
        self._max_retries = max_retries
        self._api_type = api_type
        self._cost_calculator = get_cost_calculator()

        self._logger.info(
            "llamaserver_provider_initialized",
            base_url=base_url,
            timeout=timeout,
            api_type=api_type,
            backend="litellm",
        )

    @property
    def timeout(self) -> float:
        """Request timeout in seconds."""
        return self._timeout

    @property
    def max_retries(self) -> int:
        """Number of retries on transient failures."""
        return self._max_retries

    async def generate(
        self, model: str, prompt: str, options: Optional[Dict[str, Any]] = None
    ) -> str:
        """
        Generate text completion via llama-server.

        Uses LiteLLM to call llama-server's OpenAI-compatible API.

        Args:
            model: Model name (ignored - llama-server has single model loaded)
            prompt: Input prompt text
            options: Generation options:
                - temperature: Sampling temperature (default: 0.7)
                - num_predict/max_tokens: Max tokens to generate (default: 512)
                - stop: List of stop sequences
                - top_p: Top-p sampling (default: 0.95)

        Returns:
            Generated text string

        Raises:
            Exception: On LLM errors
        """
        opts = options or {}

        self._logger.debug(
            "llamaserver_generating",
            prompt_length=len(prompt),
            max_tokens=opts.get("num_predict") or opts.get("max_tokens", 512),
            temperature=opts.get("temperature", 0.7),
        )

        try:
            # Use LiteLLM with OpenAI provider pointing to llama-server
            # The model name "openai/..." tells LiteLLM to use OpenAI format
            response = await acompletion(
                model="openai/local-model",  # Name doesn't matter for llama-server
                messages=[{"role": "user", "content": prompt}],
                api_base=f"{self._base_url}/v1",
                api_key="not-needed",  # llama-server doesn't require auth
                timeout=self._timeout,
                num_retries=self._max_retries,
                temperature=opts.get("temperature"),
                max_tokens=opts.get("num_predict") or opts.get("max_tokens"),
                top_p=opts.get("top_p"),
                stop=opts.get("stop"),
                stream=False,
            )

            # Extract text from response
            text = response.choices[0].message.content or ""

            # Extract usage for cost tracking
            usage = getattr(response, "usage", None)
            prompt_tokens = getattr(usage, "prompt_tokens", 0) if usage else 0
            completion_tokens = getattr(usage, "completion_tokens", 0) if usage else 0

            # Calculate cost
            cost_metrics = self._cost_calculator.calculate_cost(
                provider="llamaserver",
                model=model,
                prompt_tokens=prompt_tokens,
                completion_tokens=completion_tokens,
            )

            self._logger.info(
                "llm_generation_complete",
                provider="llamaserver",
                model=model,
                prompt_tokens=prompt_tokens,
                completion_tokens=completion_tokens,
                cost_usd=cost_metrics.cost_usd,
            )

            return text

        except Exception as e:
            self._logger.error("llm_error", error=str(e))
            raise Exception(f"LLM Error: {e}") from e

    async def generate_stream(
        self, model: str, prompt: str, options: Optional[Dict[str, Any]] = None
    ) -> AsyncIterator[TokenChunk]:
        """
        Generate text with streaming via llama-server.

        Uses LiteLLM to stream from llama-server's OpenAI-compatible API.

        Args:
            model: Model name (ignored - server has single model)
            prompt: Input prompt text
            options: Generation options (temperature, num_predict, etc.)

        Yields:
            TokenChunk objects with incremental text
        """
        opts = options or {}

        self._logger.debug(
            "llamaserver_streaming",
            prompt_length=len(prompt),
        )

        try:
            # Use LiteLLM with streaming
            response = await acompletion(
                model="openai/local-model",
                messages=[{"role": "user", "content": prompt}],
                api_base=f"{self._base_url}/v1",
                api_key="not-needed",
                timeout=self._timeout,
                num_retries=self._max_retries,
                temperature=opts.get("temperature"),
                max_tokens=opts.get("num_predict") or opts.get("max_tokens"),
                top_p=opts.get("top_p"),
                stop=opts.get("stop"),
                stream=True,
            )

            async for chunk in response:
                choices = getattr(chunk, "choices", [])
                if not choices:
                    continue

                delta = getattr(choices[0], "delta", None)
                if not delta:
                    continue

                content = getattr(delta, "content", None)
                finish_reason = getattr(choices[0], "finish_reason", None)

                if content:
                    yield TokenChunk(
                        text=content,
                        is_final=False,
                        token_count=1,
                    )

                if finish_reason:
                    yield TokenChunk(text="", is_final=True, token_count=0)
                    return

            # Ensure final chunk is always emitted
            yield TokenChunk(text="", is_final=True, token_count=0)

        except Exception as e:
            self._logger.error("llm_stream_error", error=str(e))
            raise Exception(f"LLM Stream Error: {e}") from e

    @property
    def supports_streaming(self) -> bool:
        """LlamaServer supports native streaming via SSE."""
        return True

    async def health_check(self) -> bool:
        """
        Check if llama-server is healthy and ready.

        Makes a minimal request to verify connectivity.

        Returns:
            True if server is healthy, False otherwise
        """
        try:
            response = await acompletion(
                model="openai/local-model",
                messages=[{"role": "user", "content": "Hi"}],
                api_base=f"{self._base_url}/v1",
                api_key="not-needed",
                timeout=10.0,
                max_tokens=1,
                stream=False,
            )
            return bool(response.choices)

        except Exception as e:
            self._logger.warning(
                "llamaserver_health_check_failed",
                base_url=self._base_url,
                error=str(e),
            )
            return False

    def get_stats(self) -> Dict[str, Any]:
        """Get provider statistics."""
        return {
            "base_url": self._base_url,
            "timeout": self._timeout,
            "max_retries": self._max_retries,
            "api_type": self._api_type,
            "backend": "llama-server (via LiteLLM)",
        }

    def __repr__(self) -> str:
        """String representation."""
        return f"LlamaServerProvider(base_url={self._base_url}, api_type={self._api_type})"
