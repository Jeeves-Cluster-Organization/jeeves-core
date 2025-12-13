"""Envelope types - mirrors Go coreengine/envelope."""

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional

from jeeves_protocols.core import TerminalReason


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
class GenericEnvelope:
    """Envelope with dynamic output slots.

    This is a Python mirror of Go's GenericEnvelope.
    Use GoClient to create/manipulate envelopes via Go runtime.
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

    # Unified Interrupt Handling (matches Go GenericEnvelope)
    interrupt_pending: bool = False
    interrupt: Optional[Dict[str, Any]] = None

    # DAG execution state (matches Go GenericEnvelope)
    active_stages: Dict[str, bool] = field(default_factory=dict)
    completed_stage_set: Dict[str, bool] = field(default_factory=dict)
    failed_stages: Dict[str, str] = field(default_factory=dict)
    dag_mode: bool = False

    # Multi-stage
    completed_stages: List[Dict[str, Any]] = field(default_factory=list)
    current_stage_number: int = 1
    max_stages: int = 5
    all_goals: List[str] = field(default_factory=list)
    remaining_goals: List[str] = field(default_factory=list)
    goal_completion_status: Dict[str, str] = field(default_factory=dict)

    # Retry
    prior_plans: List[Dict[str, Any]] = field(default_factory=list)
    critic_feedback: List[str] = field(default_factory=list)

    # Audit
    processing_history: List[ProcessingRecord] = field(default_factory=list)
    errors: List[Dict[str, Any]] = field(default_factory=list)

    # Timing
    created_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None

    # Metadata
    metadata: Dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "GenericEnvelope":
        """Create envelope from dictionary (Go JSON response)."""
        env = cls()
        for key, value in data.items():
            if hasattr(env, key):
                # Handle enum conversion for terminal_reason
                if key == "terminal_reason" and value is not None:
                    if isinstance(value, str):
                        value = TerminalReason(value)
                setattr(env, key, value)
        return env

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
            "interrupt": self.interrupt,
            # DAG execution state
            "active_stages": self.active_stages,
            "completed_stage_set": self.completed_stage_set,
            "failed_stages": self.failed_stages,
            "dag_mode": self.dag_mode,
            "errors": self.errors,
            "metadata": self.metadata,
        }
