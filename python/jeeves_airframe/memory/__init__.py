"""Jeeves Memory Module.

Provides:
- Session state management (working memory: focus, entities, context)
- Tool health governance (L7)
"""

from jeeves_airframe.memory.session_state_service import (
    SessionStateService,
    SessionState,
    EntityRef,
)

__all__ = [
    "SessionStateService",
    "SessionState",
    "EntityRef",
]
