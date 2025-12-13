"""
llama-server provider for distributed deployment.

Uses the OpenAI-compatible HTTP API exposed by llama-server (llama.cpp).
This enables multi-node inference with llama.cpp backend while maintaining
a simple HTTP interface for the orchestrator.

**Constitutional Alignment:**
- P5 (Deterministic Spine): HTTP API is stateless, deterministic
- P6 (Observable): All requests/responses logged with structlog
- Sovereignty: llama-server runs on our hardware, no external dependencies

**Usage:**
    provider = LlamaServerProvider(base_url="http://node1:8080")
    response = await provider.generate(
        model="",  # Ignored (server has single model loaded)
        prompt="What is 2+2?",
        options={"temperature": 0.7, "num_predict": 100}
    )

**llama-server API Reference:**
    POST /completion - Text completion (OpenAI-style)
    GET /health - Health check
    GET /props - Model properties
"""

import asyncio
import json
from typing import AsyncIterator, Dict, Any, Optional

try:
    import httpx
except ImportError:
    httpx = None  # Will raise at runtime if used

from .base import LLMProvider, TokenChunk
from jeeves_avionics.logging import get_current_logger
from jeeves_protocols import LoggerProtocol


class LlamaServerProvider(LLMProvider):
    """
    llama-server (OpenAI-compatible) provider for distributed deployment.

    Connects to a llama-server instance running on a GPU node.
    The server loads a single model at startup, so the `model` parameter
    in generate() is ignored.

    Thread-safe: Uses httpx AsyncClient which handles connection pooling.

    Attributes:
        base_url: llama-server endpoint (e.g., "http://node1:8080")
        timeout: Request timeout in seconds
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
        Initialize llama-server provider.

        Args:
            base_url: llama-server endpoint (e.g., "http://node1:8080")
            timeout: Request timeout in seconds (default: 120s for large contexts)
            max_retries: Number of retries on transient failures
            api_type: API type - "native" for llama.cpp /completion,
                      "openai" for /v1/completions (avoids chat templates)
            logger: Logger for DI (uses context logger if not provided)

        Raises:
            ImportError: If httpx is not installed
        """
        self._logger = logger or get_current_logger()

        if httpx is None:
            raise ImportError(
                "httpx not installed. Install with: pip install httpx"
            )

        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.max_retries = max_retries
        self.api_type = api_type.lower()

        # Connection pooling with limits
        self._client = httpx.AsyncClient(
            base_url=self.base_url,
            timeout=httpx.Timeout(timeout, connect=10.0),
            headers={"Content-Type": "application/json"},
            limits=httpx.Limits(max_connections=10, max_keepalive_connections=5)
        )

        self._logger.info(
            "llamaserver_provider_initialized",
            base_url=base_url,
            timeout=timeout,
            api_type=api_type
        )

    async def generate(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None
    ) -> str:
        """
        Generate text completion via llama-server.

        IMPORTANT: The `model` parameter is ignored because llama-server
        loads a single model at startup. Use different server instances
        for different models (as in distributed deployment).

        Args:
            model: Model name (ignored - server has single model)
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
            httpx.HTTPStatusError: On HTTP errors (4xx, 5xx)
            httpx.TimeoutException: On request timeout
        """
        opts = options or {}

        # Build payload based on API type
        if self.api_type == "openai":
            # OpenAI-compatible /v1/completions endpoint
            # Uses text completion (NOT chat) to avoid model preprompts/templates
            payload = {
                "prompt": prompt,
                "max_tokens": opts.get("num_predict", 512),
                "temperature": opts.get("temperature", 0.7),
                "stop": opts.get("stop", []) or None,
                "top_p": opts.get("top_p", 0.95),
                "stream": False,
            }
            endpoint = "/v1/completions"
        else:
            # Native llama.cpp /completion endpoint
            payload = {
                "prompt": prompt,
                "n_predict": opts.get("num_predict", 512),
                "temperature": opts.get("temperature", 0.7),
                "stop": opts.get("stop", []),
                "top_p": opts.get("top_p", 0.95),
                "top_k": opts.get("top_k", 40),
                "repeat_penalty": opts.get("repeat_penalty", 1.1),
                "stream": False,
                "cache_prompt": True,
            }
            endpoint = "/completion"

        self._logger.debug(
            "llamaserver_generating",
            prompt_length=len(prompt),
            max_tokens=opts.get("num_predict", 512),
            temperature=opts.get("temperature", 0.7),
            api_type=self.api_type
        )

        # Retry logic for transient failures
        last_error = None
        for attempt in range(self.max_retries):
            try:
                response = await self._client.post(endpoint, json=payload)
                response.raise_for_status()
                result = response.json()

                # Extract response text based on API type
                if self.api_type == "openai":
                    # OpenAI format: {"choices": [{"text": "..."}]}
                    choices = result.get("choices", [])
                    content = choices[0].get("text", "") if choices else ""
                else:
                    # Native format: {"content": "..."}
                    content = result.get("content", "")

                # Log completion stats
                self._logger.info(
                    "llamaserver_generated",
                    prompt_tokens=result.get("tokens_evaluated", 0),
                    completion_tokens=result.get("tokens_predicted", 0),
                    stop_reason=result.get("stop_type", "unknown"),
                    generation_time_ms=result.get("timings", {}).get("predicted_ms", 0)
                )

                return content

            except httpx.TimeoutException as e:
                last_error = e
                self._logger.warning(
                    "llamaserver_timeout",
                    attempt=attempt + 1,
                    max_retries=self.max_retries,
                    base_url=self.base_url
                )
                if attempt < self.max_retries - 1:
                    await asyncio.sleep(2 ** attempt)  # Exponential backoff

            except httpx.HTTPStatusError as e:
                last_error = e
                if e.response.status_code >= 500:
                    # Server error - retry
                    self._logger.warning(
                        "llamaserver_server_error",
                        status=e.response.status_code,
                        attempt=attempt + 1
                    )
                    if attempt < self.max_retries - 1:
                        await asyncio.sleep(2 ** attempt)
                else:
                    # Client error - don't retry
                    raise

            except Exception as e:
                # Network error - retry
                last_error = e
                self._logger.warning(
                    "llamaserver_network_error",
                    error=str(e),
                    attempt=attempt + 1
                )
                if attempt < self.max_retries - 1:
                    await asyncio.sleep(2 ** attempt)

        # All retries exhausted
        self._logger.error(
            "llamaserver_generation_failed",
            base_url=self.base_url,
            error=str(last_error)
        )
        raise last_error

    async def generate_stream(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> AsyncIterator[TokenChunk]:
        """Generate text with streaming via llama-server.

        Uses Server-Sent Events (SSE) for real-time token streaming.
        Supports both native llama.cpp /completion and OpenAI-compatible /v1/completions.

        Args:
            model: Model name (ignored - server has single model)
            prompt: Input prompt text
            options: Generation options (temperature, num_predict, etc.)

        Yields:
            TokenChunk objects with incremental text
        """
        opts = options or {}

        # Build payload with streaming enabled
        if self.api_type == "openai":
            payload = {
                "prompt": prompt,
                "max_tokens": opts.get("num_predict", 512),
                "temperature": opts.get("temperature", 0.7),
                "stop": opts.get("stop", []) or None,
                "top_p": opts.get("top_p", 0.95),
                "stream": True,  # Enable streaming
            }
            endpoint = "/v1/completions"
        else:
            payload = {
                "prompt": prompt,
                "n_predict": opts.get("num_predict", 512),
                "temperature": opts.get("temperature", 0.7),
                "stop": opts.get("stop", []),
                "top_p": opts.get("top_p", 0.95),
                "top_k": opts.get("top_k", 40),
                "repeat_penalty": opts.get("repeat_penalty", 1.1),
                "stream": True,  # Enable streaming
                "cache_prompt": True,
            }
            endpoint = "/completion"

        self._logger.debug(
            "llamaserver_streaming",
            prompt_length=len(prompt),
            endpoint=endpoint,
        )

        try:
            async with self._client.stream(
                "POST",
                endpoint,
                json=payload,
                timeout=httpx.Timeout(self.timeout, connect=10.0),
            ) as response:
                response.raise_for_status()

                async for line in response.aiter_lines():
                    if not line:
                        continue

                    # SSE format: "data: {...}"
                    if line.startswith("data: "):
                        data_str = line[6:]  # Remove "data: " prefix

                        # Check for [DONE] signal
                        if data_str.strip() == "[DONE]":
                            yield TokenChunk(
                                text="",
                                is_final=True,
                                token_count=0,
                            )
                            break

                        try:
                            data = json.loads(data_str)

                            # Extract text based on API type
                            if self.api_type == "openai":
                                choices = data.get("choices", [])
                                if choices:
                                    text = choices[0].get("text", "")
                                    finish_reason = choices[0].get("finish_reason")
                                    is_final = finish_reason is not None
                                else:
                                    continue
                            else:
                                # Native llama.cpp format
                                text = data.get("content", "")
                                is_final = data.get("stop", False)

                            if text:
                                yield TokenChunk(
                                    text=text,
                                    is_final=is_final,
                                    token_count=1,
                                )

                            if is_final:
                                break

                        except json.JSONDecodeError:
                            # Skip malformed lines
                            continue

        except Exception as e:
            self._logger.error(
                "llamaserver_stream_failed",
                error=str(e),
                base_url=self.base_url,
            )
            raise

    @property
    def supports_streaming(self) -> bool:
        """LlamaServer supports native streaming via SSE."""
        return True

    async def health_check(self) -> bool:
        """
        Check if llama-server is healthy and ready.

        Returns:
            True if server is healthy, False otherwise
        """
        try:
            response = await self._client.get("/health", timeout=5.0)
            if response.status_code == 200:
                data = response.json()
                # llama-server returns {"status": "ok"} when ready
                return data.get("status") == "ok"
            return False
        except Exception as e:
            self._logger.warning(
                "llamaserver_health_check_failed",
                base_url=self.base_url,
                error=str(e)
            )
            return False

    async def get_model_info(self) -> Dict[str, Any]:
        """
        Get information about the loaded model.

        Returns:
            Dictionary with model properties (context size, etc.)
        """
        try:
            response = await self._client.get("/props", timeout=5.0)
            response.raise_for_status()
            return response.json()
        except Exception as e:
            self._logger.error(
                "llamaserver_get_props_failed",
                error=str(e)
            )
            return {}

    def get_stats(self) -> Dict[str, Any]:
        """
        Get provider statistics.

        Returns:
            Dictionary with provider configuration
        """
        return {
            "base_url": self.base_url,
            "timeout": self.timeout,
            "max_retries": self.max_retries,
            "api_type": self.api_type,
            "backend": "llama-server"
        }

    async def close(self):
        """Close the HTTP client and release connections."""
        await self._client.aclose()
        self._logger.info("llamaserver_provider_closed", base_url=self.base_url)

    def __repr__(self) -> str:
        """String representation."""
        return f"LlamaServerProvider(base_url={self.base_url}, api_type={self.api_type})"

    async def __aenter__(self):
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.close()
