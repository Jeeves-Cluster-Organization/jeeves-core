"""Core enums - Python interface to Go kernel definitions.

Source of truth: coreengine/envelope/enums.go
"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Dict, List, Optional


class RiskLevel(str, Enum):
    """Risk level for tool execution."""
    # Semantic levels
    READ_ONLY = "read_only"
    WRITE = "write"
    DESTRUCTIVE = "destructive"
    # Severity levels
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"

    @classmethod
    def requires_confirmation(cls, level: "RiskLevel") -> bool:
        """Check if a risk level requires user confirmation."""
        return level in (cls.DESTRUCTIVE, cls.HIGH, cls.CRITICAL)


class ToolCategory(str, Enum):
    """Tool categorization."""
    # Operation types
    READ = "read"
    WRITE = "write"
    EXECUTE = "execute"
    NETWORK = "network"
    SYSTEM = "system"
    # Tool organization
    UNIFIED = "unified"
    COMPOSITE = "composite"
    RESILIENT = "resilient"
    STANDALONE = "standalone"
    INTERNAL = "internal"


class HealthStatus(str, Enum):
    """Health check status."""
    HEALTHY = "healthy"
    DEGRADED = "degraded"
    UNHEALTHY = "unhealthy"
    UNKNOWN = "unknown"


class TerminalReason(str, Enum):
    """Reasons for envelope termination."""
    COMPLETED = "completed"
    MAX_ITERATIONS_EXCEEDED = "max_iterations_exceeded"
    MAX_LLM_CALLS_EXCEEDED = "max_llm_calls_exceeded"
    MAX_AGENT_HOPS_EXCEEDED = "max_agent_hops_exceeded"
    MAX_LOOP_EXCEEDED = "max_loop_exceeded"
    USER_CANCELLED = "user_cancelled"
    TOOL_FAILED_FATALLY = "tool_failed_fatally"
    LLM_FAILED_FATALLY = "llm_failed_fatally"
    POLICY_VIOLATION = "policy_violation"


class LoopVerdict(str, Enum):
    """Agent loop control verdicts."""
    PROCEED = "proceed"
    LOOP_BACK = "loop_back"
    ADVANCE = "advance"
    ESCALATE = "escalate"


class RiskApproval(str, Enum):
    """Risk approval status."""
    APPROVED = "approved"
    DENIED = "denied"
    PENDING = "pending"


class ToolAccess(str, Enum):
    """Tool access levels for agents."""
    NONE = "none"
    READ = "read"
    WRITE = "write"
    ALL = "all"


class OperationStatus(str, Enum):
    """Status of an operation result."""
    SUCCESS = "success"
    ERROR = "error"
    NOT_FOUND = "not_found"
    TIMEOUT = "timeout"
    VALIDATION_ERROR = "validation_error"
    PARTIAL = "partial"
    INVALID_PARAMETERS = "invalid_parameters"


@dataclass
class OperationResult:
    """Unified result type for operations."""
    status: OperationStatus
    data: Optional[Dict[str, Any]] = None
    error: Optional[str] = None
    error_type: Optional[str] = None
    suggestions: List[str] = field(default_factory=list)

    @property
    def is_success(self) -> bool:
        """Check if operation succeeded."""
        return self.status == OperationStatus.SUCCESS

    @property
    def is_error(self) -> bool:
        """Check if operation failed."""
        return self.status != OperationStatus.SUCCESS

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        result = {"status": self.status.value}
        if self.data is not None:
            result["data"] = self.data
        if self.error is not None:
            result["error"] = self.error
        if self.error_type is not None:
            result["error_type"] = self.error_type
        if self.suggestions:
            result["suggestions"] = self.suggestions
        return result

    @classmethod
    def success(cls, data: Optional[Dict[str, Any]] = None) -> "OperationResult":
        """Create a success result."""
        return cls(status=OperationStatus.SUCCESS, data=data or {})

    @classmethod
    def error(
        cls,
        message: str,
        error_type: str = "unknown",
        suggestions: Optional[List[str]] = None,
    ) -> "OperationResult":
        """Create an error result."""
        return cls(
            status=OperationStatus.ERROR,
            error=message,
            error_type=error_type,
            suggestions=suggestions or [],
        )

    @classmethod
    def not_found(
        cls,
        message: str = "Resource not found",
        suggestions: Optional[List[str]] = None,
    ) -> "OperationResult":
        """Create a not found result."""
        return cls(
            status=OperationStatus.NOT_FOUND,
            error=message,
            error_type="not_found",
            suggestions=suggestions or [],
        )

    @classmethod
    def validation_error(
        cls,
        message: str,
        suggestions: Optional[List[str]] = None,
    ) -> "OperationResult":
        """Create a validation error result."""
        return cls(
            status=OperationStatus.VALIDATION_ERROR,
            error=message,
            error_type="validation_error",
            suggestions=suggestions or [],
        )
