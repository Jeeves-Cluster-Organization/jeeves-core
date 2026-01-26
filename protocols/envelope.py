"""Envelope types - mirrors Go coreengine/envelope."""

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional, TYPE_CHECKING

from protocols.enums import TerminalReason
from protocols.protocols import RequestContext

if TYPE_CHECKING:
    from protocols.interrupts import FlowInterrupt


@dataclass
class ProcessingRecord:
    """Record of agent processing step."""
    agent: str
    stage_order: int
    started_at: datetime
    completed_at: Optional[datetime] = None
    duration_ms: int = 0
    status: str = "running"  # running, success, error, skipped
    error: Optional[str] = None
    llm_calls: int = 0


@dataclass
class PipelineEvent:
    """Minimal stable event for streaming.

    Fields:
    - type: Event type (token, stage, error, done)
    - stage: Agent name or "__end__"
    - data: Event-specific payload
    - debug: Whether this is a debug-only event (not authoritative)
    """
    type: str
    stage: str
    data: Dict[str, Any]
    debug: bool = False


@dataclass
class Envelope:
    """Envelope with dynamic output slots.

    This is a Python mirror of Go's Envelope.
    Primary state container for pipeline execution.
    """
    # Identification
    request_context: RequestContext
    envelope_id: str = ""
    request_id: str = ""
    user_id: str = ""
    session_id: str = ""

    # Input
    raw_input: str = ""
    received_at: Optional[datetime] = None

    # Dynamic outputs
    outputs: Dict[str, Dict[str, Any]] = field(default_factory=dict)

    # Pipeline state
    current_stage: str = "start"
    stage_order: List[str] = field(default_factory=list)
    iteration: int = 0
    max_iterations: int = 3

    # Bounds
    llm_call_count: int = 0
    max_llm_calls: int = 10
    agent_hop_count: int = 0
    max_agent_hops: int = 21
    terminal_reason: Optional[TerminalReason] = None

    # Control flow
    terminated: bool = False
    termination_reason: Optional[str] = None

    # Unified Interrupt Handling (matches Go Envelope)
    interrupt_pending: bool = False
    interrupt: Optional["FlowInterrupt"] = None

    # Parallel execution state (matches Go Envelope)
    active_stages: Dict[str, bool] = field(default_factory=dict)
    completed_stage_set: Dict[str, bool] = field(default_factory=dict)
    failed_stages: Dict[str, str] = field(default_factory=dict)
    parallel_mode: bool = False

    # Multi-stage
    completed_stages: List[Dict[str, Any]] = field(default_factory=list)
    current_stage_number: int = 1
    max_stages: int = 5
    all_goals: List[str] = field(default_factory=list)
    remaining_goals: List[str] = field(default_factory=list)
    goal_completion_status: Dict[str, str] = field(default_factory=dict)

    # Retry
    prior_plans: List[Dict[str, Any]] = field(default_factory=list)
    loop_feedback: List[str] = field(default_factory=list)

    # Audit
    processing_history: List[ProcessingRecord] = field(default_factory=list)
    errors: List[Dict[str, Any]] = field(default_factory=list)

    # Timing
    created_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None

    # Metadata
    metadata: Dict[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        if self.request_context is None:
            raise ValueError("request_context is required for Envelope")

        # Normalize empty strings to None for comparison
        ctx = self.request_context
        ctx_request_id = ctx.request_id
        ctx_user_id = ctx.user_id
        ctx_session_id = ctx.session_id

        if self.request_id and self.request_id != ctx_request_id:
            raise ValueError("request_id does not match request_context.request_id")
        if self.user_id and (ctx_user_id is None or self.user_id != ctx_user_id):
            raise ValueError("user_id does not match request_context.user_id")
        if self.session_id and (ctx_session_id is None or self.session_id != ctx_session_id):
            raise ValueError("session_id does not match request_context.session_id")

        # Fill missing fields from context
        if not self.request_id:
            self.request_id = ctx_request_id
        if not self.user_id and ctx_user_id:
            self.user_id = ctx_user_id
        if not self.session_id and ctx_session_id:
            self.session_id = ctx_session_id

    @classmethod
    def _extract_request_context(cls, data: Dict[str, Any]) -> RequestContext:
        """Extract and parse request_context from dict data."""
        ctx_data = data.get("request_context")
        if ctx_data is None:
            raise ValueError("request_context missing in envelope data")
        if isinstance(ctx_data, RequestContext):
            return ctx_data
        if isinstance(ctx_data, dict):
            return RequestContext(**ctx_data)
        raise TypeError("request_context must be a dict or RequestContext")

    @classmethod
    def _validate_identifiers(cls, data: Dict[str, Any], ctx: RequestContext) -> tuple[str, str, str]:
        """Validate identifiers against request_context and return them."""
        data_request_id = data.get("request_id") or ""
        data_user_id = data.get("user_id") or ""
        data_session_id = data.get("session_id") or ""

        if data_request_id and data_request_id != ctx.request_id:
            raise ValueError("request_id does not match request_context.request_id")
        if data_user_id and (ctx.user_id is None or data_user_id != ctx.user_id):
            raise ValueError("user_id does not match request_context.user_id")
        if data_session_id and (ctx.session_id is None or data_session_id != ctx.session_id):
            raise ValueError("session_id does not match request_context.session_id")

        return data_request_id, data_user_id, data_session_id

    @classmethod
    def _convert_field_value(cls, key: str, value: Any) -> Any:
        """Convert field value based on field type (enum, object, etc)."""
        if key == "terminal_reason" and value is not None:
            if isinstance(value, str):
                return TerminalReason(value)
        elif key == "interrupt" and value is not None:
            if isinstance(value, dict):
                from protocols.interrupts import FlowInterrupt
                return FlowInterrupt.from_db_row(value)
        return value

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Envelope":
        """Create envelope from dictionary (Go JSON response)."""
        # Extract and validate core fields
        ctx = cls._extract_request_context(data)
        request_id, user_id, session_id = cls._validate_identifiers(data, ctx)

        # Create envelope with validated identifiers
        env = cls(
            request_context=ctx,
            request_id=request_id,
            user_id=user_id,
            session_id=session_id,
        )

        # Apply remaining fields with type conversions
        for key, value in data.items():
            if hasattr(env, key):
                converted_value = cls._convert_field_value(key, value)
                setattr(env, key, converted_value)

        return env

    def initialize_goals(self, goals: List[str]) -> None:
        """Initialize goals for multi-stage execution.

        Args:
            goals: List of goal strings extracted from intent
        """
        self.all_goals = list(goals)
        self.remaining_goals = list(goals)
        self.goal_completion_status = {goal: "pending" for goal in goals}

    def mark_goal_complete(self, goal: str) -> None:
        """Mark a goal as complete.

        Args:
            goal: The goal to mark complete
        """
        if goal in self.goal_completion_status:
            self.goal_completion_status[goal] = "complete"
        if goal in self.remaining_goals:
            self.remaining_goals.remove(goal)

    def advance_stage(self) -> None:
        """Advance to the next stage."""
        self.current_stage_number += 1

    def get_stage_context(self) -> Dict[str, Any]:
        """Get context for the current stage.

        Returns:
            Dictionary with stage context
        """
        return {
            "current_stage_number": self.current_stage_number,
            "max_stages": self.max_stages,
            "all_goals": self.all_goals,
            "remaining_goals": self.remaining_goals,
            "goal_completion_status": self.goal_completion_status,
            "completed_stages": self.completed_stages,
        }

    def is_stuck(self, min_progress_stages: int = 2) -> bool:
        """Check if pipeline is stuck (no progress after N stages).

        Args:
            min_progress_stages: Minimum stages before checking for stuck state

        Returns:
            True if stuck (no goals completed after min_progress_stages)
        """
        if self.current_stage_number < min_progress_stages:
            return False
        completed_count = sum(
            1 for status in self.goal_completion_status.values()
            if status == "complete"
        )
        return completed_count == 0

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for Go JSON input."""
        return {
            "request_context": self.request_context.to_dict(),
            "envelope_id": self.envelope_id,
            "request_id": self.request_id,
            "user_id": self.user_id,
            "session_id": self.session_id,
            "raw_input": self.raw_input,
            "outputs": self.outputs,
            "current_stage": self.current_stage,
            "stage_order": self.stage_order,
            "iteration": self.iteration,
            "llm_call_count": self.llm_call_count,
            "agent_hop_count": self.agent_hop_count,
            "terminal_reason": self.terminal_reason.value if self.terminal_reason else None,
            "terminated": self.terminated,
            "termination_reason": self.termination_reason,
            # Unified interrupt fields
            "interrupt_pending": self.interrupt_pending,
            "interrupt": self.interrupt.to_dict() if self.interrupt else None,
            # Parallel execution state
            "active_stages": self.active_stages,
            "completed_stage_set": self.completed_stage_set,
            "failed_stages": self.failed_stages,
            "parallel_mode": self.parallel_mode,
            # Multi-stage
            "current_stage_number": self.current_stage_number,
            "max_stages": self.max_stages,
            "all_goals": self.all_goals,
            "remaining_goals": self.remaining_goals,
            "goal_completion_status": self.goal_completion_status,
            "errors": self.errors,
            "metadata": self.metadata,
        }

    def to_state_dict(self) -> Dict[str, Any]:
        """Convert to dictionary with ALL fields for complete state serialization.

        Similar to to_dict() but includes ALL fields for checkpoint adapters
        and distributed task coordination. Datetime fields are serialized
        with .isoformat() and processing_history items are fully serialized.

        Returns:
            Dictionary with complete envelope state.
        """
        # Serialize processing history records
        serialized_history = []
        for record in self.processing_history:
            serialized_history.append({
                "agent": record.agent,
                "stage_order": record.stage_order,
                "started_at": record.started_at.isoformat() if record.started_at else None,
                "completed_at": record.completed_at.isoformat() if record.completed_at else None,
                "duration_ms": record.duration_ms,
                "status": record.status,
                "error": record.error,
                "llm_calls": record.llm_calls,
            })

        return {
            # Identification
            "request_context": self.request_context.to_dict(),
            "envelope_id": self.envelope_id,
            "request_id": self.request_id,
            "user_id": self.user_id,
            "session_id": self.session_id,
            # Input
            "raw_input": self.raw_input,
            "received_at": self.received_at.isoformat() if self.received_at else None,
            # Dynamic outputs
            "outputs": self.outputs,
            # Pipeline state
            "current_stage": self.current_stage,
            "stage_order": self.stage_order,
            "iteration": self.iteration,
            "max_iterations": self.max_iterations,
            # Bounds
            "llm_call_count": self.llm_call_count,
            "max_llm_calls": self.max_llm_calls,
            "agent_hop_count": self.agent_hop_count,
            "max_agent_hops": self.max_agent_hops,
            "terminal_reason": self.terminal_reason.value if self.terminal_reason else None,
            # Control flow
            "terminated": self.terminated,
            "termination_reason": self.termination_reason,
            # Unified interrupt fields
            "interrupt_pending": self.interrupt_pending,
            "interrupt": self.interrupt.to_dict() if self.interrupt else None,
            # Parallel execution state
            "active_stages": self.active_stages,
            "completed_stage_set": self.completed_stage_set,
            "failed_stages": self.failed_stages,
            "parallel_mode": self.parallel_mode,
            # Multi-stage
            "completed_stages": self.completed_stages,
            "current_stage_number": self.current_stage_number,
            "max_stages": self.max_stages,
            "all_goals": self.all_goals,
            "remaining_goals": self.remaining_goals,
            "goal_completion_status": self.goal_completion_status,
            # Retry
            "prior_plans": self.prior_plans,
            "loop_feedback": self.loop_feedback,
            # Audit
            "processing_history": serialized_history,
            "errors": self.errors,
            # Timing
            "created_at": self.created_at.isoformat() if self.created_at else None,
            "completed_at": self.completed_at.isoformat() if self.completed_at else None,
            # Metadata
            "metadata": self.metadata,
        }
