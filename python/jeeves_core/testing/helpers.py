"""Test helper factories — one-liner context and config construction."""

from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from uuid import uuid4

from jeeves_core.protocols.types import (
    AgentConfig,
    AgentContext,
)


def make_agent_context(
    message: str = "test",
    user_id: str = "test-user",
    session_id: str = "test-session",
    outputs: Optional[Dict[str, Dict[str, Any]]] = None,
    metadata: Optional[Dict[str, Any]] = None,
    prompt_context: Optional[Dict[str, Any]] = None,
) -> AgentContext:
    """Create a test AgentContext with sensible defaults."""
    return AgentContext(
        envelope_id=f"env-{uuid4().hex[:8]}",
        request_id=f"test-{uuid4().hex[:8]}",
        user_id=user_id,
        session_id=session_id,
        raw_input=message,
        outputs=outputs or {},
        metadata=metadata or {},
        prompt_context=prompt_context or {},
    )


def make_agent_config(
    name: str,
    *,
    has_llm: bool = False,
    **overrides: Any,
) -> AgentConfig:
    """Create a test AgentConfig with sensible defaults."""
    return AgentConfig(name=name, has_llm=has_llm, **overrides)


def build_initial_envelope(
    *,
    envelope_id: str = "",
    request_id: str = "",
    user_id: str = "test-user",
    session_id: str = "test-session",
    raw_input: str = "test message",
    max_iterations: int = 10,
    max_llm_calls: int = 100,
    max_agent_hops: int = 20,
    max_stages: int = 3,
    stage_order: Optional[List[str]] = None,
    metadata: Optional[Dict[str, Any]] = None,
) -> Dict[str, Any]:
    """Build the initial envelope dict for kernel IPC.

    Centralises the ~30-line dict that kernel.initialize_orchestration_session
    expects, so test fixtures and lightweight callers share one source of truth.
    """
    now = datetime.now(timezone.utc).isoformat()
    return {
        "identity": {
            "envelope_id": envelope_id or f"env-{uuid4().hex[:8]}",
            "request_id": request_id or f"req-{uuid4().hex[:8]}",
            "user_id": user_id,
            "session_id": session_id,
        },
        "raw_input": raw_input,
        "received_at": now,
        "outputs": {},
        "pipeline": {
            "current_stage": "",
            "stage_order": stage_order or [],
            "iteration": 0,
            "max_iterations": max_iterations,
        },
        "bounds": {
            "llm_call_count": 0,
            "max_llm_calls": max_llm_calls,
            "tool_call_count": 0,
            "agent_hop_count": 0,
            "max_agent_hops": max_agent_hops,
            "tokens_in": 0,
            "tokens_out": 0,
            "terminated": False,
        },
        "interrupts": {"interrupt_pending": False},
        "execution": {
            "completed_stages": [],
            "current_stage_number": 0,
            "max_stages": max_stages,
            "all_goals": [],
            "remaining_goals": [],
            "goal_completion_status": {},
            "prior_plans": [],
            "loop_feedback": [],
        },
        "audit": {
            "processing_history": [],
            "errors": [],
            "created_at": now,
            "metadata": metadata or {},
        },
    }
