# Skills and Patterns (L6)

**Layer:** L6 - Skills  
**Scope:** Learned patterns  
**Location:** `jeeves_infra/memory/repositories/skill_stub.py`

---

## Overview

The Skills layer provides storage for learned patterns that help the system improve over time. The current implementation is an **in-memory stub** for development and testing. Production implementations should persist skills to a database.

### What Are Skills?

Skills are learned patterns that capture successful behaviors:
- Tool usage patterns (what worked before)
- Code generation templates
- User preference learning
- Successful prompt patterns

### Key Features

- In-memory skill storage
- Confidence-based retrieval
- Usage tracking and feedback
- Automatic confidence adjustment
- Extensible design for ML integration

---

## Architecture

```
┌─────────────────────────┐
│  InMemorySkillStorage   │
│  (Development Stub)     │
└─────────────────────────┘
          │ implements
          ▼
┌─────────────────────────┐
│  SkillStorageProtocol   │
│  (from protocols)│
└─────────────────────────┘
```

---

## Skill

In-memory skill representation.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `skill_id` | `str` | Unique skill identifier |
| `skill_type` | `str` | Type of skill ('tool_usage', 'code_template', etc.) |
| `pattern` | `Dict[str, Any]` | The learned pattern |
| `source_context` | `Optional[Dict[str, Any]]` | Context where skill was learned |
| `confidence` | `float` | Confidence score (0.0-1.0) |
| `user_id` | `Optional[str]` | Owner user ID |
| `usage_count` | `int` | Number of times skill was used |
| `success_count` | `int` | Number of successful uses |
| `failure_count` | `int` | Number of failed uses |
| `created_at` | `datetime` | Creation timestamp |
| `updated_at` | `datetime` | Last update timestamp |
| `last_used_at` | `Optional[datetime]` | When skill was last used |

---

## SkillUsage

Record of skill usage for learning.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `skill_id` | `str` | Skill identifier |
| `success` | `bool` | Whether the usage was successful |
| `context` | `Optional[Dict[str, Any]]` | Usage context |
| `timestamp` | `datetime` | When the skill was used |

---

## InMemorySkillStorage

In-memory implementation of `SkillStorageProtocol`.

### Constructor

```python
def __init__(
    self,
    logger: Optional[LoggerProtocol] = None,
    min_confidence_decay: float = 0.1,
    max_confidence: float = 1.0,
)
```

### Methods

#### store_skill

```python
async def store_skill(
    self,
    skill_id: str,
    skill_type: str,
    pattern: Dict[str, Any],
    source_context: Optional[Dict[str, Any]] = None,
    confidence: float = 0.5,
    user_id: Optional[str] = None,
) -> str:
    """
    Store a learned skill/pattern.
    
    Returns:
        Skill ID
    """
```

#### get_skill

```python
async def get_skill(self, skill_id: str) -> Optional[Dict[str, Any]]:
    """Get a skill by ID."""
```

#### find_skills

```python
async def find_skills(
    self,
    skill_type: Optional[str] = None,
    context: Optional[Dict[str, Any]] = None,
    min_confidence: float = 0.0,
    limit: int = 10,
    user_id: Optional[str] = None,
) -> List[Dict[str, Any]]:
    """
    Find relevant skills.
    
    Args:
        skill_type: Filter by skill type
        context: Current context for relevance matching
        min_confidence: Minimum confidence threshold
        limit: Maximum results
        user_id: Filter by user
        
    Returns:
        List of skills sorted by confidence (descending)
    """
```

#### update_confidence

```python
async def update_confidence(
    self,
    skill_id: str,
    delta: float,
    reason: Optional[str] = None,
) -> float:
    """
    Update skill confidence based on outcome.
    
    Args:
        skill_id: Skill identifier
        delta: Confidence change (+/-)
        reason: Reason for update
        
    Returns:
        New confidence value (clamped to [min_decay, max_confidence])
    """
```

#### record_usage

```python
async def record_usage(
    self,
    skill_id: str,
    success: bool,
    context: Optional[Dict[str, Any]] = None,
) -> None:
    """
    Record skill usage for learning.
    
    Automatically adjusts confidence:
    - Success: +0.05
    - Failure: -0.10
    """
```

#### delete_skill

```python
async def delete_skill(self, skill_id: str) -> bool:
    """Delete a skill."""
```

#### get_skill_stats

```python
async def get_skill_stats(self, skill_id: str) -> Optional[Dict[str, Any]]:
    """
    Get usage statistics for a skill.
    
    Returns:
        {
            "skill_id": "...",
            "usage_count": 10,
            "success_count": 8,
            "failure_count": 2,
            "success_rate": 0.8,
            "confidence": 0.75,
            "created_at": "...",
            "last_used_at": "..."
        }
    """
```

#### get_stats

```python
def get_stats(self) -> Dict[str, Any]:
    """
    Get skill storage statistics.
    
    Returns:
        {
            "skill_count": 50,
            "total_usage": 200,
            "average_confidence": 0.65,
            "usage_history_size": 500
        }
    """
```

### Extension Points

Override these methods in subclasses for custom backends:

```python
async def _persist_skill(self, skill: Skill) -> None:
    """Extension point: persist skill to backend."""

async def _load_skills(self) -> None:
    """Extension point: load skills from backend on startup."""

async def _match_context(
    self,
    skill: Skill,
    context: Dict[str, Any],
) -> float:
    """
    Extension point: match skill to context.
    
    Override for custom relevance matching (e.g., semantic similarity).
    
    Returns:
        Relevance score (0.0 to 1.0), 0 means no match
    """
```

---

## Skill Types Reference

Common skill types in Jeeves:

| Skill Type | Description | Example Pattern |
|------------|-------------|-----------------|
| `tool_usage` | How to use a specific tool | `{"tool": "git_commit", "args_pattern": {...}}` |
| `code_template` | Reusable code patterns | `{"language": "python", "template": "..."}` |
| `user_preference` | User's preferred behaviors | `{"response_style": "concise"}` |
| `prompt_pattern` | Successful prompt structures | `{"intent": "code_review", "template": "..."}` |
| `error_recovery` | How to recover from errors | `{"error_type": "timeout", "recovery": {...}}` |

---

## Usage Examples

### Storing Skills

```python
from jeeves_infra.memory.repositories.skill_stub import InMemorySkillStorage

skills = InMemorySkillStorage()

# Store a tool usage pattern
await skills.store_skill(
    skill_id="skill-001",
    skill_type="tool_usage",
    pattern={
        "tool": "code_search",
        "query_template": "function ${name} in ${language}",
        "filters": {"exclude_tests": True}
    },
    source_context={
        "task": "find_function_definition",
        "success": True
    },
    confidence=0.7,
    user_id="user-123"
)
```

### Finding Relevant Skills

```python
# Find skills for current context
relevant_skills = await skills.find_skills(
    skill_type="tool_usage",
    context={"task": "find_function_definition"},
    min_confidence=0.5,
    limit=5
)

# Use the most confident skill
if relevant_skills:
    best_skill = relevant_skills[0]
    pattern = best_skill["pattern"]
```

### Learning from Outcomes

```python
# Record successful usage
await skills.record_usage(
    skill_id="skill-001",
    success=True,
    context={"result_count": 3}
)

# Record failed usage
await skills.record_usage(
    skill_id="skill-001",
    success=False,
    context={"error": "no_results"}
)

# Check updated stats
stats = await skills.get_skill_stats("skill-001")
# stats["success_rate"] reflects learning
```

### Confidence Management

```python
# Manually boost confidence (e.g., after validation)
new_confidence = await skills.update_confidence(
    skill_id="skill-001",
    delta=0.1,
    reason="validated_by_user"
)

# Decay confidence (e.g., skill not used in a while)
await skills.update_confidence(
    skill_id="skill-001",
    delta=-0.05,
    reason="decay_from_disuse"
)
```

---

## Protocol Compliance

The `InMemorySkillStorage` class implements `SkillStorageProtocol` from `protocols`:

```python
# Verify protocol implementation
_: SkillStorageProtocol = InMemorySkillStorage()
```

---

## Future Extensions

Potential enhancements for production:
1. **ML-based skill extraction** - Learn patterns from successful interactions
2. **Semantic context matching** - Use embeddings for better relevance
3. **Skill composition** - Combine multiple skills for complex tasks
4. **Transfer learning** - Share skills across users (with privacy controls)
5. **Skill versioning** - Track pattern evolution over time

---

## Navigation

- [Back to README](./README.md)
- [Previous: Entity Graphs (L5)](./entity_graphs.md)
- [Next: Tool Health (L7)](./tool_health.md)
