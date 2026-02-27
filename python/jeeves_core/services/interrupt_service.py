"""Concrete InterruptService backed by KernelClient IPC."""

from __future__ import annotations

import logging
from datetime import datetime
from typing import Any, Dict, List, Optional, TYPE_CHECKING

from jeeves_core.protocols import (
    FlowInterrupt,
    InterruptKind,
    InterruptResponse,
    InterruptStatus,
)

logger = logging.getLogger(__name__)

if TYPE_CHECKING:
    from jeeves_core.kernel_client import KernelClient


class KernelInterruptService:
    """Interrupt service that delegates to the Rust kernel via IPC.

    Implements the methods expected by the gateway interrupts router.
    """

    def __init__(self, kernel_client: "KernelClient") -> None:
        self._client = kernel_client

    async def get_interrupt(self, interrupt_id: str) -> Optional[FlowInterrupt]:
        """Get an interrupt by ID."""
        try:
            data = await self._client.get_interrupt(interrupt_id)
            return _dict_to_flow_interrupt(data)
        except Exception:
            return None

    async def respond(
        self,
        interrupt_id: str,
        response: InterruptResponse,
        user_id: str,
    ) -> Optional[FlowInterrupt]:
        """Resolve an interrupt with a response. Returns updated interrupt."""
        response_dict = response.to_dict()
        # Add received_at for Rust deserialization
        response_dict["received_at"] = datetime.utcnow().isoformat() + "Z"

        success = await self._client.resolve_interrupt(
            interrupt_id=interrupt_id,
            response=response_dict,
            user_id=user_id,
        )
        if not success:
            return None

        # Fetch updated interrupt
        return await self.get_interrupt(interrupt_id)

    async def get_pending_for_session(
        self,
        session_id: str,
        kinds: Optional[List[InterruptKind]] = None,
    ) -> List[FlowInterrupt]:
        """Get pending interrupts for a session."""
        kind_strings = None
        if kinds:
            kind_strings = [k.value for k in kinds]

        raw_list = await self._client.get_pending_interrupts_for_session(
            session_id=session_id,
            kinds=kind_strings,
        )
        return [_dict_to_flow_interrupt(d) for d in raw_list]

    async def cancel(
        self,
        interrupt_id: str,
        reason: Optional[str] = None,
    ) -> bool:
        """Cancel a pending interrupt."""
        return await self._client.cancel_interrupt(
            interrupt_id=interrupt_id,
            reason=reason or "cancelled",
        )

    async def create_interrupt(
        self,
        kind: InterruptKind,
        envelope_id: str,
        question: str = "",
        message: str = "",
        data: Optional[Dict[str, Any]] = None,
        request_id: str = "",
        user_id: str = "",
        session_id: str = "",
    ) -> FlowInterrupt:
        """Create a new interrupt."""
        raw = await self._client.create_interrupt(
            kind=kind.value,
            request_id=request_id,
            user_id=user_id,
            session_id=session_id,
            envelope_id=envelope_id,
            question=question,
            message=message,
            data=data,
        )
        return _dict_to_flow_interrupt(raw)


def _dict_to_flow_interrupt(d: Dict[str, Any]) -> FlowInterrupt:
    """Convert a Rust KernelInterrupt dict to Python FlowInterrupt."""
    # KernelInterrupt uses #[serde(flatten)] on flow_interrupt, so fields
    # are at the top level of the dict.
    kind_str = d.get("kind", "")
    try:
        kind = InterruptKind(kind_str)
    except ValueError:
        logger.warning("unknown_interrupt_kind", extra={"value": kind_str})
        kind = InterruptKind.SYSTEM_ERROR  # safe fallback for unknown kinds

    status_str = d.get("status", "pending")
    try:
        int_status = InterruptStatus(status_str)
    except ValueError:
        int_status = InterruptStatus.PENDING

    # Parse datetime fields
    created_at = _parse_dt(d.get("created_at"))
    expires_at = _parse_dt(d.get("expires_at"))
    resolved_at = _parse_dt(d.get("resolved_at"))

    # Parse response if present
    resp_data = d.get("response")
    response = None
    if resp_data and isinstance(resp_data, dict):
        response = InterruptResponse(
            text=resp_data.get("text", "") or "",
            approved=resp_data.get("approved", False) or False,
            decision=resp_data.get("decision", "") or "",
            data=resp_data.get("data"),
            resolved_at=_parse_dt(resp_data.get("received_at")),
        )

    return FlowInterrupt(
        id=d.get("id", ""),
        kind=kind,
        request_id=d.get("request_id", ""),
        user_id=d.get("user_id", ""),
        session_id=d.get("session_id", ""),
        envelope_id=d.get("envelope_id", ""),
        question=d.get("question", "") or "",
        message=d.get("message", "") or "",
        data=d.get("data"),
        response=response,
        status=int_status,
        created_at=created_at,
        expires_at=expires_at,
        resolved_at=resolved_at,
        trace_id=d.get("trace_id", "") or "",
        span_id=d.get("span_id", "") or "",
    )


def _parse_dt(val: Any) -> Optional[datetime]:
    """Parse an ISO datetime string or return None."""
    if val is None:
        return None
    if isinstance(val, datetime):
        return val
    if isinstance(val, str):
        try:
            return datetime.fromisoformat(val.rstrip("Z"))
        except (ValueError, TypeError):
            return None
    return None
