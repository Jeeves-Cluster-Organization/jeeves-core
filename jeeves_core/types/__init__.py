"""Core type definitions - backed by Go kernel.

Source of truth: coreengine/envelope/, coreengine/config/
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

from jeeves_core.types.config import (
    AgentCapability,
    AgentConfig,
    AgentOutputMode,
    ContextBounds,
    EdgeLimit,
    ExecutionConfig,
    GenerationParams,
    JoinStrategy,
    OrchestrationFlags,
    PipelineConfig,
    RoutingRule,
    RunMode,
    TokenStreamMode,
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
    # Config types
    "RunMode",
    "JoinStrategy",
    "AgentOutputMode",
    "TokenStreamMode",
    "AgentCapability",
    "RoutingRule",
    "EdgeLimit",
    "GenerationParams",
    "AgentConfig",
    "PipelineConfig",
    "ContextBounds",
    "ExecutionConfig",
    "OrchestrationFlags",
]
