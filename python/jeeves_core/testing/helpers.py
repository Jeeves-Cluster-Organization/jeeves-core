"""Test helper factories — one-liner context and config construction."""

from typing import Any, Dict, Optional
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
