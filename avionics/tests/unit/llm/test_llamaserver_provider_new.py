"""Unit tests for LlamaServerProvider (uses LiteLLM)."""

import pytest
from unittest.mock import Mock, AsyncMock, patch, MagicMock
from avionics.llm.providers.llamaserver_provider import LlamaServerProvider
from avionics.llm.providers.base import TokenChunk


class TestLlamaServerProvider:
    """Test LlamaServerProvider that uses LiteLLM."""

    @pytest.mark.asyncio
    async def test_provider_initialization(self):
        """Test provider initializes with correct configuration."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(
                base_url="http://localhost:8080",
                timeout=60.0,
                max_retries=5,
                api_type="native"
            )

            assert provider._base_url == "http://localhost:8080"
            assert provider._timeout == 60.0
            assert provider._max_retries == 5
            assert provider._api_type == "native"
            assert provider._cost_calculator is not None

    @pytest.mark.asyncio
    async def test_generate_calls_litellm(self):
        """Test generate() calls LiteLLM acompletion."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            with patch('avionics.llm.providers.llamaserver_provider.acompletion') as mock_acompletion:
                # Mock LiteLLM response
                mock_response = MagicMock()
                mock_response.choices = [MagicMock()]
                mock_response.choices[0].message.content = "Generated text"
                mock_response.usage = MagicMock()
                mock_response.usage.prompt_tokens = 10
                mock_response.usage.completion_tokens = 20
                mock_acompletion.return_value = mock_response

                provider = LlamaServerProvider(base_url="http://localhost:8080")
                result = await provider.generate(
                    model="test-model",
                    prompt="Test prompt",
                    options={"temperature": 0.7, "num_predict": 100}
                )

                assert result == "Generated text"
                mock_acompletion.assert_called_once()
                call_kwargs = mock_acompletion.call_args[1]
                assert call_kwargs["model"] == "openai/local-model"
                assert call_kwargs["api_base"] == "http://localhost:8080/v1"
                assert call_kwargs["stream"] is False

    @pytest.mark.asyncio
    async def test_generate_tracks_cost(self):
        """Test generate() tracks token cost."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger') as mock_logger_fn:
            mock_logger = Mock()
            mock_logger_fn.return_value = mock_logger

            with patch('avionics.llm.providers.llamaserver_provider.acompletion') as mock_acompletion:
                mock_response = MagicMock()
                mock_response.choices = [MagicMock()]
                mock_response.choices[0].message.content = "Response"
                mock_response.usage = MagicMock()
                mock_response.usage.prompt_tokens = 100
                mock_response.usage.completion_tokens = 50
                mock_acompletion.return_value = mock_response

                provider = LlamaServerProvider(base_url="http://localhost:8080")
                await provider.generate("model", "prompt")

                # Verify cost calculation logged
                log_calls = [call for call in mock_logger.info.call_args_list
                            if call[0][0] == "llm_generation_complete"]
                assert len(log_calls) == 1
                assert "cost_usd" in log_calls[0][1]

    @pytest.mark.asyncio
    async def test_generate_handles_error(self):
        """Test generate() handles LiteLLM errors."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            with patch('avionics.llm.providers.llamaserver_provider.acompletion') as mock_acompletion:
                mock_acompletion.side_effect = Exception("Connection timeout")

                provider = LlamaServerProvider(base_url="http://localhost:8080")

                with pytest.raises(Exception, match="LLM Error"):
                    await provider.generate("model", "prompt")

    @pytest.mark.asyncio
    async def test_generate_stream_calls_litellm(self):
        """Test generate_stream() calls LiteLLM with streaming."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            with patch('avionics.llm.providers.llamaserver_provider.acompletion') as mock_acompletion:
                # Mock streaming response
                async def mock_stream():
                    chunk1 = MagicMock()
                    chunk1.choices = [MagicMock()]
                    chunk1.choices[0].delta = MagicMock()
                    chunk1.choices[0].delta.content = "Hello"
                    chunk1.choices[0].finish_reason = None
                    yield chunk1

                    chunk2 = MagicMock()
                    chunk2.choices = [MagicMock()]
                    chunk2.choices[0].delta = MagicMock()
                    chunk2.choices[0].delta.content = " world"
                    chunk2.choices[0].finish_reason = None
                    yield chunk2

                    chunk3 = MagicMock()
                    chunk3.choices = [MagicMock()]
                    chunk3.choices[0].delta = MagicMock()
                    chunk3.choices[0].delta.content = None
                    chunk3.choices[0].finish_reason = "stop"
                    yield chunk3

                mock_acompletion.return_value = mock_stream()

                provider = LlamaServerProvider(base_url="http://localhost:8080")

                chunks = []
                async for chunk in provider.generate_stream("model", "prompt"):
                    chunks.append(chunk)

                assert len(chunks) == 3
                assert isinstance(chunks[0], TokenChunk)
                assert chunks[0].text == "Hello"
                assert chunks[0].is_final is False
                assert chunks[1].text == " world"
                assert chunks[2].is_final is True

    @pytest.mark.asyncio
    async def test_generate_stream_handles_error(self):
        """Test generate_stream() handles LiteLLM errors."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            with patch('avionics.llm.providers.llamaserver_provider.acompletion') as mock_acompletion:
                mock_acompletion.side_effect = Exception("Stream error")

                provider = LlamaServerProvider(base_url="http://localhost:8080")

                with pytest.raises(Exception, match="LLM Stream Error"):
                    async for _ in provider.generate_stream("model", "prompt"):
                        pass

    @pytest.mark.asyncio
    async def test_health_check_success(self):
        """Test health_check() returns True on success."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            with patch('avionics.llm.providers.llamaserver_provider.acompletion') as mock_acompletion:
                mock_response = MagicMock()
                mock_response.choices = [MagicMock()]
                mock_acompletion.return_value = mock_response

                provider = LlamaServerProvider(base_url="http://localhost:8080")
                is_healthy = await provider.health_check()

                assert is_healthy is True

    @pytest.mark.asyncio
    async def test_health_check_failure(self):
        """Test health_check() returns False on error."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            with patch('avionics.llm.providers.llamaserver_provider.acompletion') as mock_acompletion:
                mock_acompletion.side_effect = Exception("Connection refused")

                provider = LlamaServerProvider(base_url="http://localhost:8080")
                is_healthy = await provider.health_check()

                assert is_healthy is False

    def test_supports_streaming(self):
        """Test provider reports streaming support."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")
            assert provider.supports_streaming is True

    def test_get_stats(self):
        """Test get_stats() returns provider configuration."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(
                base_url="http://node1:8080",
                timeout=90.0,
                max_retries=4,
                api_type="openai"
            )

            stats = provider.get_stats()
            assert stats["base_url"] == "http://node1:8080"
            assert stats["timeout"] == 90.0
            assert stats["max_retries"] == 4
            assert stats["api_type"] == "openai"
            assert "LiteLLM" in stats["backend"]

    def test_repr(self):
        """Test string representation."""
        with patch('avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(
                base_url="http://localhost:8080",
                api_type="native"
            )
            assert "http://localhost:8080" in repr(provider)
            assert "native" in repr(provider)
