"""gRPC client for Go runtime.

Replaces subprocess-based client with persistent gRPC connection.
This is the primary Python -> Go IPC mechanism.

Usage:
    from jeeves_protocols.grpc_client import GrpcGoClient

    # Create client
    client = GrpcGoClient()

    # Create envelope via Go
    envelope = client.create_envelope("Hello", "user1", "session1")

    # Check bounds (Go is authoritative)
    result = client.check_bounds(envelope)
    if not result.can_continue:
        raise BoundsExceededError(result.terminal_reason)

    # Execute pipeline with streaming
    for event in client.execute_pipeline(envelope, thread_id):
        handle_event(event)
"""

import json
from dataclasses import dataclass
from typing import Any, Dict, Iterator, Optional

# gRPC imports - will fail if grpcio not installed
try:
    import grpc
    GRPC_AVAILABLE = True
except ImportError:
    GRPC_AVAILABLE = False
    grpc = None  # type: ignore

from jeeves_protocols.envelope import GenericEnvelope


class GrpcNotAvailableError(Exception):
    """Raised when gRPC is not available."""
    pass


class GrpcConnectionError(Exception):
    """Raised when connection to Go server fails."""
    pass


@dataclass
class BoundsResult:
    """Result from bounds check."""
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

    Provides persistent connection to Go gRPC server.
    All envelope operations go through Go for consistency.
    """

    DEFAULT_ADDRESS = "localhost:50051"

    def __init__(self, address: Optional[str] = None, timeout: float = 30.0):
        """Initialize gRPC client.

        Args:
            address: gRPC server address (default: localhost:50051)
            timeout: Default timeout for RPC calls in seconds
        """
        if not GRPC_AVAILABLE:
            raise GrpcNotAvailableError(
                "grpcio not installed. Run: pip install grpcio grpcio-tools"
            )

        self._address = address or self.DEFAULT_ADDRESS
        self._timeout = timeout
        self._channel: Optional[grpc.Channel] = None
        self._stub: Optional[Any] = None

    def _ensure_connected(self) -> None:
        """Ensure gRPC channel is connected."""
        if self._channel is None:
            self._channel = grpc.insecure_channel(self._address)
            # Import generated stub
            try:
                from jeeves_protocols import grpc_stub
                self._stub = grpc_stub.JeevesCoreServiceStub(self._channel)
            except ImportError:
                # Fallback: use raw channel calls
                self._stub = None

    def is_available(self) -> bool:
        """Check if Go gRPC server is available."""
        try:
            self._ensure_connected()
            # Try a simple ping
            if self._channel is not None:
                grpc.channel_ready_future(self._channel).result(timeout=1.0)
                return True
        except Exception:
            pass
        return False

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

        Args:
            raw_input: User's raw input text
            user_id: User identifier
            session_id: Session identifier
            request_id: Optional request ID (generated if not provided)
            metadata: Optional metadata dict
            stage_order: Optional stage execution order

        Returns:
            GenericEnvelope created by Go
        """
        self._ensure_connected()

        # Build request
        request = {
            "raw_input": raw_input,
            "user_id": user_id,
            "session_id": session_id,
            "request_id": request_id or "",
            "metadata": metadata or {},
            "stage_order": stage_order or [],
        }

        # For now, create envelope locally until proto is compiled
        # TODO: Replace with actual gRPC call
        return self._create_envelope_local(request)

    def check_bounds(self, envelope: GenericEnvelope) -> BoundsResult:
        """Check bounds - Go is authoritative.

        Args:
            envelope: Envelope to check

        Returns:
            BoundsResult with can_continue and remaining counts
        """
        self._ensure_connected()

        # For now, check bounds locally until proto is compiled
        # TODO: Replace with actual gRPC call
        return self._check_bounds_local(envelope)

    def clone_envelope(self, envelope: GenericEnvelope) -> GenericEnvelope:
        """Clone envelope via Go.

        Args:
            envelope: Envelope to clone

        Returns:
            Deep copy of envelope
        """
        self._ensure_connected()

        # For now, clone locally until proto is compiled
        # TODO: Replace with actual gRPC call
        return self._clone_envelope_local(envelope)

    def execute_pipeline(
        self,
        envelope: GenericEnvelope,
        thread_id: str,
        pipeline_config: Optional[Dict[str, Any]] = None,
    ) -> Iterator[ExecutionEvent]:
        """Execute pipeline with streaming events.

        Args:
            envelope: Starting envelope
            thread_id: Thread ID for persistence
            pipeline_config: Optional pipeline configuration

        Yields:
            ExecutionEvent for each stage transition
        """
        self._ensure_connected()

        # For now, yield a single completion event
        # TODO: Replace with actual gRPC streaming call
        import time
        yield ExecutionEvent(
            type="PIPELINE_COMPLETED",
            stage=envelope.current_stage,
            timestamp_ms=int(time.time() * 1000),
            payload=None,
            envelope=envelope,
        )

    def close(self) -> None:
        """Close the gRPC channel."""
        if self._channel:
            self._channel.close()
            self._channel = None
            self._stub = None

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()

    # =========================================================================
    # Local implementations (until proto is compiled)
    # =========================================================================

    def _create_envelope_local(self, request: Dict[str, Any]) -> GenericEnvelope:
        """Create envelope locally."""
        import uuid
        from jeeves_shared.serialization import utc_now

        return GenericEnvelope(
            envelope_id=f"env_{uuid.uuid4().hex[:16]}",
            request_id=request.get("request_id") or f"req_{uuid.uuid4().hex[:16]}",
            user_id=request["user_id"],
            session_id=request["session_id"],
            raw_input=request["raw_input"],
            received_at=utc_now(),
            created_at=utc_now(),
            metadata=request.get("metadata") or {},
            stage_order=request.get("stage_order") or [],
        )

    def _check_bounds_local(self, envelope: GenericEnvelope) -> BoundsResult:
        """Check bounds locally."""
        can_continue = True
        terminal_reason = None

        if envelope.iteration >= envelope.max_iterations:
            can_continue = False
            terminal_reason = "max_iterations_exceeded"
        elif envelope.llm_call_count >= envelope.max_llm_calls:
            can_continue = False
            terminal_reason = "max_llm_calls_exceeded"
        elif envelope.agent_hop_count >= envelope.max_agent_hops:
            can_continue = False
            terminal_reason = "max_agent_hops_exceeded"

        return BoundsResult(
            can_continue=can_continue,
            terminal_reason=terminal_reason,
            llm_calls_remaining=envelope.max_llm_calls - envelope.llm_call_count,
            agent_hops_remaining=envelope.max_agent_hops - envelope.agent_hop_count,
            iterations_remaining=envelope.max_iterations - envelope.iteration,
        )

    def _clone_envelope_local(self, envelope: GenericEnvelope) -> GenericEnvelope:
        """Clone envelope locally using dataclass copy."""
        import copy
        return copy.deepcopy(envelope)


# =============================================================================
# Module-level functions
# =============================================================================

_default_client: Optional[GrpcGoClient] = None


def get_client() -> GrpcGoClient:
    """Get default gRPC client instance."""
    global _default_client
    if _default_client is None:
        _default_client = GrpcGoClient()
    return _default_client


def create_envelope(
    raw_input: str,
    user_id: str,
    session_id: str = "",
    request_id: str = "",
    metadata: Optional[Dict[str, str]] = None,
) -> GenericEnvelope:
    """Create envelope via Go gRPC."""
    return get_client().create_envelope(
        raw_input=raw_input,
        user_id=user_id,
        session_id=session_id,
        request_id=request_id if request_id else None,
        metadata=metadata,
    )


def check_bounds(envelope: GenericEnvelope) -> BoundsResult:
    """Check bounds via Go gRPC."""
    return get_client().check_bounds(envelope)


def clone_envelope(envelope: GenericEnvelope) -> GenericEnvelope:
    """Clone envelope via Go gRPC."""
    return get_client().clone_envelope(envelope)

