"""Unit tests for LLM provider system."""

import os
import pytest
from unittest.mock import Mock, patch, AsyncMock
from jeeves_avionics.llm.providers import MockProvider, LLMProvider
from jeeves_avionics.llm.factory import create_llm_provider, create_agent_provider
from jeeves_avionics.capability_registry import get_capability_registry
from jeeves_avionics.settings import Settings

# Check Azure availability locally (no dependency on mission_system tests.config)
AZURE_AVAILABLE = bool(os.environ.get("AZURE_OPENAI_API_KEY"))

# Try to import Azure provider (uses OpenAI SDK)
try:
    from jeeves_avionics.llm.providers import AzureAIFoundryProvider, OPENAI_AVAILABLE
except ImportError:
    OPENAI_AVAILABLE = False


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
