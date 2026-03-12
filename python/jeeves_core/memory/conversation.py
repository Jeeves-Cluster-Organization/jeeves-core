"""In-memory conversation history — default implementation.

Implements ConversationHistoryProtocol with a dict-backed store.
No external dependencies. Ships in pip package.
"""

from collections import defaultdict
from typing import Any, Dict, List, Optional


class InMemoryConversationHistory:
    """Dict-backed conversation history. Suitable for single-process use."""

    def __init__(self, max_turns: int = 100):
        self._store: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
        self._max_turns = max_turns

    async def add_turn(
        self,
        session_id: str,
        role: str,
        content: str,
        *,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> None:
        turn = {"role": role, "content": content}
        if metadata:
            turn["metadata"] = metadata
        self._store[session_id].append(turn)
        # Evict oldest if over limit
        if len(self._store[session_id]) > self._max_turns:
            self._store[session_id] = self._store[session_id][-self._max_turns:]

    async def get_history(
        self,
        session_id: str,
        *,
        limit: int = 20,
    ) -> List[Dict[str, Any]]:
        turns = self._store.get(session_id, [])
        return turns[-limit:] if limit else turns

    async def clear(self, session_id: str) -> None:
        self._store.pop(session_id, None)
