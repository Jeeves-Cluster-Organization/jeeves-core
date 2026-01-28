"""gRPC stub re-exports for protocols.

This module re-exports the generated protobuf classes from jeeves_infra.protocols
to provide a clean import path for the protocols package.

Usage:
    from protocols import grpc_stub
    stub = grpc_stub.EngineServiceStub(channel)
    request = grpc_stub.CreateEnvelopeRequest(...)
"""

# Re-export from jeeves-infra protocols (proto-generated)
from jeeves_infra.protocols.engine_pb2 import (
    # Envelope messages
    CreateEnvelopeRequest,
    UpdateEnvelopeRequest,
    CloneRequest,
    Envelope,
    # Bounds
    BoundsResult,
    # Execution
    ExecuteRequest,
    ExecuteAgentRequest,
    ExecutionEvent,
    AgentResult,
    # Interrupt
    FlowInterrupt,
    InterruptResponse,
    # Enums
    TerminalReason,
    InterruptKind,
    ExecutionEventType,
)

from jeeves_infra.protocols.engine_pb2_grpc import (
    EngineServiceStub,
    EngineServiceServicer,
    add_EngineServiceServicer_to_server,
)

__all__ = [
    # Messages
    "CreateEnvelopeRequest",
    "UpdateEnvelopeRequest",
    "CloneRequest",
    "Envelope",
    "BoundsResult",
    "ExecuteRequest",
    "ExecuteAgentRequest",
    "ExecutionEvent",
    "AgentResult",
    "FlowInterrupt",
    "InterruptResponse",
    # Enums
    "TerminalReason",
    "InterruptKind",
    "ExecutionEventType",
    # Service
    "EngineServiceStub",
    "EngineServiceServicer",
    "add_EngineServiceServicer_to_server",
]
