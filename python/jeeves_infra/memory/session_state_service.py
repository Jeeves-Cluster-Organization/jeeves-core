"""Session state service — per-session working memory.

Temporal pattern: workflow execution owns its state. Each SessionStateService
instance holds a single SessionState for one session. No shared registry,
no session_id lookups. The service IS the session state.

Tracks current focus (what entity is being interacted with), recently
referenced entities, and short-term context. Purely in-memory, ephemeral.

Separate from InvestigationService (game domain: snippets, stories, dossiers).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional


@dataclass
class EntityRef:
    """Reference to an entity mentioned in a session."""
    entity_type: str
    entity_id: str
    label: str
    context: str
    timestamp: datetime = field(default_factory=lambda: datetime.now(timezone.utc))


@dataclass
class SessionState:
    """Working memory state for a single session."""
    session_id: str
    user_id: str = ""
    focus_type: str = ""
    focus_id: str = ""
    focus_label: str = ""
    focus_context: Dict[str, Any] = field(default_factory=dict)
    entities: List[EntityRef] = field(default_factory=list)
    short_term_context: Dict[str, Any] = field(default_factory=dict)
    created_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))


class SessionStateService:
    """Per-session working memory. One instance per ActiveSession.

    All methods are sync — purely in-memory dict mutation, no I/O.
    Upgrade to SQLite/Redis backend if cross-session persistence is needed.
    """

    def __init__(self, session_id: str, user_id: str = "") -> None:
        self._state = SessionState(session_id=session_id, user_id=user_id)

    def get_state(self) -> SessionState:
        """Return the current session state."""
        return self._state

    def update_focus(
        self,
        focus_type: str,
        focus_id: str,
        focus_label: str = "",
        focus_context: Optional[Dict[str, Any]] = None,
    ) -> None:
        """Update the current focus entity for this session."""
        self._state.focus_type = focus_type
        self._state.focus_id = focus_id
        self._state.focus_label = focus_label
        self._state.focus_context = focus_context or {}
        self._state.updated_at = datetime.now(timezone.utc)

    def add_entity_reference(
        self,
        entity_type: str,
        entity_id: str,
        label: str = "",
        context: str = "",
    ) -> None:
        """Record that an entity was referenced in this session."""
        self._state.entities.append(EntityRef(
            entity_type=entity_type,
            entity_id=entity_id,
            label=label,
            context=context,
        ))
        self._state.updated_at = datetime.now(timezone.utc)

    def get_recent_entities(self, limit: int = 10) -> List[EntityRef]:
        """Get the most recently referenced entities, newest first."""
        return list(reversed(self._state.entities[-limit:]))

    def clear(self) -> None:
        """Reset state to empty (preserves session_id and user_id)."""
        self._state = SessionState(
            session_id=self._state.session_id,
            user_id=self._state.user_id,
        )
