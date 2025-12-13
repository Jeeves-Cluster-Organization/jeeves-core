"""Unified Interrupt Service - Single service for all interrupt types.

This service replaces:
- ConfirmationManager
- ConfirmationCoordinator
- Clarification methods in SessionStateService
- CriticInterruptHandler

All interrupt types are handled through this single service with config-driven behavior.

Usage:
    service = InterruptService(db, logger, webhook_service, otel_adapter)

    # Create any type of interrupt
    interrupt = await service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        request_id="req-123",
        user_id="user-1",
        session_id="sess-1",
        question="What file would you like to analyze?",
    )

    # Respond to interrupt
    resolved = await service.respond(
        interrupt_id=interrupt.id,
        response=InterruptResponse(text="The main.py file"),
    )
"""

from __future__ import annotations

import asyncio
import uuid
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from typing import Any, Dict, List, Optional, Protocol, runtime_checkable

from jeeves_protocols import (
    LoggerProtocol,
    InterruptKind,
    InterruptStatus,
    InterruptResponse,
    FlowInterrupt,
)


@dataclass
class InterruptConfig:
    """Configuration for a specific interrupt kind.

    Attributes:
        default_ttl: Default time-to-live for interrupts of this kind
        auto_expire: Whether to auto-expire pending interrupts
        require_response: Whether a response is required to resolve
        webhook_event: Event name to emit via webhook
        allowed_responses: List of allowed response types
    """
    default_ttl: Optional[timedelta] = None
    auto_expire: bool = True
    require_response: bool = True
    webhook_event: str = "interrupt.created"
    allowed_responses: List[str] = field(default_factory=list)


# Default configurations per interrupt kind
DEFAULT_INTERRUPT_CONFIGS: Dict[InterruptKind, InterruptConfig] = {
    InterruptKind.CLARIFICATION: InterruptConfig(
        default_ttl=timedelta(hours=24),
        webhook_event="interrupt.clarification_needed",
        allowed_responses=["text"],
    ),
    InterruptKind.CONFIRMATION: InterruptConfig(
        default_ttl=timedelta(hours=1),
        webhook_event="interrupt.confirmation_needed",
        allowed_responses=["approved"],
    ),
    InterruptKind.CRITIC_REVIEW: InterruptConfig(
        default_ttl=timedelta(minutes=30),
        webhook_event="interrupt.critic_review",
        allowed_responses=["decision"],
    ),
    InterruptKind.CHECKPOINT: InterruptConfig(
        default_ttl=None,  # No expiry for checkpoints
        auto_expire=False,
        require_response=False,
        webhook_event="interrupt.checkpoint",
    ),
    InterruptKind.RESOURCE_EXHAUSTED: InterruptConfig(
        default_ttl=timedelta(minutes=5),
        webhook_event="interrupt.resource_exhausted",
        require_response=False,
    ),
    InterruptKind.TIMEOUT: InterruptConfig(
        default_ttl=timedelta(minutes=5),
        webhook_event="interrupt.timeout",
        require_response=False,
    ),
    InterruptKind.SYSTEM_ERROR: InterruptConfig(
        default_ttl=timedelta(hours=1),
        webhook_event="interrupt.system_error",
        require_response=False,
    ),
}


@runtime_checkable
class DatabaseProtocol(Protocol):
    """Protocol for database operations."""

    async def insert(self, table: str, data: Dict[str, Any]) -> str:
        """Insert a row and return the ID."""
        ...

    async def update(
        self, table: str, data: Dict[str, Any], where: Dict[str, Any]
    ) -> int:
        """Update rows matching where clause, return count."""
        ...

    async def fetch_one(self, query: str, params: tuple = ()) -> Optional[Dict[str, Any]]:
        """Fetch a single row."""
        ...

    async def fetch_all(self, query: str, params: tuple = ()) -> List[Dict[str, Any]]:
        """Fetch multiple rows."""
        ...


@runtime_checkable
class WebhookServiceProtocol(Protocol):
    """Protocol for webhook service."""

    async def emit_event(
        self, event_type: str, data: Dict[str, Any], request_id: Optional[str] = None
    ) -> int:
        """Emit a webhook event."""
        ...


@runtime_checkable
class OTelAdapterProtocol(Protocol):
    """Protocol for OpenTelemetry adapter."""

    def record_event(
        self, name: str, request_id: str, attributes: Optional[Dict[str, Any]] = None
    ) -> None:
        """Record a span event."""
        ...

    def get_trace_context(self, request_id: str) -> Optional[Any]:
        """Get current trace context."""
        ...


class InterruptService:
    """Unified service for all interrupt types.

    Handles creation, querying, and resolution of flow interrupts with:
    - Config-driven behavior per interrupt kind
    - Webhook event emission
    - OpenTelemetry tracing integration
    - Database persistence
    """

    def __init__(
        self,
        db: Optional[DatabaseProtocol] = None,
        logger: Optional[LoggerProtocol] = None,
        webhook_service: Optional[WebhookServiceProtocol] = None,
        otel_adapter: Optional[OTelAdapterProtocol] = None,
        configs: Optional[Dict[InterruptKind, InterruptConfig]] = None,
    ):
        """Initialize interrupt service.

        Args:
            db: Database for persistence
            logger: Logger for structured logging
            webhook_service: Webhook service for event emission
            otel_adapter: OpenTelemetry adapter for tracing
            configs: Optional custom configs per interrupt kind
        """
        self._db = db
        self._logger = logger
        self._webhook_service = webhook_service
        self._otel_adapter = otel_adapter
        self._configs = {**DEFAULT_INTERRUPT_CONFIGS, **(configs or {})}

        # In-memory fallback when DB not available
        # Protected by _memory_lock for thread safety in concurrent scenarios
        self._memory_store: Dict[str, FlowInterrupt] = {}
        self._memory_lock = asyncio.Lock()

    def get_config(self, kind: InterruptKind) -> InterruptConfig:
        """Get configuration for an interrupt kind."""
        return self._configs.get(kind, InterruptConfig())

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
        ttl: Optional[timedelta] = None,
    ) -> FlowInterrupt:
        """Create a new interrupt.

        Args:
            kind: Type of interrupt
            request_id: Associated request ID
            user_id: User who triggered the interrupt
            session_id: Session context
            envelope_id: Optional envelope reference
            question: Clarification question (for clarification kind)
            message: Confirmation message (for confirmation kind)
            data: Additional extensible data
            ttl: Time-to-live (uses default from config if not provided)

        Returns:
            Created FlowInterrupt
        """
        config = self.get_config(kind)

        # Determine expiration
        expires_at = None
        effective_ttl = ttl or config.default_ttl
        if effective_ttl:
            expires_at = datetime.now(timezone.utc) + effective_ttl

        # Get trace context if available
        trace_id = None
        span_id = None
        if self._otel_adapter:
            trace_ctx = self._otel_adapter.get_trace_context(request_id)
            if trace_ctx:
                trace_id = getattr(trace_ctx, "trace_id", None)
                span_id = getattr(trace_ctx, "span_id", None)

        interrupt = FlowInterrupt(
            id=str(uuid.uuid4()),
            kind=kind,
            request_id=request_id,
            user_id=user_id,
            session_id=session_id,
            envelope_id=envelope_id,
            question=question,
            message=message,
            data=data or {},
            expires_at=expires_at,
            trace_id=trace_id,
            span_id=span_id,
        )

        # Persist
        if self._db:
            await self._db.insert("flow_interrupts", interrupt.to_db_row())
        else:
            async with self._memory_lock:
                self._memory_store[interrupt.id] = interrupt

        # Log
        if self._logger:
            self._logger.info(
                "interrupt_created",
                interrupt_id=interrupt.id,
                kind=kind.value,
                request_id=request_id,
                user_id=user_id,
                session_id=session_id,
            )

        # Record OTel event
        if self._otel_adapter:
            self._otel_adapter.record_event(
                f"interrupt.{kind.value}.created",
                request_id,
                {
                    "interrupt.id": interrupt.id,
                    "interrupt.kind": kind.value,
                },
            )

        # Emit webhook
        if self._webhook_service:
            await self._webhook_service.emit_event(
                config.webhook_event,
                interrupt.to_dict(),
                request_id=request_id,
            )

        return interrupt

    async def get_interrupt(self, interrupt_id: str) -> Optional[FlowInterrupt]:
        """Get an interrupt by ID.

        Args:
            interrupt_id: Interrupt ID

        Returns:
            FlowInterrupt or None if not found
        """
        if self._db:
            row = await self._db.fetch_one(
                "SELECT * FROM flow_interrupts WHERE id = $1",
                (interrupt_id,),
            )
            if row:
                return FlowInterrupt.from_db_row(row)
            return None
        else:
            async with self._memory_lock:
                return self._memory_store.get(interrupt_id)

    async def get_pending_for_request(
        self, request_id: str
    ) -> Optional[FlowInterrupt]:
        """Get pending interrupt for a request.

        Args:
            request_id: Request ID

        Returns:
            Pending FlowInterrupt or None
        """
        if self._db:
            row = await self._db.fetch_one(
                """
                SELECT * FROM flow_interrupts
                WHERE request_id = $1 AND status = 'pending'
                ORDER BY created_at DESC
                LIMIT 1
                """,
                (request_id,),
            )
            if row:
                return FlowInterrupt.from_db_row(row)
            return None
        else:
            async with self._memory_lock:
                for interrupt in self._memory_store.values():
                    if (
                        interrupt.request_id == request_id
                        and interrupt.status == InterruptStatus.PENDING
                    ):
                        return interrupt
                return None

    async def get_pending_for_session(
        self, session_id: str, kinds: Optional[List[InterruptKind]] = None
    ) -> List[FlowInterrupt]:
        """Get all pending interrupts for a session.

        Args:
            session_id: Session ID
            kinds: Optional filter by interrupt kinds

        Returns:
            List of pending FlowInterrupts
        """
        if self._db:
            if kinds:
                kind_values = [k.value for k in kinds]
                rows = await self._db.fetch_all(
                    """
                    SELECT * FROM flow_interrupts
                    WHERE session_id = $1 AND status = 'pending' AND kind = ANY($2)
                    ORDER BY created_at DESC
                    """,
                    (session_id, kind_values),
                )
            else:
                rows = await self._db.fetch_all(
                    """
                    SELECT * FROM flow_interrupts
                    WHERE session_id = $1 AND status = 'pending'
                    ORDER BY created_at DESC
                    """,
                    (session_id,),
                )
            return [FlowInterrupt.from_db_row(row) for row in rows]
        else:
            async with self._memory_lock:
                result = []
                for interrupt in self._memory_store.values():
                    if (
                        interrupt.session_id == session_id
                        and interrupt.status == InterruptStatus.PENDING
                    ):
                        if kinds is None or interrupt.kind in kinds:
                            result.append(interrupt)
                return sorted(result, key=lambda i: i.created_at, reverse=True)

    async def respond(
        self,
        interrupt_id: str,
        response: InterruptResponse,
        user_id: Optional[str] = None,
    ) -> Optional[FlowInterrupt]:
        """Respond to an interrupt.

        Args:
            interrupt_id: Interrupt ID
            response: Response data
            user_id: Optional user ID for validation

        Returns:
            Updated FlowInterrupt or None if not found/invalid
        """
        interrupt = await self.get_interrupt(interrupt_id)
        if not interrupt:
            if self._logger:
                self._logger.warning(
                    "interrupt_not_found",
                    interrupt_id=interrupt_id,
                )
            return None

        if interrupt.status != InterruptStatus.PENDING:
            if self._logger:
                self._logger.warning(
                    "interrupt_not_pending",
                    interrupt_id=interrupt_id,
                    status=interrupt.status.value,
                )
            return None

        # Validate user if provided
        if user_id and interrupt.user_id != user_id:
            if self._logger:
                self._logger.warning(
                    "interrupt_user_mismatch",
                    interrupt_id=interrupt_id,
                    expected_user=interrupt.user_id,
                    actual_user=user_id,
                )
            return None

        # Update interrupt
        now = datetime.now(timezone.utc)
        interrupt.response = response
        interrupt.status = InterruptStatus.RESOLVED
        interrupt.resolved_at = now

        # Persist
        if self._db:
            await self._db.update(
                "flow_interrupts",
                {
                    "response": response.to_dict(),
                    "status": InterruptStatus.RESOLVED.value,
                    "resolved_at": now,
                },
                {"id": interrupt_id},
            )
        else:
            async with self._memory_lock:
                self._memory_store[interrupt_id] = interrupt

        # Log
        if self._logger:
            self._logger.info(
                "interrupt_resolved",
                interrupt_id=interrupt_id,
                kind=interrupt.kind.value,
                request_id=interrupt.request_id,
            )

        # Record OTel event
        if self._otel_adapter:
            self._otel_adapter.record_event(
                f"interrupt.{interrupt.kind.value}.resolved",
                interrupt.request_id,
                {
                    "interrupt.id": interrupt_id,
                    "interrupt.kind": interrupt.kind.value,
                },
            )

        # Emit webhook
        if self._webhook_service:
            await self._webhook_service.emit_event(
                "interrupt.resolved",
                interrupt.to_dict(),
                request_id=interrupt.request_id,
            )

        return interrupt

    async def cancel(
        self, interrupt_id: str, reason: Optional[str] = None
    ) -> Optional[FlowInterrupt]:
        """Cancel an interrupt.

        Args:
            interrupt_id: Interrupt ID
            reason: Optional cancellation reason

        Returns:
            Updated FlowInterrupt or None if not found
        """
        interrupt = await self.get_interrupt(interrupt_id)
        if not interrupt:
            return None

        if interrupt.status != InterruptStatus.PENDING:
            return None

        # Update status
        interrupt.status = InterruptStatus.CANCELLED
        if reason:
            interrupt.data["cancel_reason"] = reason

        # Persist
        if self._db:
            await self._db.update(
                "flow_interrupts",
                {
                    "status": InterruptStatus.CANCELLED.value,
                    "data": interrupt.data,
                },
                {"id": interrupt_id},
            )
        else:
            async with self._memory_lock:
                self._memory_store[interrupt_id] = interrupt

        if self._logger:
            self._logger.info(
                "interrupt_cancelled",
                interrupt_id=interrupt_id,
                reason=reason,
            )

        # Emit webhook
        if self._webhook_service:
            await self._webhook_service.emit_event(
                "interrupt.cancelled",
                interrupt.to_dict(),
                request_id=interrupt.request_id,
            )

        return interrupt

    async def expire_pending(self) -> int:
        """Expire all pending interrupts that have passed their expiry time.

        Returns:
            Number of interrupts expired
        """
        now = datetime.now(timezone.utc)

        if self._db:
            # Use database query to expire in bulk
            rows = await self._db.fetch_all(
                """
                UPDATE flow_interrupts
                SET status = 'expired'
                WHERE status = 'pending'
                AND expires_at IS NOT NULL
                AND expires_at < $1
                RETURNING id, request_id, kind
                """,
                (now,),
            )
            count = len(rows)

            # Emit webhooks for each expired interrupt
            if self._webhook_service:
                for row in rows:
                    await self._webhook_service.emit_event(
                        "interrupt.expired",
                        {
                            "id": row["id"],
                            "kind": row["kind"],
                            "request_id": row["request_id"],
                        },
                        request_id=row["request_id"],
                    )
        else:
            count = 0
            async with self._memory_lock:
                for interrupt in list(self._memory_store.values()):
                    if (
                        interrupt.status == InterruptStatus.PENDING
                        and interrupt.expires_at
                        and interrupt.expires_at < now
                    ):
                        interrupt.status = InterruptStatus.EXPIRED
                        count += 1

            # Emit webhooks outside the lock to avoid holding it during I/O
            if self._webhook_service and count > 0:
                async with self._memory_lock:
                    for interrupt in self._memory_store.values():
                        if interrupt.status == InterruptStatus.EXPIRED:
                            await self._webhook_service.emit_event(
                                "interrupt.expired",
                                interrupt.to_dict(),
                                request_id=interrupt.request_id,
                            )

        if self._logger and count > 0:
            self._logger.info("interrupts_expired", count=count)

        return count

    # =========================================================================
    # Convenience Methods for Common Interrupt Types
    # =========================================================================

    async def create_clarification(
        self,
        request_id: str,
        user_id: str,
        session_id: str,
        question: str,
        envelope_id: Optional[str] = None,
        context: Optional[Dict[str, Any]] = None,
    ) -> FlowInterrupt:
        """Create a clarification interrupt.

        Args:
            request_id: Request ID
            user_id: User ID
            session_id: Session ID
            question: Clarification question
            envelope_id: Optional envelope ID
            context: Optional additional context

        Returns:
            Created FlowInterrupt
        """
        return await self.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id=request_id,
            user_id=user_id,
            session_id=session_id,
            envelope_id=envelope_id,
            question=question,
            data=context or {},
        )

    async def create_confirmation(
        self,
        request_id: str,
        user_id: str,
        session_id: str,
        message: str,
        envelope_id: Optional[str] = None,
        action_data: Optional[Dict[str, Any]] = None,
    ) -> FlowInterrupt:
        """Create a confirmation interrupt.

        Args:
            request_id: Request ID
            user_id: User ID
            session_id: Session ID
            message: Confirmation message
            envelope_id: Optional envelope ID
            action_data: Optional data about the action requiring confirmation

        Returns:
            Created FlowInterrupt
        """
        return await self.create_interrupt(
            kind=InterruptKind.CONFIRMATION,
            request_id=request_id,
            user_id=user_id,
            session_id=session_id,
            envelope_id=envelope_id,
            message=message,
            data=action_data or {},
        )

    async def create_resource_exhausted(
        self,
        request_id: str,
        user_id: str,
        session_id: str,
        resource_type: str,
        retry_after_seconds: float,
        envelope_id: Optional[str] = None,
    ) -> FlowInterrupt:
        """Create a resource exhausted interrupt (e.g., rate limit hit).

        Args:
            request_id: Request ID
            user_id: User ID
            session_id: Session ID
            resource_type: Type of resource exhausted (e.g., "rate_limit")
            retry_after_seconds: Seconds until retry is allowed
            envelope_id: Optional envelope ID

        Returns:
            Created FlowInterrupt
        """
        return await self.create_interrupt(
            kind=InterruptKind.RESOURCE_EXHAUSTED,
            request_id=request_id,
            user_id=user_id,
            session_id=session_id,
            envelope_id=envelope_id,
            message=f"Resource exhausted: {resource_type}",
            data={
                "resource_type": resource_type,
                "retry_after_seconds": retry_after_seconds,
            },
        )
