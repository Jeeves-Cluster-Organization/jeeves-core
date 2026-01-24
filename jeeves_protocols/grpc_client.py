"""gRPC client for Go runtime.

Go gRPC server is REQUIRED. No fallbacks.
If Go server is not running, operations fail immediately.

Constitutional Compliance:
- Go-Only Mode (HANDOFF.md): All operations require Go server
- Contract 11 (CONTRACTS.md): Envelope round-trip must be lossless
- Contract 12 (CONTRACTS.md): Go is authoritative for bounds checking
"""

import json
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Dict, Iterator, Optional, TYPE_CHECKING

try:
    import grpc
except ImportError as e:
    raise ImportError(
        "grpcio is REQUIRED. Install with: pip install grpcio grpcio-tools"
    ) from e

from jeeves_protocols.envelope import GenericEnvelope
from jeeves_protocols import RequestContext
from jeeves_protocols.core import TerminalReason

if TYPE_CHECKING:
    from jeeves_protocols import grpc_stub as stub_types


# =============================================================================
# Exceptions
# =============================================================================


class GrpcConnectionError(Exception):
    """Raised when connection to Go server fails."""
    pass


class GrpcCallError(Exception):
    """Raised when a gRPC call fails."""
    pass


class GoServerNotRunningError(Exception):
    """Raised when Go gRPC server is not running."""
    pass


# =============================================================================
# Result Types
# =============================================================================


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


@dataclass
class AgentResult:
    """Result from single agent execution."""
    success: bool
    output: Optional[Dict[str, Any]]
    error: Optional[str]
    duration_ms: int
    llm_calls: int
    envelope: Optional[GenericEnvelope]


# =============================================================================
# Terminal Reason Conversion (Contract 11 compliance)
# =============================================================================


def _terminal_reason_to_proto(reason: Optional[TerminalReason], pb2: Any) -> int:
    """Convert Python TerminalReason to proto enum value.
    
    Args:
        reason: Python TerminalReason enum or None
        pb2: Proto module (for enum access) - not used, kept for API compatibility
        
    Returns:
        Proto enum integer value
    """
    if reason is None:
        return 0  # TERMINAL_REASON_UNSPECIFIED
    
    mapping = {
        TerminalReason.MAX_ITERATIONS_EXCEEDED: 1,
        TerminalReason.MAX_LLM_CALLS_EXCEEDED: 2,
        TerminalReason.MAX_AGENT_HOPS_EXCEEDED: 3,
        TerminalReason.USER_CANCELLED: 4,
        TerminalReason.TOOL_FAILED_FATALLY: 5,
        TerminalReason.POLICY_VIOLATION: 6,
        TerminalReason.COMPLETED: 7,
    }
    return mapping.get(reason, 0)


def _proto_to_terminal_reason(proto_value: int) -> Optional[TerminalReason]:
    """Convert proto enum value to Python TerminalReason.
    
    Args:
        proto_value: Proto enum integer value
        
    Returns:
        Python TerminalReason enum or None
    """
    mapping = {
        1: TerminalReason.MAX_ITERATIONS_EXCEEDED,
        2: TerminalReason.MAX_LLM_CALLS_EXCEEDED,
        3: TerminalReason.MAX_AGENT_HOPS_EXCEEDED,
        4: TerminalReason.USER_CANCELLED,
        5: TerminalReason.TOOL_FAILED_FATALLY,
        6: TerminalReason.POLICY_VIOLATION,
        7: TerminalReason.COMPLETED,
    }
    return mapping.get(proto_value)


# =============================================================================
# gRPC Client
# =============================================================================


class GrpcGoClient:
    """gRPC client for Go runtime. Go server is REQUIRED."""

    DEFAULT_ADDRESS = "localhost:50051"
    CONNECT_TIMEOUT = 5.0

    def __init__(self, address: Optional[str] = None, timeout: float = 30.0):
        """Initialize gRPC client.

        Args:
            address: gRPC server address (default: localhost:50051)
            timeout: Default timeout for RPC calls in seconds
        """
        self._address = address or self.DEFAULT_ADDRESS
        self._timeout = timeout
        self._channel: Optional[grpc.Channel] = None
        self._stub: Any = None
        self._pb2: Any = None

    def connect(self) -> None:
        """Establish connection to Go server.

        Raises:
            GoServerNotRunningError: If connection fails
        """
        if self._channel is not None:
            return

        self._channel = grpc.insecure_channel(self._address)

        try:
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

        try:
            from jeeves_protocols import grpc_stub
            from coreengine.proto import jeeves_core_pb2
            self._stub = grpc_stub.JeevesCoreServiceStub(self._channel)
            self._pb2 = jeeves_core_pb2
        except ImportError as e:
            raise ImportError(
                "gRPC stubs not generated. Run: "
                "python -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. "
                "coreengine/proto/jeeves_core.proto"
            ) from e

    def _require_connection(self) -> None:
        """Ensure connected."""
        if self._channel is None:
            self.connect()

    def create_envelope(
        self,
        raw_input: str,
        request_context: RequestContext,
        metadata: Optional[Dict[str, str]] = None,
        stage_order: Optional[list] = None,
    ) -> GenericEnvelope:
        """Create envelope via Go runtime.
        
        Args:
            raw_input: User's raw input text
            request_context: RequestContext for the request
            metadata: Optional metadata dict
            stage_order: Optional stage execution order
            
        Returns:
            GenericEnvelope created by Go
        """
        self._require_connection()

        try:
            from jeeves_protocols import grpc_stub
            if not isinstance(request_context, RequestContext):
                raise TypeError("request_context must be a RequestContext instance")
            request_metadata = dict(metadata or {})
            request_metadata["request_context"] = json.dumps(request_context.to_dict())
            request = grpc_stub.CreateEnvelopeRequest(
                raw_input=raw_input,
                user_id=request_context.user_id or "",
                session_id=request_context.session_id or "",
                request_id=request_context.request_id,
                metadata=request_metadata,
                stage_order=stage_order or [],
            )
            # CreateEnvelope returns Envelope directly, not wrapped
            response = self._stub.CreateEnvelope(request, timeout=self._timeout)
            return self._envelope_from_proto(response)
        except grpc.RpcError as e:
            raise GrpcCallError(f"CreateEnvelope failed: {e.details()}") from e

    def update_envelope(
        self,
        envelope: GenericEnvelope,
        agent_name: Optional[str] = None,
        output: Optional[Dict[str, Any]] = None,
        llm_calls_made: int = 0,
    ) -> GenericEnvelope:
        """Update envelope after agent execution.

        Updates the envelope state and syncs to Go. If agent_name is provided,
        increments agent_hop_count and stores output.

        Args:
            envelope: Envelope to update
            agent_name: Name of agent that executed (optional)
            output: Agent output to store (optional)
            llm_calls_made: Number of LLM calls made by agent

        Returns:
            Updated envelope from Go
        """
        # Apply local updates first
        envelope.llm_call_count += llm_calls_made
        if agent_name:
            envelope.agent_hop_count += 1
            if output is not None:
                envelope.outputs[agent_name] = output

        self._require_connection()

        try:
            from jeeves_protocols import grpc_stub
            request = grpc_stub.UpdateEnvelopeRequest(
                envelope=self._envelope_to_proto(envelope)
            )
            response = self._stub.UpdateEnvelope(request, timeout=self._timeout)
            return self._envelope_from_proto(response)
        except grpc.RpcError as e:
            raise GrpcCallError(f"UpdateEnvelope failed: {e.details()}") from e

    def check_bounds(self, envelope: GenericEnvelope) -> BoundsResult:
        """Check bounds - Go is authoritative (Contract 12).
        
        Args:
            envelope: Envelope to check bounds for
            
        Returns:
            BoundsResult with can_continue and remaining quotas
        """
        self._require_connection()

        try:
            # CheckBounds takes Envelope directly, no wrapper
            proto_envelope = self._envelope_to_proto(envelope)
            response = self._stub.CheckBounds(proto_envelope, timeout=self._timeout)
            
            # Convert terminal_reason from proto enum to string
            terminal_reason_enum = _proto_to_terminal_reason(response.terminal_reason)
            terminal_reason_str = terminal_reason_enum.value if terminal_reason_enum else None
            
            return BoundsResult(
                can_continue=response.can_continue,
                terminal_reason=terminal_reason_str,
                llm_calls_remaining=response.llm_calls_remaining,
                agent_hops_remaining=response.agent_hops_remaining,
                iterations_remaining=response.iterations_remaining,
            )
        except grpc.RpcError as e:
            raise GrpcCallError(f"CheckBounds failed: {e.details()}") from e

    def clone_envelope(self, envelope: GenericEnvelope) -> GenericEnvelope:
        """Clone envelope via Go.
        
        Args:
            envelope: Envelope to clone
            
        Returns:
            Deep copy of envelope from Go
        """
        self._require_connection()

        try:
            from jeeves_protocols import grpc_stub
            # CloneEnvelope uses CloneRequest
            request = grpc_stub.CloneRequest(
                envelope=self._envelope_to_proto(envelope)
            )
            response = self._stub.CloneEnvelope(request, timeout=self._timeout)
            return self._envelope_from_proto(response)
        except grpc.RpcError as e:
            raise GrpcCallError(f"CloneEnvelope failed: {e.details()}") from e

    def execute_agent(
        self,
        envelope: GenericEnvelope,
        agent_name: str,
        agent_config: Optional[Dict[str, Any]] = None,
    ) -> AgentResult:
        """Execute a single agent via Go.
        
        Args:
            envelope: Current envelope state
            agent_name: Name of agent to execute
            agent_config: Optional agent configuration
            
        Returns:
            AgentResult with output and updated envelope
        """
        self._require_connection()

        try:
            from jeeves_protocols import grpc_stub
            request = grpc_stub.ExecuteAgentRequest(
                envelope=self._envelope_to_proto(envelope),
                agent_name=agent_name,
                agent_config=json.dumps(agent_config or {}).encode(),
            )
            response = self._stub.ExecuteAgent(request, timeout=self._timeout)
            
            output = None
            if response.output:
                try:
                    output = json.loads(response.output.decode())
                except (json.JSONDecodeError, UnicodeDecodeError):
                    output = {"raw": response.output.decode()}
            
            return AgentResult(
                success=response.success,
                output=output,
                error=response.error or None,
                duration_ms=response.duration_ms,
                llm_calls=response.llm_calls,
                envelope=self._envelope_from_proto(response.envelope) if response.envelope else None,
            )
        except grpc.RpcError as e:
            raise GrpcCallError(f"ExecuteAgent failed: {e.details()}") from e

    def execute_pipeline(
        self,
        envelope: GenericEnvelope,
        thread_id: str,
        pipeline_config: Optional[Dict[str, Any]] = None,
    ) -> Iterator[ExecutionEvent]:
        """Execute pipeline with streaming events.
        
        Args:
            envelope: Initial envelope state
            thread_id: Thread identifier for persistence
            pipeline_config: Optional pipeline configuration
            
        Yields:
            ExecutionEvent for each pipeline stage
        """
        self._require_connection()

        try:
            from jeeves_protocols import grpc_stub

            request = grpc_stub.ExecuteRequest(
                envelope=self._envelope_to_proto(envelope),
                thread_id=thread_id,
                pipeline_config=json.dumps(pipeline_config or {}).encode(),
            )

            # Use longer timeout for pipeline execution
            for event in self._stub.ExecutePipeline(request, timeout=self._timeout * 10):
                # Map proto event type to string
                event_type_map = {
                    0: "UNSPECIFIED",
                    1: "STAGE_STARTED",
                    2: "STAGE_COMPLETED",
                    3: "STAGE_FAILED",
                    4: "PIPELINE_COMPLETED",
                    5: "INTERRUPT_RAISED",
                    6: "CHECKPOINT_CREATED",
                    7: "BOUNDS_EXCEEDED",
                }
                event_type = event_type_map.get(event.type, "UNKNOWN")
                
                payload = None
                if event.payload:
                    try:
                        payload = json.loads(event.payload.decode())
                    except (json.JSONDecodeError, UnicodeDecodeError):
                        payload = {"raw": event.payload.decode()}
                
                yield ExecutionEvent(
                    type=event_type,
                    stage=event.stage,
                    timestamp_ms=event.timestamp_ms,
                    payload=payload,
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
            self._pb2 = None

    def __enter__(self):
        self.connect()
        return self

    def __exit__(self, *args):
        self.close()

    # =========================================================================
    # Envelope Conversion (Contract 11: Lossless round-trip)
    # =========================================================================

    def _envelope_to_proto(self, envelope: GenericEnvelope) -> Any:
        """Convert Python envelope to proto message.
        
        Contract 11 compliance: All fields must be serialized for lossless round-trip.
        """
        from jeeves_protocols import grpc_stub

        proto = grpc_stub.Envelope(
            # Identification
            envelope_id=envelope.envelope_id,
            request_id=envelope.request_id,
            user_id=envelope.user_id,
            session_id=envelope.session_id,
            # Input
            raw_input=envelope.raw_input,
            received_at_ms=int(envelope.received_at.timestamp() * 1000) if envelope.received_at else 0,
            # Pipeline state
            current_stage=envelope.current_stage or "",
            stage_order=envelope.stage_order,
            iteration=envelope.iteration,
            max_iterations=envelope.max_iterations,
            # Bounds
            llm_call_count=envelope.llm_call_count,
            max_llm_calls=envelope.max_llm_calls,
            agent_hop_count=envelope.agent_hop_count,
            max_agent_hops=envelope.max_agent_hops,
            # Termination
            terminated=envelope.terminated,
            termination_reason=envelope.termination_reason or "",
            terminal_reason=_terminal_reason_to_proto(envelope.terminal_reason, None),
            completed_at_ms=int(envelope.completed_at.timestamp() * 1000) if envelope.completed_at else 0,
            # Interrupt
            interrupt_pending=envelope.interrupt_pending,
            # Multi-stage
            current_stage_number=envelope.current_stage_number,
            max_stages=envelope.max_stages,
            all_goals=envelope.all_goals,
            remaining_goals=envelope.remaining_goals,
            # Timing
            created_at_ms=int(envelope.created_at.timestamp() * 1000) if envelope.created_at else 0,
        )
        
        # Outputs as JSON bytes (map<string, bytes>)
        if envelope.outputs:
            for key, val in envelope.outputs.items():
                proto.outputs[key] = json.dumps(val).encode() if val else b""
        
        # Parallel execution state (map<string, bool>)
        if envelope.active_stages:
            for key, val in envelope.active_stages.items():
                proto.active_stages[key] = val
        
        if envelope.completed_stage_set:
            for key, val in envelope.completed_stage_set.items():
                proto.completed_stage_set[key] = val
        
        if envelope.failed_stages:
            for key, val in envelope.failed_stages.items():
                proto.failed_stages[key] = val
        
        # Goal completion status (map<string, string>)
        if envelope.goal_completion_status:
            for key, val in envelope.goal_completion_status.items():
                proto.goal_completion_status[key] = val
        
        # Metadata as string map
        if envelope.metadata:
            for key, val in envelope.metadata.items():
                proto.metadata_str[key] = str(val) if val is not None else ""
        # Always embed request_context for round-trip fidelity
        proto.metadata_str["request_context"] = json.dumps(
            envelope.request_context.to_dict()
        )
        
        # Interrupt (FlowInterrupt message)
        if envelope.interrupt:
            interrupt_data = envelope.interrupt
            if hasattr(interrupt_data, 'to_dict'):
                interrupt_data = interrupt_data.to_dict()
            if isinstance(interrupt_data, dict):
                proto.interrupt.kind = interrupt_data.get('kind', 0)
                proto.interrupt.interrupt_id = interrupt_data.get('interrupt_id', '') or interrupt_data.get('id', '')
                proto.interrupt.question = interrupt_data.get('question', '')
                proto.interrupt.message = interrupt_data.get('message', '')
                if interrupt_data.get('data'):
                    proto.interrupt.data = json.dumps(interrupt_data['data']).encode()
                proto.interrupt.created_at_ms = int(interrupt_data.get('created_at_ms', 0))
        
        return proto

    def _envelope_from_proto(self, proto: Any) -> GenericEnvelope:
        """Convert proto message to Python envelope.
        
        Contract 11 compliance: All fields must be deserialized for lossless round-trip.
        """
        # Parse outputs from bytes
        outputs: Dict[str, Any] = {}
        for key, val in proto.outputs.items():
            try:
                outputs[key] = json.loads(val.decode()) if val else None
            except (json.JSONDecodeError, UnicodeDecodeError):
                outputs[key] = {"raw": val.decode()} if val else None
        
        # Parse terminal reason from proto enum
        terminal_reason = _proto_to_terminal_reason(proto.terminal_reason)
        
        # Parse metadata from metadata_str
        metadata: Dict[str, Any] = dict(proto.metadata_str)

        # Extract request_context from metadata (required)
        request_context_raw = metadata.get("request_context")
        if not request_context_raw:
            raise ValueError("request_context missing in proto metadata")
        if isinstance(request_context_raw, str):
            try:
                request_context_data = json.loads(request_context_raw)
            except json.JSONDecodeError as exc:
                raise ValueError("request_context metadata is not valid JSON") from exc
        elif isinstance(request_context_raw, dict):
            request_context_data = request_context_raw
        else:
            raise TypeError("request_context metadata must be JSON string or dict")

        request_context = RequestContext(**request_context_data)
        # Remove raw request_context from metadata to avoid duplication
        metadata.pop("request_context", None)
        
        # Parse interrupt
        interrupt = None
        if proto.HasField("interrupt") if hasattr(proto, 'HasField') else proto.interrupt.interrupt_id:
            pi = proto.interrupt
            interrupt_data: Dict[str, Any] = {}
            if pi.data:
                try:
                    interrupt_data = json.loads(pi.data.decode())
                except (json.JSONDecodeError, UnicodeDecodeError):
                    interrupt_data = {}
            interrupt = {
                "kind": pi.kind,
                "id": pi.interrupt_id,
                "interrupt_id": pi.interrupt_id,
                "question": pi.question,
                "message": pi.message,
                "data": interrupt_data,
                "created_at_ms": pi.created_at_ms,
            }
            # Parse response if present
            if hasattr(pi, 'response') and pi.response:
                resp = pi.response
                interrupt["response"] = {
                    "text": resp.text,
                    "approved": resp.approved,
                    "decision": resp.decision,
                    "resolved_at_ms": resp.resolved_at_ms,
                }
                if resp.data:
                    try:
                        interrupt["response"]["data"] = json.loads(resp.data.decode())
                    except (json.JSONDecodeError, UnicodeDecodeError):
                        pass
        
        return GenericEnvelope(
            # Identification
            request_context=request_context,
            envelope_id=proto.envelope_id,
            request_id=proto.request_id,
            user_id=proto.user_id,
            session_id=proto.session_id,
            # Input
            raw_input=proto.raw_input,
            received_at=datetime.fromtimestamp(proto.received_at_ms / 1000, tz=timezone.utc) if proto.received_at_ms else None,
            # Outputs
            outputs=outputs,
            # Pipeline state
            current_stage=proto.current_stage or "start",
            stage_order=list(proto.stage_order),
            iteration=proto.iteration,
            max_iterations=proto.max_iterations or 3,
            # Bounds
            llm_call_count=proto.llm_call_count,
            max_llm_calls=proto.max_llm_calls or 10,
            agent_hop_count=proto.agent_hop_count,
            max_agent_hops=proto.max_agent_hops or 21,
            terminal_reason=terminal_reason,
            # Control flow
            terminated=proto.terminated,
            termination_reason=proto.termination_reason or None,
            # Interrupt
            interrupt_pending=proto.interrupt_pending,
            interrupt=interrupt,
            # Parallel execution state
            active_stages=dict(proto.active_stages),
            completed_stage_set=dict(proto.completed_stage_set),
            failed_stages=dict(proto.failed_stages),
            # Multi-stage
            current_stage_number=proto.current_stage_number,
            max_stages=proto.max_stages or 5,
            all_goals=list(proto.all_goals),
            remaining_goals=list(proto.remaining_goals),
            goal_completion_status=dict(proto.goal_completion_status),
            # Timing
            created_at=datetime.fromtimestamp(proto.created_at_ms / 1000, tz=timezone.utc) if proto.created_at_ms else None,
            completed_at=datetime.fromtimestamp(proto.completed_at_ms / 1000, tz=timezone.utc) if proto.completed_at_ms else None,
            # Metadata
            metadata=metadata,
        )


# =============================================================================
# Module-level Singleton and Convenience Functions
# =============================================================================

_default_client: Optional[GrpcGoClient] = None


def get_client(address: Optional[str] = None) -> GrpcGoClient:
    """Get or create the default gRPC client singleton.
    
    Args:
        address: Optional server address (only used on first call)
        
    Returns:
        GrpcGoClient instance
    """
    global _default_client
    if _default_client is None:
        _default_client = GrpcGoClient(address=address)
    return _default_client


def create_envelope(
    raw_input: str,
    request_context: RequestContext,
    metadata: Optional[Dict[str, str]] = None,
    stage_order: Optional[list] = None,
) -> GenericEnvelope:
    """Create envelope via Go gRPC (convenience function)."""
    return get_client().create_envelope(
        raw_input=raw_input,
        request_context=request_context,
        metadata=metadata,
        stage_order=stage_order,
    )


def update_envelope(
    envelope: GenericEnvelope,
    agent_name: Optional[str] = None,
    output: Optional[Dict[str, Any]] = None,
    llm_calls_made: int = 0,
) -> GenericEnvelope:
    """Update envelope via Go gRPC (convenience function)."""
    return get_client().update_envelope(
        envelope=envelope,
        agent_name=agent_name,
        output=output,
        llm_calls_made=llm_calls_made,
    )


def check_bounds(envelope: GenericEnvelope) -> BoundsResult:
    """Check bounds via Go gRPC (convenience function)."""
    return get_client().check_bounds(envelope)


def clone_envelope(envelope: GenericEnvelope) -> GenericEnvelope:
    """Clone envelope via Go gRPC (convenience function)."""
    return get_client().clone_envelope(envelope)
