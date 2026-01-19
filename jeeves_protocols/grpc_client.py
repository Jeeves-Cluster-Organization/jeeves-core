"""gRPC client for Go runtime.

This is the ONLY Python -> Go IPC mechanism.
Go is authoritative for all envelope operations and bounds checking.

DESIGN PRINCIPLES:
- FAIL LOUD: No silent fallbacks. If Go is unreachable, raise immediately.
- SINGLE SOURCE OF TRUTH: Go runtime is authoritative. Python does not duplicate logic.
- EXPLICIT ERRORS: All failures surface with clear error messages.
"""

from dataclasses import dataclass
from typing import Any, Dict, Iterator, Optional

# gRPC import - REQUIRED, not optional
try:
    import grpc
except ImportError as e:
    raise ImportError(
        "grpcio is REQUIRED for jeeves_protocols. "
        "Install with: pip install grpcio grpcio-tools"
    ) from e

from jeeves_protocols.envelope import GenericEnvelope


class GrpcConnectionError(Exception):
    """Raised when connection to Go server fails."""
    pass


class GrpcCallError(Exception):
    """Raised when a gRPC call fails."""
    pass


class GoServerNotRunningError(Exception):
    """Raised when Go gRPC server is not running."""
    pass


@dataclass
class BoundsResult:
    """Result from bounds check - Go is authoritative."""
    can_continue: bool
    terminal_reason: Optional[str]
    llm_calls_remaining: int
    agent_hops_remaining: int
    iterations_remaining: int


@dataclass
class ExecutionEvent:
    """Event from pipeline execution stream."""
    type: str
    stage: str
    timestamp_ms: int
    payload: Optional[Dict[str, Any]]
    envelope: Optional[GenericEnvelope]


class GrpcGoClient:
    """gRPC client for Go runtime operations.

    All operations go through Go. No local fallbacks.
    If Go server is not available, methods raise immediately.
    """

    DEFAULT_ADDRESS = "localhost:50051"
    CONNECT_TIMEOUT = 5.0  # seconds

    def __init__(self, address: Optional[str] = None, timeout: float = 30.0):
        """Initialize gRPC client.

        Args:
            address: gRPC server address (default: localhost:50051)
            timeout: Default timeout for RPC calls in seconds

        Raises:
            GoServerNotRunningError: If Go server is not reachable
        """
        self._address = address or self.DEFAULT_ADDRESS
        self._timeout = timeout
        self._channel: Optional[grpc.Channel] = None
        self._stub: Any = None
        self._connected = False

    def connect(self) -> None:
        """Establish connection to Go server.

        Raises:
            GoServerNotRunningError: If connection fails
        """
        if self._connected:
            return

        self._channel = grpc.insecure_channel(self._address)

        try:
            # Wait for channel to be ready
            grpc.channel_ready_future(self._channel).result(
                timeout=self.CONNECT_TIMEOUT
            )
        except grpc.FutureTimeoutError:
            self._channel.close()
            self._channel = None
            raise GoServerNotRunningError(
                f"Go gRPC server not running at {self._address}. "
                f"Start with: go run cmd/envelope/main.go"
            )
        except Exception as e:
            if self._channel:
                self._channel.close()
                self._channel = None
            raise GrpcConnectionError(
                f"Failed to connect to Go server at {self._address}: {e}"
            ) from e

        # Import and create stub
        try:
            from jeeves_protocols import grpc_stub
            self._stub = grpc_stub.JeevesCoreServiceStub(self._channel)
        except ImportError as e:
            raise ImportError(
                "gRPC stubs not generated. Run: "
                "python -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. "
                "coreengine/proto/jeeves_core.proto"
            ) from e

        self._connected = True

    def _require_connection(self) -> None:
        """Ensure connected, raise if not.

        Raises:
            GoServerNotRunningError: If not connected and cannot connect
        """
        if not self._connected:
            self.connect()

    def create_envelope(
        self,
        raw_input: str,
        user_id: str,
        session_id: str,
        request_id: Optional[str] = None,
        metadata: Optional[Dict[str, str]] = None,
        stage_order: Optional[list] = None,
    ) -> GenericEnvelope:
        """Create envelope via Go runtime.

        Go is authoritative for envelope creation - ensures consistent
        ID generation and timestamp handling.

        Args:
            raw_input: User's raw input text
            user_id: User identifier
            session_id: Session identifier
            request_id: Optional request ID (Go generates if not provided)
            metadata: Optional metadata dict
            stage_order: Optional stage execution order

        Returns:
            GenericEnvelope created by Go

        Raises:
            GoServerNotRunningError: If Go server not available
            GrpcCallError: If the RPC fails
        """
        self._require_connection()

        try:
            from jeeves_protocols import grpc_stub
            request = grpc_stub.CreateEnvelopeRequest(
                raw_input=raw_input,
                user_id=user_id,
                session_id=session_id,
                request_id=request_id or "",
                metadata=metadata or {},
                stage_order=stage_order or [],
            )
            response = self._stub.CreateEnvelope(request, timeout=self._timeout)
            return self._envelope_from_proto(response.envelope)
        except grpc.RpcError as e:
            raise GrpcCallError(f"CreateEnvelope failed: {e.details()}") from e

    def check_bounds(self, envelope: GenericEnvelope) -> BoundsResult:
        """Check bounds - Go is authoritative.

        This MUST go through Go. Python does not duplicate bounds logic.

        Args:
            envelope: Envelope to check

        Returns:
            BoundsResult with can_continue and remaining counts

        Raises:
            GoServerNotRunningError: If Go server not available
            GrpcCallError: If the RPC fails
        """
        self._require_connection()

        try:
            from jeeves_protocols import grpc_stub
            request = grpc_stub.CheckBoundsRequest(
                envelope=self._envelope_to_proto(envelope)
            )
            response = self._stub.CheckBounds(request, timeout=self._timeout)
            return BoundsResult(
                can_continue=response.can_continue,
                terminal_reason=response.terminal_reason or None,
                llm_calls_remaining=response.llm_calls_remaining,
                agent_hops_remaining=response.agent_hops_remaining,
                iterations_remaining=response.iterations_remaining,
            )
        except grpc.RpcError as e:
            raise GrpcCallError(f"CheckBounds failed: {e.details()}") from e

    def clone_envelope(self, envelope: GenericEnvelope) -> GenericEnvelope:
        """Clone envelope via Go.

        Go handles deep copy to ensure consistency.

        Args:
            envelope: Envelope to clone

        Returns:
            Deep copy of envelope

        Raises:
            GoServerNotRunningError: If Go server not available
            GrpcCallError: If the RPC fails
        """
        self._require_connection()

        try:
            from jeeves_protocols import grpc_stub
            request = grpc_stub.CloneEnvelopeRequest(
                envelope=self._envelope_to_proto(envelope)
            )
            response = self._stub.CloneEnvelope(request, timeout=self._timeout)
            return self._envelope_from_proto(response.envelope)
        except grpc.RpcError as e:
            raise GrpcCallError(f"CloneEnvelope failed: {e.details()}") from e

    def execute_pipeline(
        self,
        envelope: GenericEnvelope,
        thread_id: str,
        pipeline_config: Optional[Dict[str, Any]] = None,
    ) -> Iterator[ExecutionEvent]:
        """Execute pipeline with streaming events.

        Go orchestrates the pipeline. Python receives events.

        Args:
            envelope: Starting envelope
            thread_id: Thread ID for persistence
            pipeline_config: Optional pipeline configuration

        Yields:
            ExecutionEvent for each stage transition

        Raises:
            GoServerNotRunningError: If Go server not available
            GrpcCallError: If the RPC fails
        """
        self._require_connection()

        try:
            from jeeves_protocols import grpc_stub
            import json

            request = grpc_stub.ExecuteRequest(
                envelope=self._envelope_to_proto(envelope),
                thread_id=thread_id,
                pipeline_config=json.dumps(pipeline_config or {}).encode(),
            )

            for event in self._stub.ExecutePipeline(request, timeout=self._timeout):
                yield ExecutionEvent(
                    type=event.type,
                    stage=event.stage,
                    timestamp_ms=event.timestamp_ms,
                    payload=json.loads(event.payload) if event.payload else None,
                    envelope=self._envelope_from_proto(event.envelope) if event.envelope else None,
                )
        except grpc.RpcError as e:
            raise GrpcCallError(f"ExecutePipeline failed: {e.details()}") from e

    def close(self) -> None:
        """Close the gRPC channel."""
        if self._channel:
            self._channel.close()
            self._channel = None
            self._stub = None
            self._connected = False

    def __enter__(self):
        self.connect()
        return self

    def __exit__(self, *args):
        self.close()

    # =========================================================================
    # Proto conversion helpers
    # =========================================================================

    def _envelope_to_proto(self, envelope: GenericEnvelope) -> Any:
        """Convert Python envelope to proto message."""
        from jeeves_protocols import grpc_stub
        import json

        return grpc_stub.Envelope(
            envelope_id=envelope.envelope_id,
            request_id=envelope.request_id,
            user_id=envelope.user_id,
            session_id=envelope.session_id,
            raw_input=envelope.raw_input,
            current_stage=envelope.current_stage,
            stage_order=envelope.stage_order,
            iteration=envelope.iteration,
            llm_call_count=envelope.llm_call_count,
            agent_hop_count=envelope.agent_hop_count,
            max_iterations=envelope.max_iterations,
            max_llm_calls=envelope.max_llm_calls,
            max_agent_hops=envelope.max_agent_hops,
            terminated=envelope.terminated,
            terminal_reason=envelope.terminal_reason or "",
            outputs=json.dumps(envelope.outputs or {}),
            metadata=envelope.metadata or {},
        )

    def _envelope_from_proto(self, proto: Any) -> GenericEnvelope:
        """Convert proto message to Python envelope."""
        import json
        from jeeves_shared.serialization import utc_now

        return GenericEnvelope(
            envelope_id=proto.envelope_id,
            request_id=proto.request_id,
            user_id=proto.user_id,
            session_id=proto.session_id,
            raw_input=proto.raw_input,
            current_stage=proto.current_stage,
            stage_order=list(proto.stage_order),
            iteration=proto.iteration,
            llm_call_count=proto.llm_call_count,
            agent_hop_count=proto.agent_hop_count,
            max_iterations=proto.max_iterations,
            max_llm_calls=proto.max_llm_calls,
            max_agent_hops=proto.max_agent_hops,
            terminated=proto.terminated,
            terminal_reason=proto.terminal_reason or None,
            outputs=json.loads(proto.outputs) if proto.outputs else {},
            metadata=dict(proto.metadata),
            received_at=utc_now(),
            created_at=utc_now(),
        )


# =============================================================================
# Module-level singleton - lazy connection
# =============================================================================

_default_client: Optional[GrpcGoClient] = None


def get_client() -> GrpcGoClient:
    """Get default gRPC client instance.

    Raises:
        GoServerNotRunningError: On first call if Go server not available
    """
    global _default_client
    if _default_client is None:
        _default_client = GrpcGoClient()
    return _default_client


def require_go_server() -> GrpcGoClient:
    """Get client and verify Go server is running.

    Use this at application startup to fail fast.

    Raises:
        GoServerNotRunningError: If Go server not available
    """
    client = get_client()
    client.connect()
    return client


def create_envelope(
    raw_input: str,
    user_id: str,
    session_id: str = "",
    request_id: str = "",
    metadata: Optional[Dict[str, str]] = None,
) -> GenericEnvelope:
    """Create envelope via Go gRPC.

    Raises:
        GoServerNotRunningError: If Go server not available
    """
    return get_client().create_envelope(
        raw_input=raw_input,
        user_id=user_id,
        session_id=session_id,
        request_id=request_id if request_id else None,
        metadata=metadata,
    )


def check_bounds(envelope: GenericEnvelope) -> BoundsResult:
    """Check bounds via Go gRPC.

    Raises:
        GoServerNotRunningError: If Go server not available
    """
    return get_client().check_bounds(envelope)


def clone_envelope(envelope: GenericEnvelope) -> GenericEnvelope:
    """Clone envelope via Go gRPC.

    Raises:
        GoServerNotRunningError: If Go server not available
    """
    return get_client().clone_envelope(envelope)
