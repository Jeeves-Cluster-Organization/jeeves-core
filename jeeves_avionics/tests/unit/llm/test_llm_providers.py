"""Unit tests for LLM provider system."""

import os
import json
import pytest
from unittest.mock import Mock, patch, AsyncMock
from jeeves_avionics.llm.providers import MockProvider, LLMProvider
from jeeves_avionics.llm.factory import create_llm_provider, create_agent_provider
from jeeves_avionics.capability_registry import get_capability_registry
from jeeves_avionics.settings import Settings

# Check SDK availability
OPENAI_AVAILABLE = False
ANTHROPIC_AVAILABLE = False
AZURE_AVAILABLE = False

# Try to import providers - will fail if SDKs not installed
try:
    from jeeves_avionics.llm.providers import AzureAIFoundryProvider
    AZURE_AVAILABLE = True
    OPENAI_AVAILABLE = True
except ImportError:
    AzureAIFoundryProvider = None

try:
    from jeeves_avionics.llm.providers import AnthropicProvider
    ANTHROPIC_AVAILABLE = True
except ImportError:
    AnthropicProvider = None

try:
    from jeeves_avionics.llm.providers import OpenAIProvider
    OPENAI_AVAILABLE = True
except ImportError:
    OpenAIProvider = None


class TestMockProvider:
    """Test MockProvider functionality."""

    @pytest.mark.asyncio
    async def test_mock_provider_planner_response(self):
        """Test that mock provider generates planner-style responses."""
        provider = MockProvider()

        result = await provider.generate(
            model="test",
            prompt="Generate a json execution plan for: Add task to test deletion",
            options={"temperature": 0.7}
        )

        assert "add_task" in result
        assert "execution_plan" in result
        assert "intent" in result

    @pytest.mark.asyncio
    async def test_mock_provider_validator_response(self):
        """Test that mock provider generates validator-style responses."""
        provider = MockProvider()

        result = await provider.generate(
            model="test",
            prompt='Generate natural language. Tools: {"status":"success"}',
            options={"temperature": 0.3}
        )

        assert len(result) > 0
        assert "completed" in result.lower() or "processed" in result.lower()

    @pytest.mark.asyncio
    async def test_mock_provider_meta_validator_response(self):
        """Test that mock provider generates meta-validator-style responses."""
        provider = MockProvider()

        result = await provider.generate(
            model="test",
            prompt="fact-checking validation rules",
            options={"temperature": 0}
        )

        assert "approved" in result
        assert "confidence" in result

    @pytest.mark.asyncio
    async def test_mock_provider_health_check(self):
        """Test that mock provider health check always succeeds."""
        provider = MockProvider()
        is_healthy = await provider.health_check()
        assert is_healthy is True

    def test_mock_provider_call_count(self):
        """Test that mock provider tracks call count."""
        provider = MockProvider()
        assert provider.call_count == 0

        # After async call, count should increment
        # (This would need to be tested in an async context)


class TestProviderFactory:
    """Test provider factory functions."""

    def test_create_mock_provider(self):
        """Test creating mock provider via factory."""
        settings = Settings(llm_provider="mock")
        provider = create_llm_provider("mock", settings)

        assert isinstance(provider, MockProvider)

    def test_create_invalid_provider(self):
        """Test that invalid provider type raises error."""
        settings = Settings()

        with pytest.raises(ValueError, match="Unknown provider type"):
            create_llm_provider("invalid_provider", settings)

    def test_create_agent_provider_default(self):
        """Test creating agent-specific provider with default."""
        # Create settings with mock provider as default
        settings = Settings(llm_provider="mock")

        provider = create_agent_provider(settings, "planner")
        assert isinstance(provider, MockProvider)

    def test_create_agent_provider_uses_default(self):
        """Test creating agent-specific provider uses default when no capability registered."""
        # After layer extraction, agent-specific overrides come from capability registry
        # Without registration, falls back to default provider
        settings = Settings(llm_provider="mock")

        provider = create_agent_provider(settings, "planner")
        assert isinstance(provider, MockProvider)

    def test_capability_registry_model_lookup(self):
        """Test capability registry model lookup.

        After layer extraction, model configuration is owned by capabilities
        via the DomainLLMRegistry. When no capability is registered,
        callers fall back to settings.default_model.
        """
        settings = Settings(default_model="test-model")
        registry = get_capability_registry()

        # Without capability registration, registry returns None
        config = registry.get_agent_config("planner")
        model = config.model if config else settings.default_model
        assert model == settings.default_model

        config = registry.get_agent_config("validator")
        model = config.model if config else settings.default_model
        assert model == settings.default_model


class TestConfiguration:
    """Test configuration system for providers."""

    def test_default_provider_configuration(self):
        """Test default provider is llamaserver."""
        os.environ.pop("LLM_PROVIDER", None)
        settings = Settings(_env_file=None)
        assert settings.llm_provider == "llamaserver"

    def test_provider_override_from_env(self):
        """Test provider can be overridden via constructor."""
        settings = Settings(llm_provider="mock")
        assert settings.llm_provider == "mock"

    def test_model_configuration(self):
        """Test model configuration.

        After layer extraction, per-agent model config is owned by capabilities.
        Settings only provides default_model for fallback.
        """
        settings = Settings()
        assert hasattr(settings, "default_model")
        # Per-agent models are now in capability registry, not Settings

    def test_api_key_configuration(self):
        """Test API key configuration."""
        settings = Settings()
        assert hasattr(settings, "openai_api_key")
        assert hasattr(settings, "anthropic_api_key")


class TestProviderInterface:
    """Test that all providers implement the interface correctly."""

    @pytest.mark.asyncio
    async def test_mock_provider_implements_interface(self):
        """Test that MockProvider implements LLMProvider interface."""
        provider = MockProvider()

        # Test generate method exists and is async
        assert hasattr(provider, "generate")
        assert callable(provider.generate)

        # Test health_check method exists and is async
        assert hasattr(provider, "health_check")
        assert callable(provider.health_check)

        # Test they can be called
        result = await provider.generate("test", "test prompt")
        assert isinstance(result, str)

        health = await provider.health_check()
        assert isinstance(health, bool)


@pytest.mark.requires_azure
class TestAzureProvider:
    """Test Azure AI Foundry provider functionality."""

    def test_azure_provider_initialization(self):
        """Test that Azure provider initializes with required parameters."""
        with patch('openai.AsyncAzureOpenAI'):
            provider = AzureAIFoundryProvider(
                endpoint="https://test.openai.azure.com/",
                api_key="test-key",
                deployment_name="test-deployment"
            )

            assert provider.endpoint == "https://test.openai.azure.com/"
            assert provider.deployment_name == "test-deployment"
            assert provider.api_version == "2024-02-01"

    def test_azure_provider_missing_endpoint(self):
        """Test that Azure provider raises error without endpoint."""
        with pytest.raises(ValueError, match="Azure endpoint is required"):
            AzureAIFoundryProvider(
                endpoint=None,
                api_key="test-key"
            )

    def test_azure_provider_missing_api_key(self):
        """Test that Azure provider raises error without API key."""
        with pytest.raises(ValueError, match="Azure API key is required"):
            AzureAIFoundryProvider(
                endpoint="https://test.openai.azure.com/",
                api_key=None
            )

    @pytest.mark.asyncio
    async def test_azure_provider_generate(self):
        """Test Azure provider generate method."""
        with patch('openai.AsyncAzureOpenAI') as mock_client:
            # Mock the completion response
            mock_response = Mock()
            mock_response.choices = [Mock()]
            mock_response.choices[0].message = Mock()
            mock_response.choices[0].message.content = "Test response"

            mock_client_instance = Mock()
            mock_client_instance.chat = Mock()
            mock_client_instance.chat.completions = Mock()
            mock_client_instance.chat.completions.create = AsyncMock(return_value=mock_response)
            mock_client.return_value = mock_client_instance

            provider = AzureAIFoundryProvider(
                endpoint="https://test.openai.azure.com/",
                api_key="test-key",
                deployment_name="test-deployment"
            )

            result = await provider.generate(
                model="test-deployment",
                prompt="Test prompt",
                options={"temperature": 0.7, "num_predict": 100}
            )

            assert result == "Test response"

    @pytest.mark.asyncio
    async def test_azure_provider_health_check_success(self):
        """Test Azure provider health check succeeds."""
        with patch('openai.AsyncAzureOpenAI') as mock_client:
            # Mock successful health check
            mock_response = Mock()
            mock_response.choices = [Mock()]
            mock_response.choices[0].message = Mock()
            mock_response.choices[0].message.content = "Hi"

            mock_client_instance = Mock()
            mock_client_instance.chat = Mock()
            mock_client_instance.chat.completions = Mock()
            mock_client_instance.chat.completions.create = AsyncMock(return_value=mock_response)
            mock_client.return_value = mock_client_instance

            provider = AzureAIFoundryProvider(
                endpoint="https://test.openai.azure.com/",
                api_key="test-key",
                deployment_name="test-deployment"
            )

            is_healthy = await provider.health_check()
            assert is_healthy is True

    @pytest.mark.asyncio
    async def test_azure_provider_health_check_failure(self):
        """Test Azure provider health check handles failures."""
        with patch('openai.AsyncAzureOpenAI') as mock_client:
            # Mock failed health check
            mock_client_instance = Mock()
            mock_client_instance.chat = Mock()
            mock_client_instance.chat.completions = Mock()
            mock_client_instance.chat.completions.create = AsyncMock(side_effect=Exception("Connection failed"))
            mock_client.return_value = mock_client_instance

            provider = AzureAIFoundryProvider(
                endpoint="https://test.openai.azure.com/",
                api_key="test-key",
                deployment_name="test-deployment"
            )

            is_healthy = await provider.health_check()
            assert is_healthy is False


class TestAzureProviderFactory:
    """Test Azure provider creation via factory."""

    def test_azure_configuration_in_settings(self):
        """Test Azure configuration fields in settings."""
        settings = Settings()
        assert hasattr(settings, "azure_endpoint")
        assert hasattr(settings, "azure_api_key")
        assert hasattr(settings, "azure_deployment_name")
        assert hasattr(settings, "azure_api_version")

    @pytest.mark.requires_azure
    def test_create_azure_provider_via_factory(self):
        """Test creating Azure provider through factory."""
        settings = Settings(
            llm_provider="azure",
            azure_endpoint="https://test.openai.azure.com/",
            azure_api_key="test-key",
            azure_deployment_name="test-deployment"
        )

        with patch('openai.AsyncAzureOpenAI'):
            provider = create_llm_provider("azure", settings)
            assert provider is not None

    @pytest.mark.requires_azure
    def test_azure_provider_creation(self):
        """Test Azure provider can be created via factory.

        After layer extraction, hybrid deployments are configured via
        capability registry. This test verifies the Azure provider works.
        """
        settings = Settings(
            llm_provider="azure",
            azure_endpoint="https://test.openai.azure.com/",
            azure_api_key="test-key",
            azure_deployment_name="test-deployment"
        )

        with patch('openai.AsyncAzureOpenAI'):
            provider = create_llm_provider("azure", settings)
            assert provider is not None


# =============================================================================
# Anthropic Provider Tests
# =============================================================================

@pytest.mark.skip(reason="Mock-based tests conflict with installed anthropic package. Use integration tests with real API instead.")
class TestAnthropicProvider:
    """Test Anthropic provider functionality (mocked tests - skip when real package installed)."""

    def test_anthropic_unavailable_raises_runtime_error(self):
        """Test that missing anthropic package raises RuntimeError."""
        with patch('jeeves_avionics.llm.providers.anthropic.ANTHROPIC_AVAILABLE', False):
            from jeeves_avionics.llm.providers.anthropic import AnthropicProvider

            with pytest.raises(RuntimeError, match="Anthropic package not installed"):
                AnthropicProvider(api_key="test-key")

    @pytest.mark.asyncio
    async def test_anthropic_provider_initialization(self):
        """Test that Anthropic provider initializes with required parameters."""
        # Mock the anthropic module at sys.modules level
        mock_anthropic = Mock()
        mock_client = Mock()
        mock_anthropic.AsyncAnthropic = Mock(return_value=mock_client)

        with patch.dict('sys.modules', {'anthropic': mock_anthropic}), \
             patch('jeeves_avionics.llm.providers.anthropic.ANTHROPIC_AVAILABLE', True):

            from jeeves_avionics.llm.providers.anthropic import AnthropicProvider

            provider = AnthropicProvider(
                api_key="test-key",
                timeout=30,
                max_retries=2
            )

            # Verify client was created with correct params
            mock_anthropic.AsyncAnthropic.assert_called_once_with(
                api_key="test-key",
                timeout=30,
                max_retries=2
            )
            assert provider.client == mock_client

    @pytest.mark.asyncio
    async def test_anthropic_provider_generate(self):
        """Test Anthropic provider generate method."""
        # Mock response
        mock_content = Mock()
        mock_content.text = "Test response from Claude"
        mock_response = Mock()
        mock_response.content = [mock_content]

        # Mock client
        mock_client = Mock()
        mock_client.messages = Mock()
        mock_client.messages.create = AsyncMock(return_value=mock_response)

        mock_anthropic = Mock()
        mock_anthropic.AsyncAnthropic = Mock(return_value=mock_client)

        with patch.dict('sys.modules', {'anthropic': mock_anthropic}), \
             patch('jeeves_avionics.llm.providers.anthropic.ANTHROPIC_AVAILABLE', True):

            from jeeves_avionics.llm.providers.anthropic import AnthropicProvider

            provider = AnthropicProvider(api_key="test-key")

            result = await provider.generate(
                model="claude-3-5-sonnet-20241022",
                prompt="Test prompt",
                options={"temperature": 0.5, "num_predict": 200}
            )

            assert result == "Test response from Claude"

            # Verify API call
            mock_client.messages.create.assert_called_once()
            call_kwargs = mock_client.messages.create.call_args[1]
            assert call_kwargs["model"] == "claude-3-5-sonnet-20241022"
            assert call_kwargs["temperature"] == 0.5
            assert call_kwargs["max_tokens"] == 200
            assert call_kwargs["messages"] == [{"role": "user", "content": "Test prompt"}]

    @pytest.mark.asyncio
    async def test_anthropic_provider_generate_empty_response(self):
        """Test Anthropic provider handles empty response."""
        with patch('jeeves_avionics.llm.providers.anthropic.ANTHROPIC_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.anthropic.anthropic') as mock_anthropic:

            from jeeves_avionics.llm.providers.anthropic import AnthropicProvider

            # Mock empty response
            mock_response = Mock()
            mock_response.content = []

            mock_client = Mock()
            mock_client.messages = Mock()
            mock_client.messages.create = AsyncMock(return_value=mock_response)
            mock_anthropic.AsyncAnthropic = Mock(return_value=mock_client)

            provider = AnthropicProvider(api_key="test-key")
            result = await provider.generate("test-model", "test prompt")

            assert result == ""

    @pytest.mark.asyncio
    async def test_anthropic_provider_generate_stream(self):
        """Test Anthropic provider streaming generation."""
        with patch('jeeves_avionics.llm.providers.anthropic.ANTHROPIC_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.anthropic.anthropic') as mock_anthropic:

            from jeeves_avionics.llm.providers.anthropic import AnthropicProvider

            # Mock streaming response
            async def mock_text_stream():
                yield "Hello"
                yield " "
                yield "world"

            mock_stream = AsyncMock()
            mock_stream.__aenter__ = AsyncMock(return_value=mock_stream)
            mock_stream.__aexit__ = AsyncMock(return_value=None)
            mock_stream.text_stream = mock_text_stream()

            mock_client = Mock()
            mock_client.messages = Mock()
            mock_client.messages.stream = Mock(return_value=mock_stream)
            mock_anthropic.AsyncAnthropic = Mock(return_value=mock_client)

            provider = AnthropicProvider(api_key="test-key")

            chunks = []
            async for chunk in provider.generate_stream(
                model="claude-3-5-sonnet-20241022",
                prompt="Test",
                options={"temperature": 0.8}
            ):
                chunks.append(chunk)

            # Should have 3 text chunks + 1 final chunk
            assert len(chunks) == 4
            assert chunks[0].text == "Hello"
            assert chunks[0].is_final is False
            assert chunks[1].text == " "
            assert chunks[2].text == "world"
            assert chunks[3].text == ""
            assert chunks[3].is_final is True

    @pytest.mark.asyncio
    async def test_anthropic_provider_supports_streaming(self):
        """Test that Anthropic provider supports streaming."""
        with patch('jeeves_avionics.llm.providers.anthropic.ANTHROPIC_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.anthropic.anthropic'):

            from jeeves_avionics.llm.providers.anthropic import AnthropicProvider

            provider = AnthropicProvider(api_key="test-key")
            assert provider.supports_streaming is True

    @pytest.mark.asyncio
    async def test_anthropic_provider_health_check_success(self):
        """Test Anthropic provider health check succeeds."""
        with patch('jeeves_avionics.llm.providers.anthropic.ANTHROPIC_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.anthropic.anthropic') as mock_anthropic:

            from jeeves_avionics.llm.providers.anthropic import AnthropicProvider

            mock_content = Mock()
            mock_content.text = "Hi"
            mock_response = Mock()
            mock_response.content = [mock_content]

            mock_client = Mock()
            mock_client.messages = Mock()
            mock_client.messages.create = AsyncMock(return_value=mock_response)
            mock_anthropic.AsyncAnthropic = Mock(return_value=mock_client)

            provider = AnthropicProvider(api_key="test-key")
            is_healthy = await provider.health_check()

            assert is_healthy is True

    @pytest.mark.asyncio
    async def test_anthropic_provider_health_check_failure(self):
        """Test Anthropic provider health check handles failures."""
        with patch('jeeves_avionics.llm.providers.anthropic.ANTHROPIC_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.anthropic.anthropic') as mock_anthropic:

            from jeeves_avionics.llm.providers.anthropic import AnthropicProvider

            mock_client = Mock()
            mock_client.messages = Mock()
            mock_client.messages.create = AsyncMock(side_effect=Exception("API Error"))
            mock_anthropic.AsyncAnthropic = Mock(return_value=mock_client)

            provider = AnthropicProvider(api_key="test-key")
            is_healthy = await provider.health_check()

            assert is_healthy is False


# =============================================================================
# OpenAI Provider Tests
# =============================================================================

class TestOpenAIProvider:
    """Test OpenAI provider functionality."""

    def test_openai_unavailable_raises_runtime_error(self):
        """Test that missing openai package raises RuntimeError."""
        with patch('jeeves_avionics.llm.providers.openai.OPENAI_AVAILABLE', False):
            from jeeves_avionics.llm.providers.openai import OpenAIProvider

            with pytest.raises(RuntimeError, match="OpenAI package not installed"):
                OpenAIProvider(api_key="test-key")

    @pytest.mark.asyncio
    async def test_openai_provider_initialization(self):
        """Test that OpenAI provider initializes with required parameters."""
        with patch('jeeves_avionics.llm.providers.openai.OPENAI_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.openai.openai') as mock_openai:

            from jeeves_avionics.llm.providers.openai import OpenAIProvider

            mock_client = Mock()
            mock_openai.AsyncOpenAI = Mock(return_value=mock_client)

            provider = OpenAIProvider(
                api_key="test-key",
                timeout=45,
                max_retries=5
            )

            # Verify client was created with correct params
            mock_openai.AsyncOpenAI.assert_called_once_with(
                api_key="test-key",
                timeout=45,
                max_retries=5
            )
            assert provider.client == mock_client

    @pytest.mark.asyncio
    async def test_openai_provider_generate(self):
        """Test OpenAI provider generate method."""
        with patch('jeeves_avionics.llm.providers.openai.OPENAI_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.openai.openai') as mock_openai:

            from jeeves_avionics.llm.providers.openai import OpenAIProvider

            # Mock response
            mock_message = Mock()
            mock_message.content = "Response from GPT-4"
            mock_choice = Mock()
            mock_choice.message = mock_message
            mock_response = Mock()
            mock_response.choices = [mock_choice]

            # Mock client
            mock_client = Mock()
            mock_client.chat = Mock()
            mock_client.chat.completions = Mock()
            mock_client.chat.completions.create = AsyncMock(return_value=mock_response)
            mock_openai.AsyncOpenAI = Mock(return_value=mock_client)

            provider = OpenAIProvider(api_key="test-key")

            result = await provider.generate(
                model="gpt-4",
                prompt="Test prompt",
                options={"temperature": 0.9, "num_predict": 500}
            )

            assert result == "Response from GPT-4"

            # Verify API call
            mock_client.chat.completions.create.assert_called_once()
            call_kwargs = mock_client.chat.completions.create.call_args[1]
            assert call_kwargs["model"] == "gpt-4"
            assert call_kwargs["temperature"] == 0.9
            assert call_kwargs["max_tokens"] == 500
            assert call_kwargs["messages"] == [{"role": "user", "content": "Test prompt"}]

    @pytest.mark.asyncio
    async def test_openai_provider_generate_with_json_mode(self):
        """Test OpenAI provider with JSON mode enabled."""
        with patch('jeeves_avionics.llm.providers.openai.OPENAI_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.openai.openai') as mock_openai:

            from jeeves_avionics.llm.providers.openai import OpenAIProvider

            mock_message = Mock()
            mock_message.content = '{"key": "value"}'
            mock_choice = Mock()
            mock_choice.message = mock_message
            mock_response = Mock()
            mock_response.choices = [mock_choice]

            mock_client = Mock()
            mock_client.chat = Mock()
            mock_client.chat.completions = Mock()
            mock_client.chat.completions.create = AsyncMock(return_value=mock_response)
            mock_openai.AsyncOpenAI = Mock(return_value=mock_client)

            provider = OpenAIProvider(api_key="test-key")

            result = await provider.generate(
                model="gpt-4",
                prompt="Generate JSON",
                options={"json_mode": True}
            )

            assert result == '{"key": "value"}'

            # Verify response_format was set
            call_kwargs = mock_client.chat.completions.create.call_args[1]
            assert call_kwargs["response_format"] == {"type": "json_object"}

    @pytest.mark.asyncio
    async def test_openai_provider_generate_empty_response(self):
        """Test OpenAI provider handles None content."""
        with patch('jeeves_avionics.llm.providers.openai.OPENAI_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.openai.openai') as mock_openai:

            from jeeves_avionics.llm.providers.openai import OpenAIProvider

            mock_message = Mock()
            mock_message.content = None
            mock_choice = Mock()
            mock_choice.message = mock_message
            mock_response = Mock()
            mock_response.choices = [mock_choice]

            mock_client = Mock()
            mock_client.chat = Mock()
            mock_client.chat.completions = Mock()
            mock_client.chat.completions.create = AsyncMock(return_value=mock_response)
            mock_openai.AsyncOpenAI = Mock(return_value=mock_client)

            provider = OpenAIProvider(api_key="test-key")
            result = await provider.generate("gpt-4", "test")

            assert result == ""

    @pytest.mark.asyncio
    async def test_openai_provider_generate_stream(self):
        """Test OpenAI provider streaming generation."""
        with patch('jeeves_avionics.llm.providers.openai.OPENAI_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.openai.openai') as mock_openai:

            from jeeves_avionics.llm.providers.openai import OpenAIProvider

            # Mock streaming response
            async def mock_stream():
                # First chunk
                delta1 = Mock()
                delta1.content = "Hello"
                choice1 = Mock()
                choice1.delta = delta1
                choice1.finish_reason = None
                chunk1 = Mock()
                chunk1.choices = [choice1]
                chunk1.model = "gpt-4"
                yield chunk1

                # Second chunk
                delta2 = Mock()
                delta2.content = " world"
                choice2 = Mock()
                choice2.delta = delta2
                choice2.finish_reason = "stop"
                chunk2 = Mock()
                chunk2.choices = [choice2]
                chunk2.model = "gpt-4"
                yield chunk2

            mock_client = Mock()
            mock_client.chat = Mock()
            mock_client.chat.completions = Mock()
            mock_client.chat.completions.create = AsyncMock(return_value=mock_stream())
            mock_openai.AsyncOpenAI = Mock(return_value=mock_client)

            provider = OpenAIProvider(api_key="test-key")

            chunks = []
            async for chunk in provider.generate_stream(
                model="gpt-4",
                prompt="Test",
                options={"temperature": 0.7}
            ):
                chunks.append(chunk)

            assert len(chunks) == 2
            assert chunks[0].text == "Hello"
            assert chunks[0].is_final is False
            assert chunks[1].text == " world"
            assert chunks[1].is_final is True

    @pytest.mark.asyncio
    async def test_openai_provider_supports_streaming(self):
        """Test that OpenAI provider supports streaming."""
        with patch('jeeves_avionics.llm.providers.openai.OPENAI_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.openai.openai'):

            from jeeves_avionics.llm.providers.openai import OpenAIProvider

            provider = OpenAIProvider(api_key="test-key")
            assert provider.supports_streaming is True

    @pytest.mark.asyncio
    async def test_openai_provider_health_check_success(self):
        """Test OpenAI provider health check succeeds."""
        with patch('jeeves_avionics.llm.providers.openai.OPENAI_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.openai.openai') as mock_openai:

            from jeeves_avionics.llm.providers.openai import OpenAIProvider

            mock_client = Mock()
            mock_client.models = Mock()
            mock_client.models.list = AsyncMock(return_value=[])
            mock_openai.AsyncOpenAI = Mock(return_value=mock_client)

            provider = OpenAIProvider(api_key="test-key")
            is_healthy = await provider.health_check()

            assert is_healthy is True

    @pytest.mark.asyncio
    async def test_openai_provider_health_check_failure(self):
        """Test OpenAI provider health check handles failures."""
        with patch('jeeves_avionics.llm.providers.openai.OPENAI_AVAILABLE', True), \
             patch('jeeves_avionics.llm.providers.openai.openai') as mock_openai:

            from jeeves_avionics.llm.providers.openai import OpenAIProvider

            mock_client = Mock()
            mock_client.models = Mock()
            mock_client.models.list = AsyncMock(side_effect=Exception("Connection error"))
            mock_openai.AsyncOpenAI = Mock(return_value=mock_client)

            provider = OpenAIProvider(api_key="test-key")
            is_healthy = await provider.health_check()

            assert is_healthy is False


# =============================================================================
# LlamaServer Provider Tests
# =============================================================================

@pytest.mark.skip(reason="Old tests for legacy LlamaServer implementation. See test_llamaserver_provider_new.py for current tests")
class TestLlamaServerProvider:
    """Test LlamaServer provider functionality (LEGACY - refactored to use Airframe)."""

    def test_llamaserver_missing_httpx_raises_import_error(self):
        """Test that missing httpx raises ImportError."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.httpx', None):
            from jeeves_avionics.llm.providers.llamaserver_provider import LlamaServerProvider

            with pytest.raises(ImportError, match="httpx not installed"):
                LlamaServerProvider(base_url="http://localhost:8080")

    @pytest.mark.asyncio
    async def test_llamaserver_provider_initialization(self):
        """Test LlamaServer provider initialization."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.httpx') as mock_httpx, \
             patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):

            from jeeves_avionics.llm.providers.llamaserver_provider import LlamaServerProvider

            mock_client = Mock()
            mock_httpx.AsyncClient = Mock(return_value=mock_client)
            mock_httpx.Timeout = Mock(return_value="timeout_obj")
            mock_httpx.Limits = Mock(return_value="limits_obj")

            provider = LlamaServerProvider(
                base_url="http://node1:8080/",
                timeout=60.0,
                max_retries=5,
                api_type="native"
            )

            assert provider.base_url == "http://node1:8080"
            assert provider.timeout == 60.0
            assert provider.max_retries == 5
            assert provider.api_type == "native"

    @pytest.mark.asyncio
    async def test_llamaserver_provider_generate_native_api(self):
        """Test LlamaServer provider generate with native API."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.httpx') as mock_httpx, \
             patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):

            from jeeves_avionics.llm.providers.llamaserver_provider import LlamaServerProvider

            # Mock HTTP response
            mock_response = Mock()
            mock_response.json = Mock(return_value={"content": "Generated text"})
            mock_response.raise_for_status = Mock()

            mock_client = Mock()
            mock_client.post = AsyncMock(return_value=mock_response)
            mock_httpx.AsyncClient = Mock(return_value=mock_client)
            mock_httpx.Timeout = Mock(return_value="timeout")
            mock_httpx.Limits = Mock(return_value="limits")

            provider = LlamaServerProvider(
                base_url="http://localhost:8080",
                api_type="native"
            )

            result = await provider.generate(
                model="ignored",
                prompt="Test prompt",
                options={"temperature": 0.7, "num_predict": 100}
            )

            assert result == "Generated text"

            # Verify POST request
            mock_client.post.assert_called_once()
            call_args = mock_client.post.call_args
            assert call_args[0][0] == "/completion"
            request_data = json.loads(call_args[1]["content"])
            assert request_data["prompt"] == "Test prompt"
            assert request_data["temperature"] == 0.7
            assert request_data["n_predict"] == 100

    @pytest.mark.asyncio
    async def test_llamaserver_provider_health_check_success(self):
        """Test LlamaServer provider health check succeeds."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.httpx') as mock_httpx, \
             patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):

            from jeeves_avionics.llm.providers.llamaserver_provider import LlamaServerProvider

            mock_response = Mock()
            mock_response.status_code = 200
            mock_response.json = Mock(return_value={"status": "ok"})

            mock_client = Mock()
            mock_client.get = AsyncMock(return_value=mock_response)
            mock_httpx.AsyncClient = Mock(return_value=mock_client)
            mock_httpx.Timeout = Mock(return_value="timeout")
            mock_httpx.Limits = Mock(return_value="limits")

            provider = LlamaServerProvider(base_url="http://localhost:8080")
            is_healthy = await provider.health_check()

            assert is_healthy is True
            mock_client.get.assert_called_once()

    @pytest.mark.asyncio
    async def test_llamaserver_provider_health_check_failure(self):
        """Test LlamaServer provider health check handles failures."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.httpx') as mock_httpx, \
             patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):

            from jeeves_avionics.llm.providers.llamaserver_provider import LlamaServerProvider

            mock_client = Mock()
            mock_client.get = AsyncMock(side_effect=Exception("Connection refused"))
            mock_httpx.AsyncClient = Mock(return_value=mock_client)
            mock_httpx.Timeout = Mock(return_value="timeout")
            mock_httpx.Limits = Mock(return_value="limits")

            provider = LlamaServerProvider(base_url="http://localhost:8080")
            is_healthy = await provider.health_check()

            assert is_healthy is False

    @pytest.mark.asyncio
    async def test_llamaserver_provider_supports_streaming(self):
        """Test that LlamaServer provider supports streaming."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.httpx'), \
             patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):

            from jeeves_avionics.llm.providers.llamaserver_provider import LlamaServerProvider

            provider = LlamaServerProvider(base_url="http://localhost:8080")
            assert provider.supports_streaming is True


@pytest.fixture(autouse=True)
def reset_environment():
    """Reset environment variables before each test."""
    # Store original values
    original_env = os.environ.copy()

    # Set defaults for testing
    os.environ["LLM_PROVIDER"] = "mock"

    yield

    # Restore original environment
    os.environ.clear()
    os.environ.update(original_env)
