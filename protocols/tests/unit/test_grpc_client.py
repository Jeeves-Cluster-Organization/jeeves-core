"""Unit tests for gRPC client.

Constitutional Compliance (HANDOFF.md, CONTRACTS.md):
- Go gRPC server is REQUIRED. No fallbacks.
- All tests use mocked gRPC stubs to simulate Go server responses.
- No tests assume local fallback behavior.
"""

import json
import pytest
from datetime import datetime, timezone
from unittest.mock import MagicMock, patch, PropertyMock

from protocols.grpc_client import (
    GrpcGoClient,
    BoundsResult,
    ExecutionEvent,
    AgentResult,
    GrpcCallError,
    GrpcConnectionError,
    GoServerNotRunningError,
    _terminal_reason_to_proto,
    _proto_to_terminal_reason,
)
from jeeves_core.types import Envelope, TerminalReason
from protocols import RequestContext


# =============================================================================
# Fixtures
# =============================================================================


@pytest.fixture
def mock_grpc():
    """Mock gRPC channel and stub for testing."""
    with patch("protocols.grpc_client.grpc") as mock_grpc_module:
        # Mock channel
        mock_channel = MagicMock()
        mock_grpc_module.insecure_channel.return_value = mock_channel
        
        # Mock channel_ready_future to simulate successful connection
        mock_future = MagicMock()
        mock_future.result.return_value = True
        mock_grpc_module.channel_ready_future.return_value = mock_future
        
        # RpcError for testing errors
        mock_grpc_module.RpcError = Exception
        
        yield mock_grpc_module


@pytest.fixture
def mock_stub():
    """Mock gRPC stub with proto messages."""
    with patch("protocols.grpc_stub.EngineServiceStub") as mock_stub_class:
        mock_stub_instance = MagicMock()
        mock_stub_class.return_value = mock_stub_instance
        yield mock_stub_instance


@pytest.fixture
def mock_proto_envelope():
    """Create a mock proto envelope response."""
    proto = MagicMock()
    proto.envelope_id = "env_test"
    proto.request_id = "req_test"
    proto.user_id = "user_123"
    proto.session_id = "session_456"
    proto.raw_input = "Hello, world!"
    proto.current_stage = "analysis"
    proto.stage_order = ["start", "analysis", "end"]
    proto.iteration = 0
    proto.max_iterations = 3
    proto.llm_call_count = 0
    proto.max_llm_calls = 10
    proto.agent_hop_count = 0
    proto.max_agent_hops = 21
    proto.terminated = False
    proto.termination_reason = ""
    proto.terminal_reason = 0  # UNSPECIFIED
    proto.interrupt_pending = False
    proto.current_stage_number = 0
    proto.max_stages = 5
    proto.all_goals = []
    proto.remaining_goals = []
    proto.received_at_ms = 0
    proto.created_at_ms = 0
    proto.completed_at_ms = 0
    proto.outputs = {}
    proto.active_stages = {}
    proto.completed_stage_set = {}
    proto.failed_stages = {}
    proto.goal_completion_status = {}
    proto.metadata_str = {
        "request_context": json.dumps({
            "request_id": "req_test",
            "capability": "test_capability",
            "user_id": "user_123",
            "session_id": "session_456",
            "agent_role": None,
            "trace_id": None,
            "span_id": None,
            "tags": {},
        })
    }
    proto.HasField = MagicMock(return_value=False)
    proto.interrupt = MagicMock()
    proto.interrupt.interrupt_id = ""
    return proto


@pytest.fixture
def sample_envelope():
    """Create a sample Envelope for testing."""
    request_context = RequestContext(
        request_id="req_test",
        capability="test_capability",
        user_id="user_123",
        session_id="session_456",
    )
    return Envelope(
        request_context=request_context,
        envelope_id="env_test",
        request_id="req_test",
        user_id="user_123",
        session_id="session_456",
        raw_input="Hello, world!",
        current_stage="analysis",
        stage_order=["start", "analysis", "end"],
        iteration=0,
        max_iterations=3,
        llm_call_count=0,
        max_llm_calls=10,
        agent_hop_count=0,
        max_agent_hops=21,
    )


# =============================================================================
# Connection Tests
# =============================================================================


class TestGrpcGoClientConnection:
    """Tests for connection handling."""

    def test_connect_success(self, mock_grpc):
        """Test successful connection to Go server."""
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch.dict("sys.modules", {"protocols.grpc_stub": MagicMock()}):
                with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                    client = GrpcGoClient(address="localhost:50051")
                    client.connect()
                    
                    mock_grpc.insecure_channel.assert_called_once_with("localhost:50051")
                    mock_grpc.channel_ready_future.assert_called_once()

    def test_connect_timeout_raises_error(self, mock_grpc):
        """Test that connection timeout raises GoServerNotRunningError."""
        import grpc
        mock_grpc.FutureTimeoutError = grpc.FutureTimeoutError
        mock_future = MagicMock()
        mock_future.result.side_effect = grpc.FutureTimeoutError()
        mock_grpc.channel_ready_future.return_value = mock_future
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            client = GrpcGoClient()
            
            with pytest.raises(GoServerNotRunningError):
                client.connect()

    def test_close_cleans_up_resources(self, mock_grpc):
        """Test that close properly cleans up."""
        mock_channel = MagicMock()
        mock_grpc.insecure_channel.return_value = mock_channel
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch.dict("sys.modules", {"protocols.grpc_stub": MagicMock()}):
                with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                    client = GrpcGoClient()
                    client.connect()
                    client.close()
                    
                    mock_channel.close.assert_called_once()
                    assert client._channel is None
                    assert client._stub is None

    def test_context_manager(self, mock_grpc):
        """Test context manager behavior."""
        mock_channel = MagicMock()
        mock_grpc.insecure_channel.return_value = mock_channel
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch.dict("sys.modules", {"protocols.grpc_stub": MagicMock()}):
                with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                    with GrpcGoClient() as client:
                        assert client._channel is not None
                    
                    mock_channel.close.assert_called_once()


# =============================================================================
# CreateEnvelope Tests
# =============================================================================


class TestGrpcGoClientCreateEnvelope:
    """Tests for create_envelope RPC."""

    def test_create_envelope_success(self, mock_grpc, mock_proto_envelope):
        """Test successful envelope creation."""
        mock_stub_instance = MagicMock()
        mock_stub_instance.CreateEnvelope.return_value = mock_proto_envelope
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.CreateEnvelopeRequest") as mock_request:
                    with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                        client = GrpcGoClient()
                        client.connect()
                        
                        envelope = client.create_envelope(
                            raw_input="Hello, world!",
                            request_context=RequestContext(
                                request_id="req_test",
                                capability="test_capability",
                                user_id="user_123",
                                session_id="session_456",
                            ),
                        )
                        
                        assert envelope.raw_input == "Hello, world!"
                        assert envelope.user_id == "user_123"
                        mock_stub_instance.CreateEnvelope.assert_called_once()

    def test_create_envelope_rpc_error(self, mock_grpc):
        """Test that RPC errors are converted to GrpcCallError."""
        import grpc as real_grpc
        mock_grpc.RpcError = real_grpc.RpcError
        
        mock_stub_instance = MagicMock()
        rpc_error = real_grpc.RpcError()
        rpc_error.details = MagicMock(return_value="Server error")
        mock_stub_instance.CreateEnvelope.side_effect = rpc_error
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.CreateEnvelopeRequest") as mock_request:
                    with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                        client = GrpcGoClient()
                        client.connect()
                        
                        with pytest.raises(GrpcCallError):
                            client.create_envelope(
                                raw_input="test",
                                request_context=RequestContext(
                                    request_id="req_test",
                                    capability="test_capability",
                                    user_id="user",
                                    session_id="session",
                                ),
                            )


# =============================================================================
# UpdateEnvelope Tests
# =============================================================================


class TestGrpcGoClientUpdateEnvelope:
    """Tests for update_envelope RPC."""

    def test_update_envelope_success(self, mock_grpc, mock_proto_envelope, sample_envelope):
        """Test successful envelope update."""
        # Set expected values on proto response
        mock_proto_envelope.llm_call_count = 2
        mock_proto_envelope.agent_hop_count = 1
        mock_proto_envelope.outputs = {"test_agent": json.dumps({"result": "success"}).encode()}
        
        mock_stub_instance = MagicMock()
        mock_stub_instance.UpdateEnvelope.return_value = mock_proto_envelope
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.UpdateEnvelopeRequest") as mock_request:
                    with patch("protocols.grpc_stub.Envelope") as mock_envelope:
                        with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                            client = GrpcGoClient()
                            client.connect()
                            
                            updated = client.update_envelope(
                                envelope=sample_envelope,
                                agent_name="test_agent",
                                output={"result": "success"},
                                llm_calls_made=2,
                            )
                            
                            mock_stub_instance.UpdateEnvelope.assert_called_once()
                            # Verify local state was updated before RPC
                            assert sample_envelope.llm_call_count == 2
                            assert sample_envelope.agent_hop_count == 1

    def test_update_envelope_without_agent_name(self, mock_grpc, mock_proto_envelope, sample_envelope):
        """Test update without agent name (no hop increment)."""
        mock_proto_envelope.llm_call_count = 1
        mock_proto_envelope.agent_hop_count = 0
        
        mock_stub_instance = MagicMock()
        mock_stub_instance.UpdateEnvelope.return_value = mock_proto_envelope
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.UpdateEnvelopeRequest") as mock_request:
                    with patch("protocols.grpc_stub.Envelope") as mock_envelope:
                        with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                            client = GrpcGoClient()
                            client.connect()
                            
                            updated = client.update_envelope(
                                envelope=sample_envelope,
                                llm_calls_made=1,
                            )
                            
                            # No agent_name = no hop increment
                            assert sample_envelope.agent_hop_count == 0
                            assert sample_envelope.llm_call_count == 1


# =============================================================================
# CheckBounds Tests
# =============================================================================


class TestGrpcGoClientCheckBounds:
    """Tests for check_bounds RPC - Go is authoritative (Contract 12)."""

    def test_check_bounds_can_continue(self, mock_grpc, sample_envelope):
        """Test bounds check when within limits."""
        mock_response = MagicMock()
        mock_response.can_continue = True
        mock_response.terminal_reason = 0  # UNSPECIFIED
        mock_response.llm_calls_remaining = 10
        mock_response.agent_hops_remaining = 21
        mock_response.iterations_remaining = 3
        
        mock_stub_instance = MagicMock()
        mock_stub_instance.CheckBounds.return_value = mock_response
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.Envelope") as mock_envelope:
                    with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                        client = GrpcGoClient()
                        client.connect()
                        
                        result = client.check_bounds(sample_envelope)
                        
                        assert result.can_continue is True
                        assert result.terminal_reason is None
                        assert result.llm_calls_remaining == 10
                        assert result.agent_hops_remaining == 21
                        assert result.iterations_remaining == 3

    def test_check_bounds_exceeded(self, mock_grpc, sample_envelope):
        """Test bounds check when limit exceeded."""
        mock_response = MagicMock()
        mock_response.can_continue = False
        mock_response.terminal_reason = 2  # max_llm_calls_exceeded
        mock_response.llm_calls_remaining = 0
        mock_response.agent_hops_remaining = 21
        mock_response.iterations_remaining = 3
        
        mock_stub_instance = MagicMock()
        mock_stub_instance.CheckBounds.return_value = mock_response
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.Envelope") as mock_envelope:
                    with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                        client = GrpcGoClient()
                        client.connect()
                        
                        result = client.check_bounds(sample_envelope)
                        
                        assert result.can_continue is False
                        assert result.terminal_reason == "max_llm_calls_exceeded"
                        assert result.llm_calls_remaining == 0


# =============================================================================
# CloneEnvelope Tests
# =============================================================================


class TestGrpcGoClientCloneEnvelope:
    """Tests for clone_envelope RPC."""

    def test_clone_envelope_success(self, mock_grpc, mock_proto_envelope, sample_envelope):
        """Test successful envelope cloning."""
        mock_stub_instance = MagicMock()
        mock_stub_instance.CloneEnvelope.return_value = mock_proto_envelope
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.CloneRequest") as mock_request:
                    with patch("protocols.grpc_stub.Envelope") as mock_envelope:
                        with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                            client = GrpcGoClient()
                            client.connect()
                            
                            clone = client.clone_envelope(sample_envelope)
                            
                            mock_stub_instance.CloneEnvelope.assert_called_once()
                            assert clone.envelope_id == mock_proto_envelope.envelope_id


# =============================================================================
# ExecuteAgent Tests
# =============================================================================


class TestGrpcGoClientExecuteAgent:
    """Tests for execute_agent RPC."""

    def test_execute_agent_success(self, mock_grpc, mock_proto_envelope, sample_envelope):
        """Test successful agent execution."""
        mock_response = MagicMock()
        mock_response.success = True
        mock_response.output = json.dumps({"result": "done"}).encode()
        mock_response.error = ""
        mock_response.duration_ms = 150
        mock_response.llm_calls = 2
        mock_response.envelope = mock_proto_envelope
        
        mock_stub_instance = MagicMock()
        mock_stub_instance.ExecuteAgent.return_value = mock_response
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.ExecuteAgentRequest") as mock_request:
                    with patch("protocols.grpc_stub.Envelope") as mock_envelope:
                        with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                            client = GrpcGoClient()
                            client.connect()
                            
                            result = client.execute_agent(
                                envelope=sample_envelope,
                                agent_name="test_agent",
                            )
                            
                            assert result.success is True
                            assert result.output == {"result": "done"}
                            assert result.duration_ms == 150
                            assert result.llm_calls == 2

    def test_execute_agent_failure(self, mock_grpc, mock_proto_envelope, sample_envelope):
        """Test agent execution failure."""
        mock_response = MagicMock()
        mock_response.success = False
        mock_response.output = b""
        mock_response.error = "Agent failed"
        mock_response.duration_ms = 50
        mock_response.llm_calls = 1
        mock_response.envelope = mock_proto_envelope
        
        mock_stub_instance = MagicMock()
        mock_stub_instance.ExecuteAgent.return_value = mock_response
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.ExecuteAgentRequest") as mock_request:
                    with patch("protocols.grpc_stub.Envelope") as mock_envelope:
                        with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                            client = GrpcGoClient()
                            client.connect()
                            
                            result = client.execute_agent(
                                envelope=sample_envelope,
                                agent_name="test_agent",
                            )
                            
                            assert result.success is False
                            assert result.error == "Agent failed"


# =============================================================================
# ExecutePipeline Tests
# =============================================================================


class TestGrpcGoClientExecutePipeline:
    """Tests for execute_pipeline streaming RPC."""

    def test_execute_pipeline_streams_events(self, mock_grpc, mock_proto_envelope, sample_envelope):
        """Test pipeline execution yields events."""
        mock_event1 = MagicMock()
        mock_event1.type = 1  # STAGE_STARTED
        mock_event1.stage = "analysis"
        mock_event1.timestamp_ms = 1000
        mock_event1.payload = json.dumps({"step": 1}).encode()
        mock_event1.envelope = mock_proto_envelope
        
        mock_event2 = MagicMock()
        mock_event2.type = 4  # PIPELINE_COMPLETED
        mock_event2.stage = "end"
        mock_event2.timestamp_ms = 2000
        mock_event2.payload = b""
        mock_event2.envelope = mock_proto_envelope
        
        mock_stub_instance = MagicMock()
        mock_stub_instance.ExecutePipeline.return_value = iter([mock_event1, mock_event2])
        
        with patch("protocols.grpc_client.grpc", mock_grpc):
            with patch("protocols.grpc_stub.EngineServiceStub", return_value=mock_stub_instance):
                with patch("protocols.grpc_stub.ExecuteRequest") as mock_request:
                    with patch("protocols.grpc_stub.Envelope") as mock_envelope:
                        with patch.dict("sys.modules", {"coreengine.proto.jeeves_core_pb2": MagicMock()}):
                            client = GrpcGoClient()
                            client.connect()
                            
                            events = list(client.execute_pipeline(
                                envelope=sample_envelope,
                                thread_id="thread_123",
                            ))
                            
                            assert len(events) == 2
                            assert events[0].type == "STAGE_STARTED"
                            assert events[0].stage == "analysis"
                            assert events[1].type == "PIPELINE_COMPLETED"


# =============================================================================
# Terminal Reason Conversion Tests
# =============================================================================


class TestTerminalReasonConversion:
    """Tests for terminal reason enum conversions."""

    def test_terminal_reason_to_proto_all_values(self):
        """Test conversion of all terminal reason values to proto."""
        test_cases = [
            (None, 0),
            (TerminalReason.MAX_ITERATIONS_EXCEEDED, 1),
            (TerminalReason.MAX_LLM_CALLS_EXCEEDED, 2),
            (TerminalReason.MAX_AGENT_HOPS_EXCEEDED, 3),
            (TerminalReason.USER_CANCELLED, 4),
            (TerminalReason.TOOL_FAILED_FATALLY, 5),
            (TerminalReason.POLICY_VIOLATION, 6),
            (TerminalReason.COMPLETED, 7),
        ]
        
        for python_value, expected_proto in test_cases:
            result = _terminal_reason_to_proto(python_value, None)
            assert result == expected_proto, f"Failed for {python_value}"

    def test_proto_to_terminal_reason_all_values(self):
        """Test conversion of all proto values to terminal reason."""
        test_cases = [
            (0, None),
            (1, TerminalReason.MAX_ITERATIONS_EXCEEDED),
            (2, TerminalReason.MAX_LLM_CALLS_EXCEEDED),
            (3, TerminalReason.MAX_AGENT_HOPS_EXCEEDED),
            (4, TerminalReason.USER_CANCELLED),
            (5, TerminalReason.TOOL_FAILED_FATALLY),
            (6, TerminalReason.POLICY_VIOLATION),
            (7, TerminalReason.COMPLETED),
        ]
        
        for proto_value, expected_python in test_cases:
            result = _proto_to_terminal_reason(proto_value)
            assert result == expected_python, f"Failed for proto value {proto_value}"

    def test_roundtrip_conversion(self):
        """Test roundtrip conversion preserves values."""
        for reason in TerminalReason:
            proto_val = _terminal_reason_to_proto(reason, None)
            back = _proto_to_terminal_reason(proto_val)
            assert back == reason, f"Roundtrip failed for {reason}"


# =============================================================================
# Result Dataclass Tests
# =============================================================================


class TestBoundsResult:
    """Tests for BoundsResult dataclass."""

    def test_bounds_result_creation(self):
        """Test BoundsResult creation."""
        result = BoundsResult(
            can_continue=True,
            terminal_reason=None,
            llm_calls_remaining=5,
            agent_hops_remaining=10,
            iterations_remaining=2,
        )
        
        assert result.can_continue is True
        assert result.terminal_reason is None
        assert result.llm_calls_remaining == 5
        assert result.agent_hops_remaining == 10
        assert result.iterations_remaining == 2

    def test_bounds_result_with_terminal_reason(self):
        """Test BoundsResult with terminal reason."""
        result = BoundsResult(
            can_continue=False,
            terminal_reason="max_llm_calls_exceeded",
            llm_calls_remaining=0,
            agent_hops_remaining=15,
            iterations_remaining=1,
        )
        
        assert result.can_continue is False
        assert result.terminal_reason == "max_llm_calls_exceeded"


class TestAgentResult:
    """Tests for AgentResult dataclass."""

    def test_agent_result_success(self):
        """Test AgentResult for successful execution."""
        result = AgentResult(
            success=True,
            output={"result": "done"},
            error=None,
            duration_ms=150,
            llm_calls=2,
            envelope=None,
        )
        
        assert result.success is True
        assert result.output == {"result": "done"}
        assert result.error is None
        assert result.duration_ms == 150
        assert result.llm_calls == 2

    def test_agent_result_failure(self):
        """Test AgentResult for failed execution."""
        result = AgentResult(
            success=False,
            output=None,
            error="Something went wrong",
            duration_ms=50,
            llm_calls=1,
            envelope=None,
        )
        
        assert result.success is False
        assert result.output is None
        assert result.error == "Something went wrong"


class TestExecutionEvent:
    """Tests for ExecutionEvent dataclass."""

    def test_execution_event_creation(self):
        """Test ExecutionEvent creation."""
        request_context = RequestContext(
            request_id="req_test",
            capability="test_capability",
            user_id="user",
            session_id="session",
        )
        envelope = Envelope(
            request_context=request_context,
            envelope_id="env_test",
            request_id="req_test",
            user_id="user",
            session_id="session",
            raw_input="test",
        )
        
        event = ExecutionEvent(
            type="STAGE_COMPLETED",
            stage="analysis",
            timestamp_ms=1704067200000,
            payload={"duration_ms": 150},
            envelope=envelope,
        )
        
        assert event.type == "STAGE_COMPLETED"
        assert event.stage == "analysis"
        assert event.timestamp_ms == 1704067200000
        assert event.payload == {"duration_ms": 150}
        assert event.envelope is envelope

    def test_execution_event_without_payload(self):
        """Test ExecutionEvent without payload."""
        event = ExecutionEvent(
            type="PIPELINE_COMPLETED",
            stage="final",
            timestamp_ms=1704067200000,
            payload=None,
            envelope=None,
        )
        
        assert event.type == "PIPELINE_COMPLETED"
        assert event.payload is None
        assert event.envelope is None


# =============================================================================
# Module-level Function Tests  
# =============================================================================


class TestModuleFunctions:
    """Tests for module-level convenience functions."""

    def test_get_client_returns_singleton(self):
        """Test that get_client returns the same instance."""
        import protocols.grpc_client as module
        
        # Reset singleton
        module._default_client = None
        
        client1 = module.get_client()
        client2 = module.get_client()
        
        assert client1 is client2
        
        # Cleanup
        module._default_client = None
