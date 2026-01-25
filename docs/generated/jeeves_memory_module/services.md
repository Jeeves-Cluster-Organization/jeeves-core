# Services

**Location:** `memory_module/services/`

---

## Overview

The services layer provides the business logic for memory operations. Services use repositories for persistence and implement higher-level operations that orchestrate multiple components.

### Service Categories

| Category | Services | Purpose |
|----------|----------|---------|
| **Protocol Adapters** | `SessionStateAdapter` | Bridge Core protocols to implementations |
| **Memory Services** | `ChunkService`, `SessionStateService` | L3/L4 memory operations |
| **Event Services** | `EventEmitter`, `TraceRecorder` | L2 event logging |
| **Infrastructure** | `EmbeddingService`, `CodeIndexer` | Support services |
| **Governance** | `ToolHealthService` | L7 monitoring |
| **Intelligence** | `NLIService` | Claim verification |

---

## Service Exports

```python
from memory_module.services import (
    # Protocol Adapters
    SessionStateAdapter,
    
    # Infrastructure Services
    EmbeddingService,
    NLIService,
    get_nli_service,
    CrossRefManager,
    EventEmitter,
    TraceRecorder,
    SessionStateService,
    ChunkService,
    ToolHealthService,
    CodeIndexer,
)
```

---

## SessionStateAdapter

**Purpose:** Implements Core's `SessionStateProtocol`, bridging to `SessionStateService`.

**See:** [Session State Documentation](./session_state.md)

```python
from memory_module.services import SessionStateAdapter

adapter = SessionStateAdapter(db)
memory = await adapter.get_or_create("sess-123", "user-456")
# Returns: WorkingMemory (Core protocol type)
```

---

## EmbeddingService

**Purpose:** Generate embeddings for text using sentence-transformers.

**See:** [Semantic Memory Documentation](./semantic_memory.md)

### Quick Reference

```python
from memory_module.services import EmbeddingService

service = EmbeddingService(
    model_name="all-MiniLM-L6-v2",  # 384 dimensions
    cache_size=1000
)

# Single embedding
embedding = service.embed("Hello world")
# Returns: List[float] (384 dimensions)

# Batch embedding
embeddings = service.embed_batch(["Hello", "World"])

# Similarity
score = service.similarity("Hello", "Hi there")
# Returns: float (0.0 to 1.0)

# Cache stats
stats = service.get_cache_stats()
```

---

## NLIService

**Purpose:** Natural Language Inference for claim verification.

### Overview

Uses a pretrained cross-encoder NLI model to verify that claims are entailed by their cited evidence. This is an anti-hallucination gate per Constitution P1.

### Constructor

```python
def __init__(
    self,
    model_name: Optional[str] = None,  # Default: "cross-encoder/nli-MiniLM2-L6-H768"
    device: str = "cpu",
    enabled: bool = True,
    logger: Optional[LoggerProtocol] = None
)
```

### Key Types

```python
@dataclass
class NLIResult:
    label: str       # "entailment", "neutral", or "contradiction"
    score: float     # Confidence 0-1
    entailment_score: float
    
    @property
    def is_entailed(self) -> bool
    
    @property
    def is_contradicted(self) -> bool

@dataclass
class ClaimVerificationResult:
    claim: str
    citation: str
    nli_result: NLIResult
    verified: bool
    confidence: float
    reason: str
```

### Methods

```python
def check_entailment(self, premise: str, hypothesis: str) -> NLIResult:
    """Check if premise entails hypothesis."""

def verify_claim(
    self,
    claim: str,
    evidence: str,
    citation: str = "",
    threshold: float = 0.6
) -> ClaimVerificationResult:
    """Verify a single claim against evidence."""

def verify_claims_batch(
    self,
    claims: List[Dict[str, str]],
    evidence_map: Dict[str, str],
    threshold: float = 0.6
) -> List[ClaimVerificationResult]:
    """Verify multiple claims against their evidence."""
```

### Usage

```python
from memory_module.services import NLIService, get_nli_service

# Singleton pattern
nli = get_nli_service()

# Verify a claim
result = nli.verify_claim(
    claim="UserService handles authentication",
    evidence="class UserService:\n    def authenticate(self, user)...",
    citation="src/services/user.py:10"
)

if result.verified:
    print(f"Claim verified with {result.confidence:.0%} confidence")
else:
    print(f"Claim not verified: {result.reason}")
```

---

## CrossRefManager

**Purpose:** Manage relationships between memory items.

### Relationship Types

| Relationship | Description |
|--------------|-------------|
| `references` | General reference |
| `mentions` | Entity mentions another |
| `related_to` | Related entities |

### Constructor

```python
def __init__(
    self,
    db_client: DatabaseClientProtocol,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

```python
async def create_ref(
    self,
    source_id: str,
    source_type: str,
    target_id: str,
    target_type: str,
    relationship: str = "references",
    confidence: float = 1.0
) -> str:
    """Create a cross-reference."""

async def find_refs(
    self,
    item_id: str,
    direction: str = "both",  # "outgoing", "incoming", "both"
    item_type: Optional[str] = None,
    relationship: Optional[str] = None,
    min_confidence: float = 0.0
) -> List[Dict[str, Any]]:
    """Find all references for an item."""

async def get_related_items(
    self,
    item_id: str,
    item_type: str,
    depth: int = 1,
    min_confidence: float = 0.5
) -> Dict[str, List[str]]:
    """Get all items related to this item (grouped by type)."""

async def delete_refs_for_item(self, item_id: str) -> int:
    """Delete all cross-references for an item."""

async def create_refs_batch(
    self,
    references: List[Dict[str, Any]]
) -> List[str]:
    """
    Create multiple cross-references in batch.
    
    Args:
        references: List of reference dicts with source_id, source_type,
                   target_id, target_type, relationship, confidence
    
    Returns:
        List of reference IDs
    """

async def extract_refs_from_text(
    self,
    text: str,
    source_id: str,
    source_type: str,
    existing_items: Optional[Dict[str, List[Dict[str, Any]]]] = None,
    min_confidence: float = 0.7
) -> List[str]:
    """
    Extract references from text using pattern matching.
    
    This is a simple implementation that looks for explicit mentions.
    Could be enhanced with LLM-based extraction for better accuracy.
    
    Args:
        text: Text to analyze
        source_id: Source item ID
        source_type: Source item type
        existing_items: Optional dict of existing items by type
        min_confidence: Minimum confidence threshold
    
    Returns:
        List of created reference IDs
    """

async def get_ref_stats(self) -> Dict[str, Any]:
    """
    Get statistics about cross-references.
    
    Returns:
        Dict with stats (total_refs, by_relationship, by_source_type)
    """
```

### Usage

```python
from memory_module.services import CrossRefManager

xref = CrossRefManager(db)

# Create reference
ref_id = await xref.create_ref(
    source_id="task-123",
    source_type="task",
    target_id="journal-456",
    target_type="journal",
    relationship="mentions",
    confidence=0.9
)

# Find related items
related = await xref.get_related_items(
    item_id="task-123",
    item_type="task",
    depth=2
)
# Returns: {"tasks": [...], "journal": [...], "messages": [...], "facts": [...]}
```

---

## EventEmitter

**Purpose:** Emit domain events for the L2 event log.

**See:** [Event Sourcing Documentation](./event_sourcing.md)

### Quick Reference

```python
from memory_module.services import EventEmitter

emitter = EventEmitter(event_repository)

# Emit with session deduplication
await emitter.emit(
    aggregate_type="task",
    aggregate_id="task-123",
    event_type="task_completed",
    payload={"completed_at": "..."},
    user_id="user-456",
    session_id="sess-789"  # For deduplication
)

# Convenience methods
await emitter.emit_task_created(task_id, user_id, title, ...)
await emitter.emit_memory_stored(item_id, layer, user_id, item_type, ...)
```

---

## TraceRecorder

**Purpose:** Record agent decision traces for audit and debugging.

### Constructor

```python
def __init__(
    self,
    trace_repository: TraceRepository,
    feature_flags: Optional[FeatureFlagsProtocol] = None,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

```python
def start_trace(
    self,
    agent_name: str,
    stage: str,
    correlation_id: str,
    request_id: str,
    user_id: str,
    input_data: Optional[Dict[str, Any]] = None,
    llm_model: Optional[str] = None,
    parent_span_id: Optional[str] = None
) -> AgentTrace:
    """Start a new trace."""

async def complete_trace(
    self,
    trace: AgentTrace,
    output_data: Optional[Dict[str, Any]] = None,
    status: str = "success",
    confidence: Optional[float] = None,
    error_details: Optional[Dict[str, Any]] = None
) -> Optional[str]:
    """Complete and save a trace."""

@asynccontextmanager
async def trace_context(self, ...):
    """Context manager for automatic trace start/complete."""
```

### Usage

```python
from memory_module.services import TraceRecorder

recorder = TraceRecorder(trace_repository)

# Using context manager
async with recorder.trace_context(
    agent_name="planner",
    stage="planning",
    correlation_id="corr-123",
    request_id="req-456",
    user_id="user-789"
) as trace:
    result = await planner.plan(request)
    trace.output_data = result
    trace.confidence = 0.95
```

---

## SessionStateService

**Purpose:** High-level session state management.

**See:** [Session State Documentation](./session_state.md)

### Quick Reference

```python
from memory_module.services import SessionStateService

service = SessionStateService(db)

# Get or create session
state = await service.get_or_create("sess-123", "user-456")

# Update focus
await service.update_focus("sess-123", "task", focus_id="task-789")

# Record entity reference
await service.record_entity_reference("sess-123", "task", "task-789", "Code review")

# Get context for LLM prompts
context = await service.get_context_for_prompt("sess-123")
```

---

## ChunkService

**Purpose:** Semantic chunking and retrieval.

**See:** [Semantic Memory Documentation](./semantic_memory.md)

### Quick Reference

```python
from memory_module.services import ChunkService

service = ChunkService(db, embedding_service)

# Chunk and store
chunks = await service.chunk_and_store(
    user_id="user-123",
    source_type="journal",
    source_id="entry-456",
    content="Long text content..."
)

# Semantic search
results = await service.search(
    user_id="user-123",
    query="authentication issues"
)

# Get context for prompts
context = await service.get_context_for_query(
    user_id="user-123",
    query="What do I know about auth?",
    max_context_length=2000
)
```

---

## ToolHealthService

**Purpose:** Monitor tool health and performance.

**See:** [Tool Health Documentation](./tool_health.md)

### Quick Reference

```python
from memory_module.services import ToolHealthService

service = ToolHealthService(db)

# Record execution
await service.record_execution(
    tool_name="code_search",
    user_id="user-123",
    status="success",
    execution_time_ms=150
)

# Check health
report = await service.check_tool_health("code_search")

# Circuit breaker
if await service.should_circuit_break("code_search"):
    # Use fallback
    pass
```

---

## CodeIndexer

**Purpose:** Index code files for RAG-based semantic search.

**See:** [Semantic Memory Documentation](./semantic_memory.md)

### Quick Reference

```python
from memory_module.services import CodeIndexer

indexer = CodeIndexer(postgres_client, embedding_service)

# Index repository
stats = await indexer.index_repository("/path/to/repo")
# Returns: {"total_files": 150, "indexed": 148, ...}

# Search code
results = await indexer.search(
    query="authentication middleware",
    languages=["python"]
)

# Get stats
stats = await indexer.get_stats()
```

---

## Service Dependencies

```
                    ┌─────────────────┐
                    │  DatabaseClient │
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│  Repository   │   │  Repository   │   │  Repository   │
│   (Events)    │   │   (Chunks)    │   │  (Session)    │
└───────┬───────┘   └───────┬───────┘   └───────┬───────┘
        │                   │                   │
        ▼                   ▼                   ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│ EventEmitter  │   │ ChunkService  │◀──│ SessionState  │
│               │   │               │   │   Service     │
└───────────────┘   └───────────────┘   └───────────────┘
                            │                   │
                            ▼                   ▼
                    ┌───────────────┐   ┌───────────────┐
                    │ Embedding     │   │ SessionState  │
                    │   Service     │   │   Adapter     │
                    └───────────────┘   └───────────────┘
```

---

## Navigation

- [Back to README](./README.md)
- [Previous: CommBus Messages](./messages.md)
