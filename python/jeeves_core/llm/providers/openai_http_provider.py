"""OpenAI-compatible HTTP provider.

Message-based chat interface with native tool calling.
Direct HTTP adapter for OpenAI-compatible endpoints:
- llama-server (llama.cpp)
- vLLM
- SGLang
- Any OpenAI API-compatible server

No external dependencies beyond httpx (already in jeeves_core deps).
"""

from typing import AsyncIterator, Dict, Any, List, Optional
import json

import httpx

from .base import LLMProvider, TokenChunk
from jeeves_core.logging import get_current_logger
from jeeves_core.protocols import LoggerProtocol


def _parse_tool_calls_from_dict(message: dict) -> list:
    """Extract tool_calls from an OpenAI-format response message dict."""
    raw = message.get("tool_calls")
    if not raw:
        return []
    calls = []
    for tc in raw:
        calls.append({
            "id": tc.get("id", ""),
            "type": "function",
            "function": {
                "name": tc.get("function", {}).get("name", ""),
                "arguments": tc.get("function", {}).get("arguments", "{}"),
            },
        })
    return calls


class OpenAIHTTPProvider(LLMProvider):
    """LLM provider using direct OpenAI-compatible HTTP calls.

    This is the zero-dependency default adapter. Works with any server
    that implements the OpenAI chat completions API.
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
        self._logger = logger or get_current_logger()
        self._model = model
        self._api_base = (api_base or "http://localhost:8080/v1").rstrip("/")
        self._api_key = api_key
        self._timeout = timeout
        self._max_retries = max_retries
        self._extra_params = extra_params

        self._logger.info(
            "openai_http_provider_initialized",
            model=model,
            api_base=self._api_base,
        )

    async def chat(
        self,
        model: str,
        messages: List[Dict[str, Any]],
        options: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        result, _usage = await self.chat_with_usage(model, messages, options)
        return result

    async def chat_with_usage(
        self,
        model: str,
        messages: List[Dict[str, Any]],
        options: Optional[Dict[str, Any]] = None,
    ) -> tuple[Dict[str, Any], Optional[Dict[str, int]]]:
        """Chat completion via OpenAI-compatible API."""
        opts = options or {}
        model_to_use = model or self._model

        payload = self._build_payload(messages, opts, stream=False)

        self._logger.debug(
            "openai_http_chat",
            model=model_to_use,
            message_count=len(messages),
            has_tools=bool(opts.get("tools")),
        )

        headers = self._build_headers()

        async with httpx.AsyncClient(timeout=self._timeout) as client:
            for attempt in range(self._max_retries + 1):
                try:
                    response = await client.post(
                        f"{self._api_base}/chat/completions",
                        json=payload,
                        headers=headers,
                    )
                    response.raise_for_status()

                    data = response.json()
                    message = data["choices"][0]["message"]
                    content = message.get("content") or ""
                    tool_calls = _parse_tool_calls_from_dict(message)

                    usage = data.get("usage", {})
                    prompt_tokens = usage.get("prompt_tokens")
                    completion_tokens = usage.get("completion_tokens")
                    usage_payload: Optional[Dict[str, int]] = None
                    if isinstance(prompt_tokens, int) and isinstance(completion_tokens, int):
                        usage_payload = {
                            "prompt_tokens": prompt_tokens,
                            "completion_tokens": completion_tokens,
                        }

                    self._logger.info(
                        "openai_http_chat_complete",
                        model=model_to_use,
                        response_length=len(content),
                        tool_calls_count=len(tool_calls),
                    )

                    result = {"content": content, "tool_calls": tool_calls}
                    return result, usage_payload

                except httpx.HTTPStatusError as e:
                    if attempt == self._max_retries:
                        self._logger.error(
                            "openai_http_error",
                            model=model_to_use,
                            status=e.response.status_code,
                            error=str(e),
                        )
                        raise
                    self._logger.warning(
                        "openai_http_retry",
                        attempt=attempt + 1,
                        status=e.response.status_code,
                    )
                except httpx.RequestError as e:
                    if attempt == self._max_retries:
                        self._logger.error(
                            "openai_http_connection_error",
                            model=model_to_use,
                            error=str(e),
                        )
                        raise
                    self._logger.warning(
                        "openai_http_retry",
                        attempt=attempt + 1,
                        error=str(e),
                    )

        raise RuntimeError("Unreachable")

    async def chat_stream(
        self,
        model: str,
        messages: List[Dict[str, Any]],
        options: Optional[Dict[str, Any]] = None,
    ) -> AsyncIterator[TokenChunk]:
        """Chat completion with streaming via SSE."""
        opts = options or {}
        model_to_use = model or self._model

        payload = self._build_payload(messages, opts, stream=True)
        headers = self._build_headers()

        self._logger.debug(
            "openai_http_streaming",
            model=model_to_use,
            message_count=len(messages),
        )

        async with httpx.AsyncClient(timeout=self._timeout) as client:
            async with client.stream(
                "POST",
                f"{self._api_base}/chat/completions",
                json=payload,
                headers=headers,
            ) as response:
                response.raise_for_status()

                async for line in response.aiter_lines():
                    if not line.startswith("data: "):
                        continue

                    data_str = line[6:]  # Remove "data: " prefix

                    if data_str.strip() == "[DONE]":
                        yield TokenChunk(text="", is_final=True, token_count=0)
                        return

                    try:
                        data = json.loads(data_str)
                    except json.JSONDecodeError:
                        continue

                    choices = data.get("choices", [])
                    if not choices:
                        continue

                    delta = choices[0].get("delta", {})
                    content = delta.get("content")
                    finish_reason = choices[0].get("finish_reason")

                    if content:
                        yield TokenChunk(
                            text=content,
                            is_final=False,
                            token_count=1,
                        )

                    if finish_reason:
                        yield TokenChunk(text="", is_final=True, token_count=0)
                        return

        # Ensure final chunk
        yield TokenChunk(text="", is_final=True, token_count=0)

    def _build_payload(
        self,
        messages: List[Dict[str, Any]],
        options: Dict[str, Any],
        stream: bool,
    ) -> Dict[str, Any]:
        """Build request payload."""
        payload: Dict[str, Any] = {
            "model": self._model,
            "messages": messages,
            "stream": stream,
        }

        # Tool calling
        if options.get("tools"):
            payload["tools"] = options["tools"]
        if options.get("tool_choice"):
            payload["tool_choice"] = options["tool_choice"]

        if options.get("temperature") is not None:
            payload["temperature"] = options["temperature"]

        max_tokens = options.get("max_tokens") or options.get("num_predict")
        if max_tokens is not None:
            payload["max_tokens"] = max_tokens

        for key in ["top_p", "stop", "presence_penalty", "frequency_penalty"]:
            if key in options:
                payload[key] = options[key]

        return payload

    def _build_headers(self) -> Dict[str, str]:
        """Build request headers."""
        headers = {
            "Content-Type": "application/json",
        }
        if self._api_key:
            headers["Authorization"] = f"Bearer {self._api_key}"
        return headers

    @property
    def supports_streaming(self) -> bool:
        return True

    async def health_check(self) -> bool:
        """Check if endpoint is healthy."""
        try:
            async with httpx.AsyncClient(timeout=10.0) as client:
                response = await client.get(
                    f"{self._api_base}/models",
                    headers=self._build_headers(),
                )
                return response.status_code == 200
        except Exception as e:
            self._logger.warning(
                "openai_http_health_check_failed",
                error=str(e),
            )
            return False

    def __repr__(self) -> str:
        return f"OpenAIHTTPProvider(model={self._model}, api_base={self._api_base})"
