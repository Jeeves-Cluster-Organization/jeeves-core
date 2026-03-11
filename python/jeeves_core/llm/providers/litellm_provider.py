"""
LiteLLM provider - unified provider for 100+ LLM backends.

Message-based chat interface with native tool calling.
LiteLLM handles: HTTP, retries, streaming, provider quirks.
This provider adds: cost tracking, logging, tool_calls parsing.

Supports:
- OpenAI, Azure OpenAI, Anthropic, Cohere, etc. (cloud)
- llama.cpp, vLLM, Ollama, etc. (local via OpenAI-compatible API)
- AWS Bedrock, Google Vertex AI, etc. (cloud with special auth)

Usage:
    provider = LiteLLMProvider(model="claude-3-sonnet-20240229")
    result = await provider.chat(
        model="claude-3-sonnet",
        messages=[{"role": "user", "content": "Hello"}],
        options={"tools": [...], "tool_choice": "auto"},
    )
"""

from typing import AsyncIterator, Dict, Any, List, Optional

from .base import LLMProvider, TokenChunk
from ..cost_calculator import CostCalculator
from jeeves_core.logging import get_current_logger
from jeeves_core.protocols import LoggerProtocol

try:
    from litellm import acompletion
except ImportError:
    acompletion = None  # type: ignore


def _parse_tool_calls(message) -> list:
    """Extract tool_calls from a LiteLLM response message into dicts."""
    raw = getattr(message, "tool_calls", None)
    if not raw:
        return []
    calls = []
    for tc in raw:
        calls.append({
            "id": getattr(tc, "id", ""),
            "type": "function",
            "function": {
                "name": getattr(tc.function, "name", ""),
                "arguments": getattr(tc.function, "arguments", "{}"),
            },
        })
    return calls


class LiteLLMProvider(LLMProvider):
    """
    LLM provider using LiteLLM for unified access to 100+ backends.

    This is the recommended provider for all LLM interactions.

    Attributes:
        model: LiteLLM model string (e.g., "claude-3-sonnet-20240229", "openai/gpt-4")
        api_base: Optional API base URL for local/custom endpoints
        api_key: Optional API key (can also be set via environment)
        timeout: Request timeout in seconds
        max_retries: Number of retries on transient failures
    """

    def __init__(
        self,
        model: str,
        api_base: Optional[str] = None,
        api_key: Optional[str] = None,
        timeout: float = 120.0,
        max_retries: int = 3,
        logger: Optional[LoggerProtocol] = None,
        **extra_params,
    ):
        if acompletion is None:
            raise ImportError("litellm is required: pip install litellm")

        self._logger = logger or get_current_logger()
        self._model = model
        self._api_base = api_base
        self._api_key = api_key
        self._timeout = timeout
        self._max_retries = max_retries
        self._extra_params = extra_params
        self._cost_calculator = CostCalculator()

        self._logger.info(
            "litellm_provider_initialized",
            model=model,
            api_base=api_base,
            timeout=timeout,
        )

    async def chat(
        self, model: str, messages: List[Dict[str, Any]], options: Optional[Dict[str, Any]] = None
    ) -> Dict[str, Any]:
        result, _usage = await self.chat_with_usage(model, messages, options)
        return result

    async def chat_with_usage(
        self, model: str, messages: List[Dict[str, Any]], options: Optional[Dict[str, Any]] = None
    ) -> tuple[Dict[str, Any], Optional[Dict[str, int]]]:
        """Chat completion via LiteLLM with usage tracking."""
        opts = options or {}
        model_to_use = model or self._model

        kwargs = self._build_kwargs(messages, opts, stream=False)

        self._logger.debug(
            "litellm_chat",
            model=model_to_use,
            message_count=len(messages),
            has_tools=bool(opts.get("tools")),
        )

        try:
            response = await acompletion(
                model=model_to_use,
                **kwargs,
            )

            message = response.choices[0].message
            content = message.content or ""
            tool_calls = _parse_tool_calls(message)

            # Extract usage for cost tracking
            usage = getattr(response, "usage", None)
            prompt_tokens = getattr(usage, "prompt_tokens", 0) if usage else 0
            completion_tokens = getattr(usage, "completion_tokens", 0) if usage else 0

            # Calculate cost
            cost_metrics = self._cost_calculator.calculate_cost(
                provider="litellm",
                model=model_to_use,
                prompt_tokens=prompt_tokens,
                completion_tokens=completion_tokens,
            )

            self._logger.info(
                "llm_chat_complete",
                provider="litellm",
                model=model_to_use,
                prompt_tokens=prompt_tokens,
                completion_tokens=completion_tokens,
                cost_usd=cost_metrics.cost_usd,
                tool_calls_count=len(tool_calls),
            )

            result = {"content": content, "tool_calls": tool_calls}
            usage_dict = {
                "prompt_tokens": int(prompt_tokens),
                "completion_tokens": int(completion_tokens),
            }
            return result, usage_dict

        except Exception as e:
            self._logger.error(
                "litellm_error",
                model=model_to_use,
                error=str(e),
            )
            raise

    async def chat_stream(
        self, model: str, messages: List[Dict[str, Any]], options: Optional[Dict[str, Any]] = None
    ) -> AsyncIterator[TokenChunk]:
        """Chat completion with streaming via LiteLLM."""
        opts = options or {}
        model_to_use = model or self._model

        kwargs = self._build_kwargs(messages, opts, stream=True)

        self._logger.debug(
            "litellm_streaming",
            model=model_to_use,
            message_count=len(messages),
        )

        try:
            response = await acompletion(
                model=model_to_use,
                **kwargs,
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

            # Ensure we always emit final chunk
            yield TokenChunk(text="", is_final=True, token_count=0)

        except Exception as e:
            self._logger.error(
                "litellm_stream_error",
                model=model_to_use,
                error=str(e),
            )
            raise

    def _build_kwargs(
        self, messages: List[Dict[str, Any]], options: Dict[str, Any], stream: bool
    ) -> Dict[str, Any]:
        """Build kwargs for litellm.acompletion."""
        kwargs: Dict[str, Any] = {
            "messages": messages,
            "stream": stream,
            "timeout": self._timeout,
            "num_retries": self._max_retries,
        }

        # API configuration
        if self._api_base:
            kwargs["api_base"] = self._api_base
        if self._api_key:
            kwargs["api_key"] = self._api_key

        # Tool calling
        if options.get("tools"):
            kwargs["tools"] = options["tools"]
        if options.get("tool_choice"):
            kwargs["tool_choice"] = options["tool_choice"]

        # Generation parameters
        if options.get("temperature") is not None:
            kwargs["temperature"] = options["temperature"]

        # Handle max_tokens (support both names)
        max_tokens = options.get("max_tokens") or options.get("num_predict")
        if max_tokens is not None:
            kwargs["max_tokens"] = max_tokens

        # Pass through extra params from init and options
        kwargs.update(self._extra_params)
        for key in ["top_p", "top_k", "stop", "presence_penalty", "frequency_penalty"]:
            if key in options:
                kwargs[key] = options[key]

        return kwargs

    @property
    def supports_streaming(self) -> bool:
        """LiteLLM supports streaming for most providers."""
        return True

    async def health_check(self) -> bool:
        """Check if the LLM endpoint is healthy."""
        try:
            kwargs = self._build_kwargs(
                [{"role": "user", "content": "Hi"}], {}, stream=False
            )
            kwargs["max_tokens"] = 1

            response = await acompletion(
                model=self._model,
                **kwargs,
            )
            return bool(response.choices)

        except Exception as e:
            self._logger.warning(
                "litellm_health_check_failed",
                model=self._model,
                error=str(e),
            )
            return False

    def get_stats(self) -> Dict[str, Any]:
        """Get provider statistics."""
        return {
            "provider": "litellm",
            "model": self._model,
            "api_base": self._api_base,
            "timeout": self._timeout,
            "max_retries": self._max_retries,
        }

    def __repr__(self) -> str:
        return f"LiteLLMProvider(model={self._model}, api_base={self._api_base})"
