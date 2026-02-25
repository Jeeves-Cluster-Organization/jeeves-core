"""Memory Queries - Request/Response patterns for memory operations.

These queries can be sent via CommBus to retrieve memory data.
Handlers in the memory module process these and return responses.
"""

from dataclasses import dataclass, field
from typing import List


@dataclass(frozen=True)
class GetSessionState:
    """Query for session working memory.

    Attributes:
        session_id: Session identifier
        user_id: User identifier

    Returns:
        WorkingMemory instance
    """
    category: str = field(default="query", init=False)
    session_id: str = ""
    user_id: str = ""


@dataclass(frozen=True)
class GetRecentEntities:
    """Query for recently referenced entities.

    Attributes:
        session_id: Session identifier
        limit: Maximum entities to return

    Returns:
        List[EntityRef]
    """
    category: str = field(default="query", init=False)
    session_id: str = ""
    limit: int = 10
