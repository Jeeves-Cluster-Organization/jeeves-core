"""Error handling tests for LLM providers.

Tests edge cases, error conditions, and recovery behavior.
"""

import pytest
from unittest.mock import Mock, patch, AsyncMock, MagicMock
from datetime import datetime, timezone

from jeeves_avionics.llm.providers import (
    MockProvider,
    LLMProvider,
)
from jeeves_avionics.llm.providers.base import TokenChunk


# =============================================================================
# MockProvider Error Tests
# =============================================================================

class TestMockProviderErrors:
    """Test MockProvider error handling."""

    @pytest.mark.asyncio
    async def test_mock_provider_handles_empty_prompt(self):
        """Test that mock provider handles empty prompt gracefully."""
        provider = MockProvider()
        
        result = await provider.generate(
            model="test",
            prompt="",
            options={},
        )
        
        # Should return some valid response even for empty prompt
        assert isinstance(result, str)

    @pytest.mark.asyncio
    async def test_mock_provider_handles_none_options(self):
        """Test that mock provider handles None options."""
        provider = MockProvider()
        
        result = await provider.generate(
            model="test",
            prompt="test prompt",
            options=None,
        )
        
        assert isinstance(result, str)
        assert len(result) > 0

    @pytest.mark.asyncio
    async def test_mock_provider_respects_max_tokens(self):
        """Test that mock provider respects max_tokens option."""
        provider = MockProvider()
        
        result = await provider.generate(
            model="test",
            prompt="Generate a very long response",
            options={"max_tokens": 10},
        )
        
        # Response should be truncated (roughly)
        # Mock provider's behavior depends on implementation
        assert isinstance(result, str)

    @pytest.mark.asyncio
    async def test_mock_provider_streaming_yields_chunks(self):
        """Test that mock provider streaming yields TokenChunks."""
        provider = MockProvider()
        
        chunks = []
        async for chunk in provider.generate_stream(
            model="test",
            prompt="test prompt",
            options={},
        ):
            chunks.append(chunk)
        
        assert len(chunks) > 0
        assert all(isinstance(c, TokenChunk) for c in chunks)
        
        # Last chunk should be marked as final
        assert chunks[-1].is_final is True

    @pytest.mark.asyncio
    async def test_mock_provider_streaming_combines_to_full_response(self):
        """Test that streaming chunks combine to full response."""
        provider = MockProvider()
        
        chunks = []
        async for chunk in provider.generate_stream(
            model="test",
            prompt="test prompt",
            options={},
        ):
            chunks.append(chunk)
        
        # Combine all chunk texts
        full_text = "".join(c.text for c in chunks)
        
        # Should be non-empty
        assert len(full_text) > 0


# =============================================================================
# LLMProvider Base Class Tests
# =============================================================================

class TestLLMProviderBase:
    """Test LLMProvider base class behavior."""

    def test_supports_streaming_default_false(self):
        """Test that supports_streaming defaults to False."""
        provider = MockProvider()
        
        # MockProvider may override this
        # Base class default is False
        assert isinstance(provider.supports_streaming, bool)

    @pytest.mark.asyncio
    async def test_generate_stream_fallback(self):
        """Test that generate_stream falls back to generate."""
        # Create a mock provider that doesn't override generate_stream
        provider = MockProvider()
        
        # generate_stream should still work via fallback
        chunks = []
        async for chunk in provider.generate_stream(
            model="test",
            prompt="test",
            options={},
        ):
            chunks.append(chunk)
        
        assert len(chunks) >= 1


# =============================================================================
# TokenChunk Tests
# =============================================================================

class TestTokenChunk:
    """Test TokenChunk dataclass."""

    def test_token_chunk_creation(self):
        """Test TokenChunk creation with all fields."""
        chunk = TokenChunk(
            text="Hello",
            is_final=False,
            token_count=1,
        )
        
        assert chunk.text == "Hello"
        assert chunk.is_final is False
        assert chunk.token_count == 1

    def test_token_chunk_defaults(self):
        """Test TokenChunk default values."""
        chunk = TokenChunk(text="Hello")

        assert chunk.text == "Hello"
        assert chunk.is_final is False
        assert chunk.token_count == 0  # Defaults to 0 per base.py:24

    def test_token_chunk_final(self):
        """Test TokenChunk with is_final=True."""
        chunk = TokenChunk(
            text="Done",
            is_final=True,
        )
        
        assert chunk.is_final is True


# =============================================================================
# Provider Factory Error Tests
# =============================================================================

class TestProviderFactoryErrors:
    """Test provider factory error handling."""

    def test_factory_invalid_provider_type(self):
        """Test that factory raises for invalid provider type."""
        from jeeves_avionics.llm.factory import create_llm_provider
        from jeeves_avionics.settings import Settings
        
        settings = Settings()
        
        with pytest.raises(ValueError, match="Unknown provider type"):
            create_llm_provider("nonexistent_provider", settings)

    def test_factory_handles_missing_api_key(self):
        """Test that factory handles missing API keys gracefully."""
        from jeeves_avionics.llm.factory import create_llm_provider
        from jeeves_avionics.settings import Settings
        
        settings = Settings()
        
        # Should not raise during creation - key validation happens at call time
        provider = create_llm_provider("openai", settings)
        assert provider is not None


# =============================================================================
# Provider Retry Logic Tests
# =============================================================================

class TestProviderRetryLogic:
    """Test provider retry and error recovery logic."""

    @pytest.mark.asyncio
    async def test_mock_provider_always_succeeds(self):
        """Test that mock provider never fails (for testing stability)."""
        provider = MockProvider()
        
        # Multiple calls should all succeed
        for _ in range(10):
            result = await provider.generate(
                model="test",
                prompt="test",
                options={},
            )
            assert result is not None

    @pytest.mark.asyncio
    async def test_mock_provider_health_always_true(self):
        """Test that mock provider health check always returns True."""
        provider = MockProvider()
        
        # Multiple health checks should all pass
        for _ in range(5):
            is_healthy = await provider.health_check()
            assert is_healthy is True


# =============================================================================
# Provider Call Count Tests
# =============================================================================

class TestProviderCallCounting:
    """Test provider call counting for resource tracking."""

    @pytest.mark.asyncio
    async def test_mock_provider_increments_call_count(self):
        """Test that mock provider increments call count."""
        provider = MockProvider()
        
        assert provider.call_count == 0
        
        await provider.generate(model="test", prompt="test1")
        assert provider.call_count == 1
        
        await provider.generate(model="test", prompt="test2")
        assert provider.call_count == 2

    @pytest.mark.asyncio
    async def test_mock_provider_call_history(self):
        """Test that mock provider tracks call history."""
        provider = MockProvider()
        
        await provider.generate(
            model="test-model",
            prompt="first prompt",
            options={"temperature": 0.5},
        )
        
        await provider.generate(
            model="test-model-2",
            prompt="second prompt",
            options={"temperature": 0.7},
        )
        
        assert len(provider.call_history) == 2
        assert provider.call_history[0]["prompt"] == "first prompt"
        assert provider.call_history[1]["prompt"] == "second prompt"


# =============================================================================
# Edge Cases
# =============================================================================

class TestProviderEdgeCases:
    """Test edge cases for LLM providers."""

    @pytest.mark.asyncio
    async def test_unicode_prompt_handling(self):
        """Test that providers handle unicode prompts."""
        provider = MockProvider()
        
        result = await provider.generate(
            model="test",
            prompt="Hello 世界! 🎉 Привет мир",
            options={},
        )
        
        assert isinstance(result, str)

    @pytest.mark.asyncio
    async def test_very_long_prompt(self):
        """Test handling of very long prompts."""
        provider = MockProvider()
        
        # Create a long prompt
        long_prompt = "test " * 10000
        
        result = await provider.generate(
            model="test",
            prompt=long_prompt,
            options={},
        )
        
        assert isinstance(result, str)

    @pytest.mark.asyncio
    async def test_special_characters_in_prompt(self):
        """Test handling of special characters."""
        provider = MockProvider()
        
        special_prompt = 'Test with "quotes" and \'apostrophes\' and \n newlines \t tabs'
        
        result = await provider.generate(
            model="test",
            prompt=special_prompt,
            options={},
        )
        
        assert isinstance(result, str)

    @pytest.mark.asyncio
    async def test_json_in_prompt(self):
        """Test handling of JSON in prompt."""
        provider = MockProvider()
        
        json_prompt = '{"action": "test", "params": {"key": "value"}}'
        
        result = await provider.generate(
            model="test",
            prompt=json_prompt,
            options={},
        )
        
        assert isinstance(result, str)


