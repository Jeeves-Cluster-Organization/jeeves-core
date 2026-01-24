"""Core enums and constants - mirrors Go definitions."""

from enum import Enum


class RiskLevel(str, Enum):
    """Risk level for tool execution.

    Semantic levels (used by tool definitions):
    - READ_ONLY: Safe read operations, no side effects
    - WRITE: Operations that modify state
    - DESTRUCTIVE: Operations that delete data or are irreversible

    Severity levels (used by risk assessment):
    - LOW: Minimal risk
    - MEDIUM: Moderate risk, may need review
    - HIGH: Significant risk, requires confirmation
    - CRITICAL: Highest risk, requires explicit approval
    """
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
    """Tool categorization.

    Operation types:
    - READ: Read-only operations
    - WRITE: Modification operations
    - EXECUTE: Code/command execution
    - NETWORK: Network operations
    - SYSTEM: System-level operations

    Tool organization (for code analysis capability):
    - UNIFIED: Single entry point tools (e.g., analyze)
    - COMPOSITE: Multi-step orchestration tools
    - RESILIENT: Tools with retry/fallback strategies
    - STANDALONE: Independent utility tools
    - INTERNAL: Internal tools not exposed to agents
    """
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


class TerminalReason(str, Enum):
    """Reasons for envelope termination."""
    COMPLETED = "completed"
    MAX_ITERATIONS_EXCEEDED = "max_iterations_exceeded"
    MAX_LLM_CALLS_EXCEEDED = "max_llm_calls_exceeded"
    MAX_AGENT_HOPS_EXCEEDED = "max_agent_hops_exceeded"
    USER_CANCELLED = "user_cancelled"
    TOOL_FAILED_FATALLY = "tool_failed_fatally"
    POLICY_VIOLATION = "policy_violation"


class LoopVerdict(str, Enum):
    """Agent loop control verdicts - generic routing decisions."""
    PROCEED = "proceed"      # Continue to next stage
    LOOP_BACK = "loop_back"  # Return to an earlier stage
    ADVANCE = "advance"      # Skip ahead to a later stage
    ESCALATE = "escalate"    # Escalate to human


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
    """Status of an operation result.

    Core statuses:
    - SUCCESS: Operation completed successfully
    - ERROR: Operation failed with error
    - NOT_FOUND: Resource/target not found (valid outcome, not an error)
    - TIMEOUT: Operation timed out
    - VALIDATION_ERROR: Input validation failed

    Extended statuses (for tool operations):
    - PARTIAL: Some results found, but incomplete
    - INVALID_PARAMETERS: Semantic misuse (e.g., file path as symbol)
    """
    SUCCESS = "success"
    ERROR = "error"
    NOT_FOUND = "not_found"
    TIMEOUT = "timeout"
    VALIDATION_ERROR = "validation_error"
    # Extended statuses for tool operations
    PARTIAL = "partial"
    INVALID_PARAMETERS = "invalid_parameters"


from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional


@dataclass
class OperationResult:
    """Unified result type for operations across the codebase.

    Standardizes error responses and success results to eliminate
    inconsistent patterns like:
    - {"status": "error", "error": str}
    - {"status": "error", "error_type": str}
    - Raw exception raising

    Usage:
        # Success
        result = OperationResult.success({"items": [...]})

        # Error
        result = OperationResult.error("File not found", "not_found")

        # Check result
        if result.is_success:
            process(result.data)
        else:
            handle_error(result.error)
    """
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
