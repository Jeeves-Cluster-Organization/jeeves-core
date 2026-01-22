"""gRPC stub re-exports for jeeves_protocols.

This module re-exports the generated protobuf classes from coreengine.proto
to provide a clean import path for the jeeves_protocols package.

Usage:
    from jeeves_protocols import grpc_stub
    stub = grpc_stub.JeevesCoreServiceStub(channel)
    request = grpc_stub.CreateEnvelopeRequest(...)
"""

# Re-export from coreengine proto
from coreengine.proto.jeeves_core_pb2 import (
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

from coreengine.proto.jeeves_core_pb2_grpc import (
    JeevesCoreServiceStub,
    JeevesCoreServiceServicer,
    add_JeevesCoreServiceServicer_to_server,
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
    "JeevesCoreServiceStub",
    "JeevesCoreServiceServicer",
    "add_JeevesCoreServiceServicer_to_server",
]
