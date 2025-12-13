"""Azure OpenAI LLM Provider.

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


class AzureAIFoundryProvider(LLMProvider):
    """Provider for Azure OpenAI API.

    This provider enables integration with Azure OpenAI Service deployments.
    Uses the OpenAI SDK with Azure-specific configuration for better
    compatibility and feature support. Provides enterprise-grade security,
    compliance, and scalability.
    """

    def __init__(
        self,
        endpoint: Optional[str] = None,
        api_key: Optional[str] = None,
        deployment_name: Optional[str] = None,
        api_version: str = "2024-02-01",
        timeout: int = 60,
        max_retries: int = 3,
    ):
        """Initialize Azure OpenAI provider.

        Args:
            endpoint: Azure endpoint URL (e.g., https://your-resource.openai.azure.com/)
            api_key: Azure API key (or set AZURE_API_KEY env var)
            deployment_name: Deployment name for the model (or set AZURE_DEPLOYMENT_NAME env var)
            api_version: Azure API version
            timeout: Request timeout in seconds
            max_retries: Number of retries on failure

        Raises:
            RuntimeError: If openai package is not installed
            ValueError: If endpoint or api_key is not provided
        """
        if not OPENAI_AVAILABLE:
            raise RuntimeError(
                "OpenAI package not installed. "
                "Install with: pip install openai"
            )

        if not endpoint:
            raise ValueError("Azure endpoint is required. Set AZURE_ENDPOINT env var.")
        if not api_key:
            raise ValueError("Azure API key is required. Set AZURE_API_KEY env var.")

        self.endpoint = endpoint
        self.deployment_name = deployment_name
        self.api_version = api_version
        self.timeout = timeout
        self.max_retries = max_retries

        # Use OpenAI SDK with Azure configuration (Microsoft's recommended approach)
        self.client = openai.AsyncAzureOpenAI(
            azure_endpoint=endpoint.rstrip('/'),
            api_key=api_key,
            api_version=api_version,
            timeout=timeout,
            max_retries=max_retries,
        )

    async def generate(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> str:
        """Generate text using Azure OpenAI API.

        Options mapping:
        - temperature: Controls randomness (0.0-2.0)
        - num_predict: Maps to max_tokens
        - num_ctx: Ignored (Azure handles context automatically)
        - json_mode: Enable JSON output format (response_format={"type": "json_object"})

        Args:
            model: Model deployment name (uses self.deployment_name if not specified)
            prompt: Input prompt text
            options: Provider-specific options

        Returns:
            Generated text string
        """
        if options is None:
            options = {}

        # Map llama-server-style options to Azure format
        temperature = options.get("temperature")
        max_tokens = options.get("num_predict", 400)
        json_mode = options.get("json_mode", False)

        # Use deployment name from config if model not specified or use model parameter
        deployment = self.deployment_name if self.deployment_name else model

        try:
            # Build request parameters
            # Note: GPT-4o-mini and newer models use max_completion_tokens instead of max_tokens
            request_params: Dict[str, Any] = {
                "model": deployment,
                "messages": [{"role": "user", "content": prompt}],
                "max_completion_tokens": max_tokens,
            }

            if temperature is not None:
                request_params["temperature"] = temperature

            # Add JSON mode if requested
            if json_mode:
                request_params["response_format"] = {"type": "json_object"}

            # Use OpenAI SDK's chat completions API
            response = await self.client.chat.completions.create(**request_params)

            return response.choices[0].message.content or ""

        except Exception as e:
            raise RuntimeError(f"Azure OpenAI generation failed: {str(e)}")

    async def generate_stream(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> AsyncIterator[TokenChunk]:
        """Generate text with streaming using Azure OpenAI API.

        Yields TokenChunk objects as tokens are generated.

        Options mapping:
        - temperature: Controls randomness (0.0-2.0)
        - num_predict: Maps to max_completion_tokens
        - json_mode: Enable JSON output format
        """
        if options is None:
            options = {}

        temperature = options.get("temperature")
        max_tokens = options.get("num_predict", 400)
        json_mode = options.get("json_mode", False)

        deployment = self.deployment_name if self.deployment_name else model

        try:
            request_params: Dict[str, Any] = {
                "model": deployment,
                "messages": [{"role": "user", "content": prompt}],
                "max_completion_tokens": max_tokens,
                "stream": True,
            }

            if temperature is not None:
                request_params["temperature"] = temperature

            if json_mode:
                request_params["response_format"] = {"type": "json_object"}

            stream = await self.client.chat.completions.create(**request_params)

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

        except Exception as e:
            raise RuntimeError(f"Azure OpenAI streaming failed: {str(e)}")

    @property
    def supports_streaming(self) -> bool:
        """Azure OpenAI supports native streaming."""
        return True

    async def health_check(self) -> bool:
        """Check if Azure OpenAI API is accessible."""
        try:
            # Make a minimal request to verify connectivity
            deployment = self.deployment_name or "test"
            await self.client.chat.completions.create(
                model=deployment,
                messages=[{"role": "user", "content": "Hi"}],
                max_completion_tokens=10,
            )
            return True
        except Exception:
            return False


__all__ = ["AzureAIFoundryProvider"]
