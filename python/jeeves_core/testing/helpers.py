"""Test helper factories — one-liner envelope and config construction."""

from typing import Any, Dict, Optional
from uuid import uuid4

from jeeves_core.protocols.types import (
    AgentConfig,
    Envelope,
)
from jeeves_core.protocols.interfaces import RequestContext


def make_envelope(
    message: str = "test",
    user_id: str = "test-user",
    session_id: str = "test-session",
    **metadata: Any,
) -> Envelope:
    """Create a test envelope with sensible defaults."""
    request_id = f"test-{uuid4().hex[:8]}"
    return Envelope(
        request_context=RequestContext(
            request_id=request_id,
            capability="test",
            session_id=session_id,
            user_id=user_id,
        ),
        envelope_id=f"env-{uuid4().hex[:8]}",
        request_id=request_id,
        user_id=user_id,
        session_id=session_id,
        raw_input=message,
        metadata=metadata,
    )


def make_agent_config(
    name: str,
    *,
    has_llm: bool = False,
    **overrides: Any,
) -> AgentConfig:
    """Create a test AgentConfig with sensible defaults."""
    return AgentConfig(name=name, has_llm=has_llm, **overrides)
