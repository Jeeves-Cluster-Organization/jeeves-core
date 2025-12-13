"""Memory types - working memory, findings, focus state."""

from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional


class FocusType(str, Enum):
    """Type of focus in working memory."""
    FILE = "file"
    FUNCTION = "function"
    CLASS = "class"
    MODULE = "module"
    CONCEPT = "concept"


@dataclass
class EntityRef:
    """Reference to an entity in working memory."""
    entity_type: str
    entity_id: str
    name: str
    context: Optional[str] = None
    timestamp: Optional[datetime] = None


@dataclass
class FocusState:
    """Current focus state in working memory."""
    focus_type: FocusType
    focus_id: str
    focus_name: str
    context: Dict[str, Any] = field(default_factory=dict)
    set_at: Optional[datetime] = None


@dataclass
class Finding:
    """Discovery result from analysis."""
    finding_id: str
    finding_type: str
    title: str
    description: str
    location: Optional[str] = None
    severity: str = "info"
    evidence: List[Dict[str, Any]] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)
    created_at: Optional[datetime] = None


@dataclass
class WorkingMemory:
    """Agent working memory state."""
    session_id: str
    user_id: str

    # Focus
    current_focus: Optional[FocusState] = None
    focus_history: List[FocusState] = field(default_factory=list)

    # Entities
    entities: List[EntityRef] = field(default_factory=list)

    # Findings
    findings: List[Finding] = field(default_factory=list)

    # Context
    context: Dict[str, Any] = field(default_factory=dict)

    # Metadata
    created_at: Optional[datetime] = None
    updated_at: Optional[datetime] = None

    def add_entity(self, entity: EntityRef) -> None:
        """Add entity to working memory."""
        self.entities.append(entity)

    def add_finding(self, finding: Finding) -> None:
        """Add finding to working memory."""
        self.findings.append(finding)

    def set_focus(self, focus: FocusState) -> None:
        """Set current focus."""
        if self.current_focus:
            self.focus_history.append(self.current_focus)
        self.current_focus = focus

    def clear_focus(self) -> None:
        """Clear current focus."""
        if self.current_focus:
            self.focus_history.append(self.current_focus)
        self.current_focus = None


@dataclass
class MemoryItem:
    """Generic memory item for storage."""
    item_id: str
    item_type: str
    content: Any
    metadata: Dict[str, Any] = field(default_factory=dict)
    created_at: Optional[datetime] = None
    expires_at: Optional[datetime] = None


@dataclass
class ClarificationContext:
    """Context for clarification requests."""
    question: str
    options: List[str] = field(default_factory=list)
    context: Dict[str, Any] = field(default_factory=dict)
    timeout_seconds: int = 300
    created_at: Optional[datetime] = None


# Pure functions for working memory operations
def create_working_memory(session_id: str, user_id: str) -> WorkingMemory:
    """Factory to create new working memory."""
    from jeeves_shared.serialization import utc_now
    return WorkingMemory(
        session_id=session_id,
        user_id=user_id,
        created_at=utc_now(),
        updated_at=utc_now(),
    )


def merge_working_memory(base: WorkingMemory, update: WorkingMemory) -> WorkingMemory:
    """Merge two working memory instances."""
    from jeeves_shared.serialization import utc_now
    return WorkingMemory(
        session_id=base.session_id,
        user_id=base.user_id,
        current_focus=update.current_focus or base.current_focus,
        focus_history=base.focus_history + update.focus_history,
        entities=base.entities + update.entities,
        findings=base.findings + update.findings,
        context={**base.context, **update.context},
        created_at=base.created_at,
        updated_at=utc_now(),
    )


# =============================================================================
# FOCUS OPERATIONS
# =============================================================================

def set_focus(memory: WorkingMemory, focus: FocusState) -> WorkingMemory:
    """Set focus in working memory."""
    memory.set_focus(focus)
    return memory


def clear_focus(memory: WorkingMemory) -> WorkingMemory:
    """Clear focus in working memory."""
    memory.clear_focus()
    return memory


# =============================================================================
# ENTITY OPERATIONS
# =============================================================================

def add_entity_ref(memory: WorkingMemory, entity: EntityRef) -> WorkingMemory:
    """Add entity reference to working memory."""
    memory.add_entity(entity)
    return memory


def get_recent_entities(memory: WorkingMemory, limit: int = 10) -> List[EntityRef]:
    """Get most recent entities from working memory."""
    return memory.entities[-limit:] if memory.entities else []


# =============================================================================
# CLARIFICATION OPERATIONS
# =============================================================================

def set_clarification(memory: WorkingMemory, clarification: ClarificationContext) -> WorkingMemory:
    """Set clarification context in working memory."""
    memory.context["clarification"] = {
        "question": clarification.question,
        "options": clarification.options,
        "context": clarification.context,
    }
    return memory


def clear_clarification(memory: WorkingMemory) -> WorkingMemory:
    """Clear clarification context from working memory."""
    if "clarification" in memory.context:
        del memory.context["clarification"]
    return memory


# =============================================================================
# SERIALIZATION
# =============================================================================

def serialize_working_memory(memory: WorkingMemory) -> Dict[str, Any]:
    """Serialize working memory to dictionary."""
    return {
        "session_id": memory.session_id,
        "user_id": memory.user_id,
        "current_focus": {
            "focus_type": memory.current_focus.focus_type.value,
            "focus_id": memory.current_focus.focus_id,
            "focus_name": memory.current_focus.focus_name,
            "context": memory.current_focus.context,
        } if memory.current_focus else None,
        "entities": [
            {
                "entity_type": e.entity_type,
                "entity_id": e.entity_id,
                "name": e.name,
                "context": e.context,
            }
            for e in memory.entities
        ],
        "findings": [
            {
                "finding_id": f.finding_id,
                "finding_type": f.finding_type,
                "title": f.title,
                "description": f.description,
                "location": f.location,
                "severity": f.severity,
            }
            for f in memory.findings
        ],
        "context": memory.context,
    }


def deserialize_working_memory(data: Dict[str, Any]) -> WorkingMemory:
    """Deserialize working memory from dictionary."""
    memory = WorkingMemory(
        session_id=data.get("session_id", ""),
        user_id=data.get("user_id", ""),
        context=data.get("context", {}),
    )

    # Restore focus
    if data.get("current_focus"):
        focus_data = data["current_focus"]
        memory.current_focus = FocusState(
            focus_type=FocusType(focus_data["focus_type"]),
            focus_id=focus_data["focus_id"],
            focus_name=focus_data["focus_name"],
            context=focus_data.get("context", {}),
        )

    # Restore entities
    for e in data.get("entities", []):
        memory.entities.append(EntityRef(
            entity_type=e["entity_type"],
            entity_id=e["entity_id"],
            name=e["name"],
            context=e.get("context"),
        ))

    # Restore findings
    for f in data.get("findings", []):
        memory.findings.append(Finding(
            finding_id=f["finding_id"],
            finding_type=f["finding_type"],
            title=f["title"],
            description=f["description"],
            location=f.get("location"),
            severity=f.get("severity", "info"),
        ))

    return memory
