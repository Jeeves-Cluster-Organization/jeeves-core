"""Unit tests for refactored LlamaServerProvider (delegates to Airframe)."""

import pytest
from unittest.mock import Mock, AsyncMock, patch
from airframe.types import InferenceStreamEvent, StreamEventType, AirframeError, ErrorCategory
from avionics.llm.providers.llamaserver_provider import LlamaServerProvider
from avionics.llm.providers.base import TokenChunk


class TestRefactoredLlamaServerProvider:
    """Test LlamaServerProvider that delegates to Airframe adapter."""

    @pytest.mark.asyncio
    async def test_provider_initialization(self):
        """Test provider initializes with correct Airframe components."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(
                base_url="http://localhost:8080",
                timeout=60.0,
                max_retries=5,
                api_type="native"
            )

            # Verify Airframe adapter created
            assert provider._adapter is not None
            assert provider._adapter.timeout == 60.0
            assert provider._adapter.max_retries == 5

            # Verify endpoint spec
            assert provider._endpoint.name == "llamaserver"
            assert provider._endpoint.base_url == "http://localhost:8080"
            assert provider._endpoint.api_type == "native"

            # Verify Avionics features
            assert provider._cost_calculator is not None

    @pytest.mark.asyncio
    async def test_generate_delegates_to_airframe(self):
        """Test generate() delegates to Airframe adapter."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")

            # Mock Airframe adapter
            async def mock_stream_infer(endpoint, request):
                # Simulate Airframe response
                yield InferenceStreamEvent(
                    type=StreamEventType.MESSAGE,
                    content="Generated text from Airframe",
                    usage={"prompt_tokens": 10, "completion_tokens": 20},
                )
                yield InferenceStreamEvent(type=StreamEventType.DONE)

            provider._adapter.stream_infer = mock_stream_infer

            # Call generate
            result = await provider.generate(
                model="test-model",
                prompt="Test prompt",
                options={"temperature": 0.7, "num_predict": 100}
            )

            assert result == "Generated text from Airframe"

    @pytest.mark.asyncio
    async def test_generate_tracks_cost(self):
        """Test generate() tracks token cost."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger') as mock_logger_fn:
            mock_logger = Mock()
            mock_logger_fn.return_value = mock_logger

            provider = LlamaServerProvider(base_url="http://localhost:8080")

            # Mock Airframe adapter with token usage
            async def mock_stream_infer(endpoint, request):
                yield InferenceStreamEvent(
                    type=StreamEventType.MESSAGE,
                    content="Response",
                    usage={"prompt_tokens": 100, "completion_tokens": 50},
                )
                yield InferenceStreamEvent(type=StreamEventType.DONE)

            provider._adapter.stream_infer = mock_stream_infer

            result = await provider.generate("model", "prompt")

            # Verify cost calculation logged
            mock_logger.info.assert_called()
            log_call = [call for call in mock_logger.info.call_args_list
                       if call[0][0] == "llm_generation_complete"]
            assert len(log_call) == 1
            assert "cost_usd" in log_call[0][1]

    @pytest.mark.asyncio
    async def test_generate_handles_airframe_error(self):
        """Test generate() handles errors from Airframe adapter."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")

            # Mock Airframe adapter returning error
            async def mock_stream_infer(endpoint, request):
                yield InferenceStreamEvent(
                    type=StreamEventType.ERROR,
                    error=AirframeError(ErrorCategory.TIMEOUT, "Request timed out"),
                )

            provider._adapter.stream_infer = mock_stream_infer

            # Should raise exception
            with pytest.raises(Exception, match="LLM Error"):
                await provider.generate("model", "prompt")

    @pytest.mark.asyncio
    async def test_generate_stream_delegates_to_airframe(self):
        """Test generate_stream() delegates to Airframe and converts events."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")

            # Mock Airframe streaming response
            async def mock_stream_infer(endpoint, request):
                yield InferenceStreamEvent(type=StreamEventType.TOKEN, content="Hello")
                yield InferenceStreamEvent(type=StreamEventType.TOKEN, content=" world")
                yield InferenceStreamEvent(type=StreamEventType.DONE)

            provider._adapter.stream_infer = mock_stream_infer

            # Collect streamed chunks
            chunks = []
            async for chunk in provider.generate_stream("model", "prompt"):
                chunks.append(chunk)

            # Verify conversion to TokenChunk
            assert len(chunks) == 3
            assert isinstance(chunks[0], TokenChunk)
            assert chunks[0].text == "Hello"
            assert chunks[0].is_final is False
            assert chunks[1].text == " world"
            assert chunks[2].text == ""
            assert chunks[2].is_final is True

    @pytest.mark.asyncio
    async def test_generate_stream_handles_error(self):
        """Test generate_stream() handles Airframe errors."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")

            # Mock Airframe error during streaming
            async def mock_stream_infer(endpoint, request):
                yield InferenceStreamEvent(type=StreamEventType.TOKEN, content="Start")
                yield InferenceStreamEvent(
                    type=StreamEventType.ERROR,
                    error=AirframeError(ErrorCategory.CONNECTION, "Connection lost"),
                )

            provider._adapter.stream_infer = mock_stream_infer

            # Should raise exception after first token
            chunks = []
            with pytest.raises(Exception, match="LLM Stream Error"):
                async for chunk in provider.generate_stream("model", "prompt"):
                    chunks.append(chunk)

            # Should have received first token before error
            assert len(chunks) == 1
            assert chunks[0].text == "Start"

    @pytest.mark.asyncio
    async def test_health_check_success(self):
        """Test health_check() delegates to Airframe."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")

            # Mock healthy response from Airframe
            async def mock_stream_infer(endpoint, request):
                yield InferenceStreamEvent(type=StreamEventType.MESSAGE, content="Hi")

            provider._adapter.stream_infer = mock_stream_infer

            is_healthy = await provider.health_check()
            assert is_healthy is True

    @pytest.mark.asyncio
    async def test_health_check_failure(self):
        """Test health_check() detects Airframe errors."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")

            # Mock error from Airframe
            async def mock_stream_infer(endpoint, request):
                yield InferenceStreamEvent(
                    type=StreamEventType.ERROR,
                    error=AirframeError(ErrorCategory.CONNECTION, "Connection refused"),
                )

            provider._adapter.stream_infer = mock_stream_infer

            is_healthy = await provider.health_check()
            assert is_healthy is False

    @pytest.mark.asyncio
    async def test_health_check_exception(self):
        """Test health_check() handles exceptions gracefully."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")

            # Mock exception during health check
            async def mock_stream_infer(endpoint, request):
                raise Exception("Network error")

            provider._adapter.stream_infer = mock_stream_infer

            is_healthy = await provider.health_check()
            assert is_healthy is False

    def test_supports_streaming(self):
        """Test provider reports streaming support."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")
            assert provider.supports_streaming is True

    def test_get_stats(self):
        """Test get_stats() returns provider configuration."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
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
            assert "Airframe" in stats["backend"]

    @pytest.mark.asyncio
    async def test_request_conversion_to_airframe_format(self):
        """Test Avionics request correctly converts to Airframe InferenceRequest."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")

            captured_request = None

            async def mock_stream_infer(endpoint, request):
                nonlocal captured_request
                captured_request = request
                yield InferenceStreamEvent(type=StreamEventType.MESSAGE, content="Test")
                yield InferenceStreamEvent(type=StreamEventType.DONE)

            provider._adapter.stream_infer = mock_stream_infer

            # Call with Avionics API
            await provider.generate(
                model="test-model",
                prompt="Test prompt",
                options={"temperature": 0.8, "num_predict": 200}
            )

            # Verify Airframe request format
            assert captured_request is not None
            assert len(captured_request.messages) == 1
            assert captured_request.messages[0].role == "user"
            assert captured_request.messages[0].content == "Test prompt"
            assert captured_request.model == "test-model"
            assert captured_request.temperature == 0.8
            assert captured_request.max_tokens == 200
            assert captured_request.stream is False

    @pytest.mark.asyncio
    async def test_streaming_request_sets_stream_true(self):
        """Test generate_stream() sets stream=True in Airframe request."""
        with patch('jeeves_avionics.llm.providers.llamaserver_provider.get_current_logger'):
            provider = LlamaServerProvider(base_url="http://localhost:8080")

            captured_request = None

            async def mock_stream_infer(endpoint, request):
                nonlocal captured_request
                captured_request = request
                yield InferenceStreamEvent(type=StreamEventType.DONE)

            provider._adapter.stream_infer = mock_stream_infer

            async for _ in provider.generate_stream("model", "prompt"):
                pass

            # Verify streaming enabled
            assert captured_request.stream is True
