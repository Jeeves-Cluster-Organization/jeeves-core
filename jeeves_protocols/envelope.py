"""Envelope types - mirrors Go coreengine/envelope."""

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional, TYPE_CHECKING

from jeeves_protocols.enums import TerminalReason

if TYPE_CHECKING:
    from jeeves_protocols.interrupts import FlowInterrupt


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
class Envelope:
    """Envelope with dynamic output slots.

    This is a Python mirror of Go's Envelope.
    Primary state container for pipeline execution.
    """
    # Identification
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

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Envelope":
        """Create envelope from dictionary (Go JSON response)."""
        from jeeves_protocols.interrupts import FlowInterrupt

        env = cls()
        for key, value in data.items():
            if hasattr(env, key):
                # Handle enum conversion for terminal_reason
                if key == "terminal_reason" and value is not None:
                    if isinstance(value, str):
                        value = TerminalReason(value)
                # Handle FlowInterrupt conversion
                elif key == "interrupt" and value is not None:
                    if isinstance(value, dict):
                        value = FlowInterrupt.from_db_row(value)
                setattr(env, key, value)
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
