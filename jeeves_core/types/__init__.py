"""Core type definitions - backed by Go kernel.

Source of truth: coreengine/envelope/
"""

from jeeves_core.types.enums import (
    HealthStatus,
    LoopVerdict,
    OperationResult,
    OperationStatus,
    RiskApproval,
    RiskLevel,
    TerminalReason,
    ToolAccess,
    ToolCategory,
)

from jeeves_core.types.interrupts import (
    FlowInterrupt,
    InterruptKind,
    InterruptResponse,
    InterruptServiceProtocol,
    InterruptStatus,
    RateLimitConfig,
    RateLimiterProtocol,
    RateLimitResult,
)

from jeeves_core.types.envelope import (
    Envelope,
    PipelineEvent,
    ProcessingRecord,
)

__all__ = [
    # Enums
    "RiskLevel",
    "ToolCategory",
    "HealthStatus",
    "TerminalReason",
    "LoopVerdict",
    "RiskApproval",
    "ToolAccess",
    "OperationStatus",
    "OperationResult",
    # Interrupt types
    "InterruptKind",
    "InterruptStatus",
    "InterruptResponse",
    "FlowInterrupt",
    "InterruptServiceProtocol",
    "RateLimitConfig",
    "RateLimitResult",
    "RateLimiterProtocol",
    # Envelope types
    "Envelope",
    "ProcessingRecord",
    "PipelineEvent",
]
