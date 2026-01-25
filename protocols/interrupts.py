"""Interrupt types and protocols for flow control.

These types define the contract for interrupt handling across layers.
Implementations live in jeeves_control_tower.
"""

from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional, Protocol, runtime_checkable


class InterruptKind(str, Enum):
    """Type of flow interrupt."""
    CLARIFICATION = "clarification"
    CONFIRMATION = "confirmation"
    AGENT_REVIEW = "agent_review"
    CHECKPOINT = "checkpoint"
    RESOURCE_EXHAUSTED = "resource_exhausted"
    TIMEOUT = "timeout"
    SYSTEM_ERROR = "system_error"


class InterruptStatus(str, Enum):
    """Status of an interrupt."""
    PENDING = "pending"
    RESOLVED = "resolved"
    EXPIRED = "expired"
    CANCELLED = "cancelled"


@dataclass
class InterruptResponse:
    """Response to an interrupt.

    Attributes:
        text: For clarification responses
        approved: For confirmation responses (True/False)
        decision: For critic decisions (approve/reject/modify)
        data: Extensible payload for additional data
    """
    text: Optional[str] = None
    approved: Optional[bool] = None
    decision: Optional[str] = None
    data: Optional[Dict[str, Any]] = None

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for storage."""
        result: Dict[str, Any] = {}
        if self.text is not None:
            result["text"] = self.text
        if self.approved is not None:
            result["approved"] = self.approved
        if self.decision is not None:
            result["decision"] = self.decision
        if self.data is not None:
            result["data"] = self.data
        result["received_at"] = datetime.now(timezone.utc).isoformat()
        return result

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "InterruptResponse":
        """Create from dictionary."""
        return cls(
            text=data.get("text"),
            approved=data.get("approved"),
            decision=data.get("decision"),
            data=data.get("data"),
        )


@dataclass
class FlowInterrupt:
    """Unified flow interrupt.

    Attributes:
        id: Unique interrupt identifier
        kind: Type of interrupt
        request_id: Associated request
        user_id: User who triggered the interrupt
        session_id: Session context
        envelope_id: Optional envelope reference
        question: Clarification question (for clarification kind)
        message: Confirmation message (for confirmation kind)
        data: Extensible payload
        response: Response when resolved
        status: Current status
        created_at: When created
        expires_at: When it expires (optional)
        resolved_at: When resolved
        trace_id: OpenTelemetry trace ID
        span_id: OpenTelemetry span ID
    """
    id: str
    kind: InterruptKind
    request_id: str
    user_id: str
    session_id: str
    envelope_id: Optional[str] = None
    question: Optional[str] = None
    message: Optional[str] = None
    data: Dict[str, Any] = field(default_factory=dict)
    response: Optional[InterruptResponse] = None
    status: InterruptStatus = InterruptStatus.PENDING
    created_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    expires_at: Optional[datetime] = None
    resolved_at: Optional[datetime] = None
    trace_id: Optional[str] = None
    span_id: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for API responses."""
        return {
            "id": self.id,
            "kind": self.kind.value,
            "request_id": self.request_id,
            "user_id": self.user_id,
            "session_id": self.session_id,
            "envelope_id": self.envelope_id,
            "question": self.question,
            "message": self.message,
            "data": self.data,
            "response": self.response.to_dict() if self.response else None,
            "status": self.status.value,
            "created_at": self.created_at.isoformat(),
            "expires_at": self.expires_at.isoformat() if self.expires_at else None,
            "resolved_at": self.resolved_at.isoformat() if self.resolved_at else None,
            "trace_id": self.trace_id,
            "span_id": self.span_id,
        }

    def to_db_row(self) -> Dict[str, Any]:
        """Convert to database row format."""
        return {
            "id": self.id,
            "kind": self.kind.value,
            "request_id": self.request_id,
            "user_id": self.user_id,
            "session_id": self.session_id,
            "envelope_id": self.envelope_id,
            "question": self.question,
            "message": self.message,
            "data": self.data,
            "response": self.response.to_dict() if self.response else None,
            "status": self.status.value,
            "created_at": self.created_at,
            "expires_at": self.expires_at,
            "resolved_at": self.resolved_at,
            "trace_id": self.trace_id,
            "span_id": self.span_id,
        }

    @classmethod
    def from_db_row(cls, row: Dict[str, Any]) -> "FlowInterrupt":
        """Create from database row."""
        response = None
        if row.get("response"):
            response = InterruptResponse.from_dict(row["response"])

        # Parse datetime fields
        created_at = row.get("created_at")
        if isinstance(created_at, str):
            created_at = datetime.fromisoformat(created_at.replace("Z", "+00:00"))

        expires_at = row.get("expires_at")
        if isinstance(expires_at, str):
            expires_at = datetime.fromisoformat(expires_at.replace("Z", "+00:00"))

        resolved_at = row.get("resolved_at")
        if isinstance(resolved_at, str):
            resolved_at = datetime.fromisoformat(resolved_at.replace("Z", "+00:00"))

        return cls(
            id=row["id"],
            kind=InterruptKind(row["kind"]),
            request_id=row["request_id"],
            user_id=row["user_id"],
            session_id=row["session_id"],
            envelope_id=row.get("envelope_id"),
            question=row.get("question"),
            message=row.get("message"),
            data=row.get("data", {}),
            response=response,
            status=InterruptStatus(row["status"]),
            created_at=created_at or datetime.now(timezone.utc),
            expires_at=expires_at,
            resolved_at=resolved_at,
            trace_id=row.get("trace_id"),
            span_id=row.get("span_id"),
        )


@runtime_checkable
class InterruptServiceProtocol(Protocol):
    """Protocol for interrupt handling service.

    This protocol defines the contract for managing flow interrupts.
    Implementations handle persistence, expiration, and webhook notifications.
    """

    async def create_interrupt(
        self,
        kind: InterruptKind,
        request_id: str,
        user_id: str,
        session_id: str,
        envelope_id: Optional[str] = None,
        question: Optional[str] = None,
        message: Optional[str] = None,
        data: Optional[Dict[str, Any]] = None,
        ttl_seconds: Optional[int] = None,
    ) -> FlowInterrupt:
        """Create a new interrupt."""
        ...

    async def get_interrupt(self, interrupt_id: str) -> Optional[FlowInterrupt]:
        """Get an interrupt by ID."""
        ...

    async def respond(
        self,
        interrupt_id: str,
        response: InterruptResponse,
    ) -> Optional[FlowInterrupt]:
        """Respond to an interrupt."""
        ...

    async def list_pending(
        self,
        user_id: Optional[str] = None,
        session_id: Optional[str] = None,
        request_id: Optional[str] = None,
    ) -> List[FlowInterrupt]:
        """List pending interrupts."""
        ...

    async def cancel(self, interrupt_id: str) -> bool:
        """Cancel an interrupt."""
        ...


# Rate limiting types

@dataclass
class RateLimitConfig:
    """Configuration for rate limiting.

    Defines rate limits per user/endpoint combination.
    Uses sliding window algorithm for accurate limiting.
    """
    requests_per_minute: int = 60
    requests_per_hour: int = 1000
    requests_per_day: int = 10000
    burst_size: int = 10  # Max requests in a burst


@dataclass
class RateLimitResult:
    """Result of a rate limit check.

    Attributes:
        allowed: Whether the request is allowed
        exceeded: Whether a limit was exceeded
        reason: Human-readable reason if exceeded
        limit_type: Which limit was exceeded (minute, hour, day)
        current_count: Current request count in the window
        limit: The limit that was checked
        retry_after_seconds: Seconds until the request would be allowed
        remaining: Remaining requests in the current window
    """
    allowed: bool
    exceeded: bool = False
    reason: Optional[str] = None
    limit_type: Optional[str] = None
    current_count: int = 0
    limit: int = 0
    retry_after_seconds: float = 0.0
    remaining: int = 0

    @staticmethod
    def ok(remaining: int = 0) -> "RateLimitResult":
        """Create an 'allowed' result."""
        return RateLimitResult(allowed=True, remaining=remaining)

    @staticmethod
    def exceeded_limit(
        limit_type: str,
        current: int,
        limit: int,
        retry_after: float = 0.0,
    ) -> "RateLimitResult":
        """Create an 'exceeded' result."""
        return RateLimitResult(
            allowed=False,
            exceeded=True,
            reason=f"Rate limit exceeded: {current}/{limit} requests per {limit_type}",
            limit_type=limit_type,
            current_count=current,
            limit=limit,
            retry_after_seconds=retry_after,
            remaining=0,
        )


@runtime_checkable
class RateLimiterProtocol(Protocol):
    """Protocol for rate limiting service."""

    def check_rate_limit(
        self,
        user_id: str,
        endpoint: str,
    ) -> RateLimitResult:
        """Check if a request is allowed under rate limits."""
        ...

    def set_user_limits(
        self,
        user_id: str,
        config: RateLimitConfig,
    ) -> None:
        """Set rate limits for a specific user."""
        ...

    def get_user_limits(self, user_id: str) -> Optional[RateLimitConfig]:
        """Get rate limits for a specific user."""
        ...


__all__ = [
    # Interrupt types
    "InterruptKind",
    "InterruptStatus",
    "InterruptResponse",
    "FlowInterrupt",
    "InterruptServiceProtocol",
    # Rate limit types
    "RateLimitConfig",
    "RateLimitResult",
    "RateLimiterProtocol",
]
