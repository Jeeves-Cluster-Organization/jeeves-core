"""
LlamaServer provider - REFACTORED to delegate to Airframe adapter.

This provider wraps Airframe's LlamaServerAdapter and adds:
- Cost tracking (tokens → USD)
- Settings integration
- Request translation (Avionics API → Airframe API)
- Structured logging with cost metrics

Airframe handles:
- HTTP requests and SSE parsing
- Retry logic and error categorization
- Backend protocol quirks
"""

import asyncio
from typing import AsyncIterator, Dict, Any, Optional

# Airframe imports (L1 substrate)
from airframe.adapters import LlamaServerAdapter
from airframe.endpoints import EndpointSpec, BackendKind
from airframe.types import (
    InferenceRequest,
    Message,
    InferenceStreamEvent,
    StreamEventType,
)

# Avionics imports (L3 orchestration)
from .base import LLMProvider, TokenChunk
from ..cost_calculator import get_cost_calculator
from avionics.logging import get_current_logger
from protocols import LoggerProtocol


class LlamaServerProvider(LLMProvider):
    """
    LlamaServer provider that delegates to Airframe's LlamaServerAdapter.

    Responsibilities:
    - Convert Avionics API (generate/generate_stream) to Airframe API (stream_infer)
    - Track token usage and calculate costs
    - Enrich telemetry with cost metrics
    - Handle provider-specific settings

    Thread-safe: Airframe's httpx AsyncClient handles connection pooling.

    Attributes:
        base_url: llama-server endpoint (e.g., "http://node1:8080")
        timeout: Request timeout in seconds
        max_retries: Number of retries on transient failures
        api_type: "native" for llama.cpp /completion, "openai" for /v1/completions
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
            api_type: API type - "native" for llama.cpp /completion,
                      "openai" for /v1/completions (avoids chat templates)
            logger: Logger for DI (uses context logger if not provided)
        """
        self._logger = logger or get_current_logger()

        # Airframe adapter (handles backend protocol)
        self._adapter = LlamaServerAdapter(timeout=timeout, max_retries=max_retries)

        # Endpoint specification for Airframe
        self._endpoint = EndpointSpec(
            name="llamaserver",
            base_url=base_url,
            backend_kind=BackendKind.LLAMA_SERVER,
            api_type=api_type,
            tags={"deployment": "local", "backend": "llama.cpp"},
        )

        # Avionics-specific features
        self._cost_calculator = get_cost_calculator()

        self._logger.info(
            "llamaserver_provider_initialized",
            base_url=base_url,
            timeout=timeout,
            api_type=api_type,
        )

    async def generate(
        self, model: str, prompt: str, options: Optional[Dict[str, Any]] = None
    ) -> str:
        """
        Generate text completion via llama-server.

        Delegates to Airframe's LlamaServerAdapter for HTTP/SSE handling,
        then tracks cost and logs metrics.

        Args:
            model: Model name (ignored - llama-server has single model loaded)
            prompt: Input prompt text
            options: Generation options:
                - temperature: Sampling temperature (default: 0.7)
                - num_predict: Max tokens to generate (default: 512)
                - stop: List of stop sequences
                - top_p: Top-p sampling (default: 0.95)
                - top_k: Top-k sampling (default: 40)
                - repeat_penalty: Repetition penalty (default: 1.1)

        Returns:
            Generated text string

        Raises:
            Exception: On LLM errors (wrapped from Airframe)
        """
        opts = options or {}

        # Convert Avionics request to Airframe InferenceRequest
        request = InferenceRequest(
            messages=[Message(role="user", content=prompt)],
            model=model,
            temperature=opts.get("temperature"),
            max_tokens=opts.get("num_predict"),
            stream=False,  # For generate(), collect full response
            extra_params=opts,  # Pass through extra options
        )

        self._logger.debug(
            "llamaserver_generating",
            prompt_length=len(prompt),
            max_tokens=opts.get("num_predict", 512),
            temperature=opts.get("temperature", 0.7),
        )

        # Call Airframe adapter (delegation!)
        result_text = ""
        prompt_tokens = 0
        completion_tokens = 0

        async for event in self._adapter.stream_infer(self._endpoint, request):
            if event.type == StreamEventType.MESSAGE:
                result_text = event.content or ""
                # Extract token usage if available
                if event.usage:
                    prompt_tokens = event.usage.get("prompt_tokens", 0)
                    completion_tokens = event.usage.get("completion_tokens", 0)
            elif event.type == StreamEventType.ERROR:
                self._logger.error("llm_error", error=str(event.error))
                raise Exception(f"LLM Error: {event.error}")

        # Avionics adds cost tracking (Airframe doesn't know about costs)
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

        return result_text

    async def generate_stream(
        self, model: str, prompt: str, options: Optional[Dict[str, Any]] = None
    ) -> AsyncIterator[TokenChunk]:
        """
        Generate text with streaming via llama-server.

        Delegates to Airframe's LlamaServerAdapter for SSE streaming,
        converts InferenceStreamEvent → TokenChunk for Avionics API.

        Args:
            model: Model name (ignored - server has single model)
            prompt: Input prompt text
            options: Generation options (temperature, num_predict, etc.)

        Yields:
            TokenChunk objects with incremental text
        """
        opts = options or {}

        # Convert Avionics request to Airframe InferenceRequest
        request = InferenceRequest(
            messages=[Message(role="user", content=prompt)],
            model=model,
            temperature=opts.get("temperature"),
            max_tokens=opts.get("num_predict"),
            stream=True,  # Enable streaming
            extra_params=opts,
        )

        self._logger.debug(
            "llamaserver_streaming",
            prompt_length=len(prompt),
        )

        # Delegate to Airframe, convert events to Avionics TokenChunk
        async for event in self._adapter.stream_infer(self._endpoint, request):
            if event.type == StreamEventType.TOKEN:
                yield TokenChunk(
                    text=event.content or "",
                    is_final=False,
                    token_count=1,
                )
            elif event.type == StreamEventType.DONE:
                # Final event - no text, just completion signal
                yield TokenChunk(text="", is_final=True, token_count=0)
                # TODO: Track cost from event.usage if available
            elif event.type == StreamEventType.ERROR:
                self._logger.error("llm_stream_error", error=str(event.error))
                raise Exception(f"LLM Stream Error: {event.error}")

    @property
    def supports_streaming(self) -> bool:
        """LlamaServer supports native streaming via SSE."""
        return True

    async def health_check(self) -> bool:
        """
        Check if llama-server is healthy and ready.

        Delegates to Airframe adapter which checks /health endpoint.

        Returns:
            True if server is healthy, False otherwise
        """
        try:
            # Airframe health check via adapter
            # For now, make a simple inference request as health check
            # TODO: Add explicit health_check() to Airframe adapter
            request = InferenceRequest(
                messages=[Message(role="user", content="Hi")],
                model="",
                max_tokens=1,
                stream=False,
            )

            async for event in self._adapter.stream_infer(self._endpoint, request):
                if event.type == StreamEventType.MESSAGE:
                    return True
                elif event.type == StreamEventType.ERROR:
                    return False

            return False

        except Exception as e:
            self._logger.warning(
                "llamaserver_health_check_failed",
                base_url=self._endpoint.base_url,
                error=str(e),
            )
            return False

    def get_stats(self) -> Dict[str, Any]:
        """
        Get provider statistics.

        Returns:
            Dictionary with provider configuration
        """
        return {
            "base_url": self._endpoint.base_url,
            "timeout": self._adapter.timeout,
            "max_retries": self._adapter.max_retries,
            "api_type": self._endpoint.api_type,
            "backend": "llama-server (via Airframe)",
        }

    def __repr__(self) -> str:
        """String representation."""
        return f"LlamaServerProvider(base_url={self._endpoint.base_url}, api_type={self._endpoint.api_type})"
