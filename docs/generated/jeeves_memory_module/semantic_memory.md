# Semantic Memory (L3)

**Layer:** L3 - Semantic  
**Scope:** Embeddings-based search  
**Location:** `jeeves_infra/memory/repositories/chunk_repository.py`, `jeeves_infra/memory/repositories/pgvector_repository.py`, `jeeves_infra/memory/services/chunk_service.py`, `jeeves_infra/memory/services/embedding_service.py`, `jeeves_infra/memory/services/code_indexer.py`

---

## Overview

The Semantic Memory layer provides vector-based semantic search capabilities using embeddings. It enables content-based retrieval across all memory types, supporting intelligent context gathering for LLM prompts.

### Key Features

- Text chunking with configurable overlap
- Embedding generation using sentence-transformers
- Semantic similarity search with pgvector
- Code file indexing for RAG
- LRU caching for performance

---

## Architecture

```
┌─────────────────────┐     ┌──────────────────────┐
│   ChunkService      │────▶│   ChunkRepository    │
│  (High-level API)   │     │  (Chunk Storage)     │
└─────────────────────┘     └──────────────────────┘
         │                            │
         │ embed                      │ store
         ▼                            ▼
┌─────────────────────┐     ┌──────────────────────┐
│  EmbeddingService   │     │  PgVectorRepository  │
│  (384-dim vectors)  │     │  (Vector Search)     │
└─────────────────────┘     └──────────────────────┘
```

---

## Chunk

Represents a text chunk with embedding for semantic search.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `chunk_id` | `str` | Unique chunk identifier (UUID) |
| `user_id` | `str` | Owner user ID |
| `source_type` | `str` | Type of source ('journal', 'task', 'message', 'fact') |
| `source_id` | `str` | ID of source document |
| `content` | `str` | Text content of the chunk |
| `embedding` | `Optional[List[float]]` | Vector embedding (384 dimensions) |
| `metadata` | `Dict[str, Any]` | Additional structured metadata |
| `chunk_index` | `int` | Position within source document |
| `total_chunks` | `int` | Total number of chunks from source |
| `created_at` | `datetime` | Creation timestamp |
| `deleted_at` | `Optional[datetime]` | Soft delete timestamp |

### Methods

```python
def to_dict(self) -> Dict[str, Any]:
    """Convert to dictionary."""

@classmethod
def from_dict(cls, data: Dict[str, Any]) -> "Chunk":
    """Create from dictionary."""
```

---

## ChunkRepository

Repository for semantic chunk storage and retrieval.

### Constants

```python
EMBEDDING_DIM = 384  # Matches all-MiniLM-L6-v2
```

### Constructor

```python
def __init__(
    self,
    db: DatabaseClientProtocol,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

#### create

```python
async def create(self, chunk: Chunk) -> Chunk:
    """Create a new chunk."""
```

#### create_batch

```python
async def create_batch(self, chunks: List[Chunk]) -> List[Chunk]:
    """Create multiple chunks in a batch."""
```

#### get

```python
async def get(self, chunk_id: str) -> Optional[Chunk]:
    """Get a chunk by ID."""
```

#### get_by_source

```python
async def get_by_source(
    self,
    source_type: str,
    source_id: str
) -> List[Chunk]:
    """Get all chunks for a source document."""
```

#### search_similar

```python
async def search_similar(
    self,
    user_id: str,
    query_embedding: List[float],
    source_type: Optional[str] = None,
    limit: int = 10,
    min_similarity: float = 0.5
) -> List[Tuple[Chunk, float]]:
    """
    Search for similar chunks using cosine similarity.
    
    Returns:
        List of (Chunk, similarity_score) tuples
    """
```

#### update_embedding

```python
async def update_embedding(
    self,
    chunk_id: str,
    embedding: List[float]
) -> Optional[Chunk]:
    """Update the embedding for a chunk."""
```

#### delete_by_source

```python
async def delete_by_source(
    self,
    source_type: str,
    source_id: str
) -> int:
    """Soft-delete all chunks for a source."""
```

---

## ChunkService

High-level service for managing semantic chunks.

### Constructor

```python
def __init__(
    self,
    db: DatabaseClientProtocol,
    embedding_service: Optional[EmbeddingService] = None,
    repository: Optional[ChunkRepository] = None,
    chunk_size: int = 512,  # Characters per chunk
    chunk_overlap: int = 50,  # Overlap between chunks
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

#### chunk_and_store

```python
async def chunk_and_store(
    self,
    user_id: str,
    source_type: str,
    source_id: str,
    content: str,
    metadata: Optional[Dict[str, Any]] = None,
    generate_embeddings: bool = True
) -> List[Chunk]:
    """
    Chunk text and store with embeddings.
    
    Args:
        user_id: Owner user ID
        source_type: Type of source ('journal', 'task', 'message', 'fact')
        source_id: Source document ID
        content: Text content to chunk
        metadata: Additional metadata for chunks
        generate_embeddings: Whether to generate embeddings
        
    Returns:
        List of created chunks
    """
```

#### search

```python
async def search(
    self,
    user_id: str,
    query: str,
    source_type: Optional[str] = None,
    limit: int = 10,
    min_similarity: float = 0.5
) -> List[Dict[str, Any]]:
    """Search for semantically similar chunks."""
```

#### get_context_for_query

```python
async def get_context_for_query(
    self,
    user_id: str,
    query: str,
    max_context_length: int = 2000,
    source_type: Optional[str] = None
) -> str:
    """
    Get relevant context for a query.
    
    Retrieves and concatenates relevant chunks for inclusion
    in LLM prompts.
    """
```

#### reconstruct_source

```python
async def reconstruct_source(
    self,
    source_type: str,
    source_id: str
) -> str:
    """Reconstruct original text from chunks."""
```

---

## EmbeddingService

Generate embeddings for text using sentence-transformers.

### Constructor

```python
def __init__(
    self,
    model_name: str = "all-MiniLM-L6-v2",
    cache_size: int = 1000,
    logger: Optional[LoggerProtocol] = None
)
```

### Methods

#### embed

```python
def embed(self, text: str) -> List[float]:
    """
    Generate embedding for single text with caching.
    
    Args:
        text: Text to embed
        
    Returns:
        List of floats representing the embedding vector (384 dimensions)
    """
```

#### embed_batch

```python
def embed_batch(self, texts: List[str]) -> List[List[float]]:
    """
    Generate embeddings for multiple texts (more efficient than individual calls).
    """
```

#### similarity

```python
def similarity(self, text1: str, text2: str) -> float:
    """
    Calculate cosine similarity between two texts.
    
    Returns:
        Similarity score between 0 and 1
    """
```

#### get_cache_stats

```python
def get_cache_stats(self) -> Dict[str, Any]:
    """Get cache statistics (hit rate, size, etc.)."""
```

---

## PgVectorRepository

Handles embedding storage and semantic search via pgvector.

### Constructor

```python
def __init__(
    self,
    postgres_client: PostgreSQLClient,
    embedding_service: EmbeddingService,
    dimension: int = 384,
    logger: Optional[LoggerProtocol] = None
)
```

### Collection Configuration

```python
collection_config = {
    "tasks": {"table": "tasks", "id_col": "task_id", "content_col": "title"},
    "journal": {"table": "journal_entries", "id_col": "entry_id", "content_col": "content"},
    "facts": {"table": "knowledge_facts", "id_col": "fact_id", "content_col": "value"},
    "conversations": {"table": "messages", "id_col": "message_id", "content_col": "content"},
    "sessions": {"table": "sessions", "id_col": "session_id", "content_col": "title"},
}
```

### Methods

#### upsert

```python
async def upsert(
    self,
    item_id: str,
    content: str,
    collection: str,
    metadata: Optional[Dict[str, Any]] = None
) -> bool:
    """Generate embedding and store in PostgreSQL table."""
```

#### search

```python
async def search(
    self,
    query: str,
    collections: List[str],
    filters: Optional[Dict[str, Any]] = None,
    limit: int = 10,
    min_similarity: float = 0.5
) -> List[Dict[str, Any]]:
    """
    Semantic search across collections using pgvector.
    
    Returns:
        List of search results with scores:
        [{"item_id", "collection", "content", "score", "distance", "metadata"}, ...]
    """
```

#### batch_upsert

```python
async def batch_upsert(
    self,
    items: List[Dict[str, Any]],
    collection: str
) -> int:
    """Batch upsert embeddings for multiple items."""
```

#### rebuild_index

```python
async def rebuild_index(self, collection: str, lists: int = 100):
    """Rebuild the IVFFlat index for a collection."""
```

---

## CodeIndexer

Index code files for RAG-based semantic search.

### Constructor

```python
def __init__(
    self,
    postgres_client: DatabaseClientProtocol,
    embedding_service: EmbeddingService,
    language_config: Optional[LanguageConfigProtocol] = None,
    logger: Optional[LoggerProtocol] = None,
)
```

### Constants

```python
MAX_CONTENT_LENGTH = 8000  # Maximum chars to embed

INDEXABLE_EXTENSIONS = {
    '.py', '.js', '.ts', '.tsx', '.jsx',
    '.java', '.go', '.rs', '.rb', '.php',
    '.c', '.cpp', '.h', '.hpp', '.cs',
    '.sql', '.sh', '.bash', '.zsh',
    '.yaml', '.yml', '.json', '.toml',
    '.md', '.txt', '.rst',
    '.html', '.css', '.scss', '.less',
}

SKIP_DIRS = {
    '.git', 'node_modules', '__pycache__',
    'venv', '.venv', 'dist', 'build',
    '.idea', '.vscode', 'coverage', ...
}
```

### Methods

#### index_repository

```python
async def index_repository(
    self,
    repo_path: str,
    force: bool = False,
    progress_callback: Optional[callable] = None,
) -> Dict[str, Any]:
    """
    Index all files in a repository.
    
    Returns:
        Dict with indexing statistics:
        {"status", "total_files", "indexed", "skipped", "failed"}
    """
```

#### index_file

```python
async def index_file(
    self,
    repo_path: str,
    file_path: Path,
    force: bool = False,
) -> bool:
    """Index a single file."""
```

#### search

```python
async def search(
    self,
    query: str,
    limit: int = 10,
    min_similarity: float = 0.3,
    languages: Optional[List[str]] = None,
) -> List[Dict[str, Any]]:
    """
    Search indexed code files by semantic similarity.
    
    Returns:
        List of results with file_path, language, score
    """
```

---

## Usage Examples

### Chunking and Storing Content

```python
from jeeves_infra.memory.services.chunk_service import ChunkService
from jeeves_infra.memory.services.embedding_service import EmbeddingService

embedding_service = EmbeddingService()
chunk_service = ChunkService(db, embedding_service)

# Chunk and store a journal entry
chunks = await chunk_service.chunk_and_store(
    user_id="user-123",
    source_type="journal",
    source_id="entry-456",
    content="Today I worked on the authentication system...",
    metadata={"category": "work"}
)
```

### Semantic Search

```python
# Search for relevant chunks
results = await chunk_service.search(
    user_id="user-123",
    query="authentication issues",
    source_type="journal",
    limit=5
)

# Get context for LLM prompt
context = await chunk_service.get_context_for_query(
    user_id="user-123",
    query="What do I know about authentication?",
    max_context_length=2000
)
```

### Code Indexing

```python
from jeeves_infra.memory.services.code_indexer import CodeIndexer

indexer = CodeIndexer(postgres_client, embedding_service)

# Index entire repository
stats = await indexer.index_repository("/path/to/repo")
# Returns: {"total_files": 150, "indexed": 148, "skipped": 2, "failed": 0}

# Search code
results = await indexer.search(
    query="authentication middleware",
    languages=["python", "typescript"]
)
```

---

## Database Schema

### chunks table

```sql
CREATE TABLE chunks (
    chunk_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_id TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding TEXT,  -- JSON array or vector type
    metadata TEXT,  -- JSON
    chunk_index INTEGER DEFAULT 0,
    total_chunks INTEGER DEFAULT 1,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP
);

CREATE INDEX idx_chunks_user ON chunks(user_id);
CREATE INDEX idx_chunks_source ON chunks(source_type, source_id);
```

### code_index table

```sql
CREATE TABLE code_index (
    file_path TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    language TEXT NOT NULL,
    size_bytes INTEGER,
    line_count INTEGER,
    embedding vector(384),
    last_indexed TIMESTAMPTZ
);
```

---

## Navigation

- [Back to README](./README.md)
- [Previous: Event Sourcing (L2)](./event_sourcing.md)
- [Next: Session State (L4)](./session_state.md)
