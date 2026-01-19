"""
JeevesState TypedDict for Jeeves runtime workflow.

Centralized Architecture (v4.0):
This state container mirrors the GenericEnvelope structure to enable
easy conversion between the two. Uses dynamic outputs dict pattern.

Design Principles:
- Keep close to GenericEnvelope for easy conversion
- Use plain List types (merge logic handled by runtime)
- Immutable identity fields, mutable processing fields

List merging is done explicitly in the runtime's merge_state() function.
"""

from typing import TypedDict, Optional, List, Dict, Any


# Fields that require list concatenation during state merges.
# The runtime's merge_state() function handles these explicitly.
REDUCER_FIELDS = [
    "step_results",
    "prior_plans",
    "loop_feedback",
    "completed_stages",
    "confidence_history",
    "routing_decisions",
    "errors",
]


class JeevesState(TypedDict, total=False):
    """
    Jeeves runtime state container - mirrors GenericEnvelope structure.

    This TypedDict flows through all nodes in the pipeline.
    List merging is handled by the runtime's merge_state() function.

    Fields marked total=False are optional during state updates.
    """

    # ─── Identity (immutable per workflow) ───
    envelope_id: str
    request_id: str
    user_id: str
    session_id: str

    # ─── Original Input ───
    raw_input: str
    received_at: str  # ISO format

    # ─── Agent Outputs (each agent writes to exactly one) ───
    perception: Optional[Dict[str, Any]]
    intent: Optional[Dict[str, Any]]
    plan: Optional[Dict[str, Any]]
    arbiter: Optional[Dict[str, Any]]
    execution: Optional[Dict[str, Any]]
    synthesizer: Optional[Dict[str, Any]]  # 7-agent architecture
    critic: Optional[Dict[str, Any]]
    integration: Optional[Dict[str, Any]]

    # ─── Flow Control ───
    stage: str  # Current stage name (defined in PipelineConfig)
    iteration: int

    # ─── Multi-step Support ───
    current_step_index: int  # Which step in plan we're executing
    step_results: List[Dict[str, Any]]  # Accumulate results (reducer field)

    # ─── Interrupts ───
    confirmation_required: bool
    confirmation_message: Optional[str]
    confirmation_response: Optional[bool]
    clarification_required: bool
    clarification_question: Optional[str]
    clarification_response: Optional[str]

    # ─── Termination ───
    terminated: bool
    terminal_reason: Optional[str]

    # ─── Retry Context ───
    prior_plans: List[Dict[str, Any]]  # Reducer field
    loop_feedback: List[str]  # Reducer field (feedback for loop_back routing)

    # ─── Loop Context (loop_back architecture) ───
    loop_count: int  # Number of loop_back iterations executed
    loop_feedback_for_intent: Optional[Dict[str, Any]]  # Hints for Intent on loop_back

    # ─── Multi-Stage Execution ───
    completed_stages: List[Dict[str, Any]]  # Reducer field
    current_stage: int
    max_stages: int
    goal_completion_status: Dict[str, str]
    all_goals: List[str]
    remaining_goals: List[str]

    # ─── Observability (Decision Audit Trail) ───
    confidence_history: List[Dict[str, Any]]  # Confidence at each stage (reducer field)
    routing_decisions: List[Dict[str, Any]]  # Routing decisions with reasons (reducer field)

    # ─── Audit ───
    errors: List[Dict[str, Any]]  # Reducer field

    # ─── Timing ───
    completed_at: Optional[str]  # ISO format

    # ─── Metadata ───
    metadata: Dict[str, Any]


def create_initial_state(
    user_id: str,
    session_id: str,
    raw_input: str,
    envelope_id: str,
    request_id: str,
    received_at: str,
    metadata: Optional[Dict[str, Any]] = None,
) -> JeevesState:
    """
    Create initial state for a new workflow.

    Args:
        user_id: User identifier
        session_id: Session identifier
        raw_input: Original user message
        envelope_id: Unique envelope ID
        request_id: Unique request ID
        received_at: ISO timestamp of request receipt
        metadata: Optional metadata dict

    Returns:
        JeevesState with all fields initialized
    """
    return JeevesState(
        envelope_id=envelope_id,
        request_id=request_id,
        user_id=user_id,
        session_id=session_id,
        raw_input=raw_input,
        received_at=received_at,
        perception=None,
        intent=None,
        plan=None,
        arbiter=None,
        execution=None,
        synthesizer=None,
        critic=None,
        integration=None,
        stage="perception",
        iteration=0,
        current_step_index=0,
        step_results=[],
        confirmation_required=False,
        confirmation_message=None,
        confirmation_response=None,
        clarification_required=False,
        clarification_question=None,
        clarification_response=None,
        terminated=False,
        terminal_reason=None,
        prior_plans=[],
        loop_feedback=[],
        # Loop context (loop_back architecture)
        loop_count=0,
        loop_feedback_for_intent=None,
        # Multi-stage execution
        completed_stages=[],
        current_stage=1,
        max_stages=5,
        goal_completion_status={},
        all_goals=[],
        remaining_goals=[],
        # Observability (Decision Audit Trail)
        confidence_history=[],
        routing_decisions=[],
        errors=[],
        completed_at=None,
        metadata=metadata or {},
    )
