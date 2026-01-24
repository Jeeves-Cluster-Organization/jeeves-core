# Entity Graphs (L5)

**Layer:** L5 - Graph  
**Scope:** Entity relationships  
**Location:** `jeeves_memory_module/repositories/graph_stub.py`

---

## Overview

The Entity Graphs layer provides relationship tracking between entities in the Jeeves system. The current implementation is an **in-memory stub** designed for development and testing. Production implementations should use a proper graph database (Neo4j, etc.) or PostgreSQL with recursive CTEs.

### Key Features

- In-memory graph storage
- Node and edge management
- BFS path finding
- Subgraph queries
- Extensible design for production backends

---

## Architecture

```
┌─────────────────────────┐
│  InMemoryGraphStorage   │
│  (Development Stub)     │
└─────────────────────────┘
          │ implements
          ▼
┌─────────────────────────┐
│  GraphStorageProtocol   │
│  (from jeeves_protocols)│
└─────────────────────────┘
```

### Extension Points

For production, the stub can be replaced with:
- `PostgresGraphAdapter` from `jeeves_avionics.database`
- Custom Neo4j adapter
- Any implementation of `GraphStorageProtocol`

---

## GraphNode

In-memory graph node representation.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `node_id` | `str` | Unique node identifier |
| `node_type` | `str` | Type of node ('task', 'user', 'project', etc.) |
| `properties` | `Dict[str, Any]` | Node properties |
| `user_id` | `Optional[str]` | Owner user ID |
| `created_at` | `datetime` | Creation timestamp |
| `updated_at` | `datetime` | Last update timestamp |

---

## GraphEdge

In-memory graph edge representation.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `source_id` | `str` | Source node ID |
| `target_id` | `str` | Target node ID |
| `edge_type` | `str` | Type of relationship |
| `properties` | `Dict[str, Any]` | Edge properties |
| `created_at` | `datetime` | Creation timestamp |

---

## InMemoryGraphStorage

In-memory implementation of `GraphStorageProtocol`.

### Constructor

```python
def __init__(self, logger: Optional[LoggerProtocol] = None)
```

### Internal Data Structures

```python
_nodes: Dict[str, GraphNode] = {}
_edges: Dict[Tuple[str, str, str], GraphEdge] = {}  # (source, target, type)
_outgoing: Dict[str, Set[Tuple[str, str]]] = {}  # node_id -> [(target, type)]
_incoming: Dict[str, Set[Tuple[str, str]]] = {}  # node_id -> [(source, type)]
```

### Methods

#### add_node

```python
async def add_node(
    self,
    node_id: str,
    node_type: str,
    properties: Dict[str, Any],
    user_id: Optional[str] = None,
) -> bool:
    """
    Add a node to the graph.
    
    Returns:
        True if added, False if already exists
    """
```

#### add_edge

```python
async def add_edge(
    self,
    source_id: str,
    target_id: str,
    edge_type: str,
    properties: Optional[Dict[str, Any]] = None,
) -> bool:
    """
    Add an edge between nodes.
    
    Returns:
        True if added, False if exists or nodes missing
    """
```

#### get_node

```python
async def get_node(self, node_id: str) -> Optional[Dict[str, Any]]:
    """Get a node by ID."""
```

#### get_neighbors

```python
async def get_neighbors(
    self,
    node_id: str,
    edge_type: Optional[str] = None,
    direction: str = "both",  # "in", "out", or "both"
    limit: int = 100,
) -> List[Dict[str, Any]]:
    """
    Get neighboring nodes.
    
    Returns:
        List of neighbor dicts with edge_type and direction info
    """
```

#### find_path

```python
async def find_path(
    self,
    source_id: str,
    target_id: str,
    max_depth: int = 5,
) -> Optional[List[Dict[str, Any]]]:
    """
    Find path between two nodes using BFS.
    
    Returns:
        List of nodes in path, or None if no path exists
    """
```

#### query_subgraph

```python
async def query_subgraph(
    self,
    center_id: str,
    depth: int = 2,
    node_types: Optional[List[str]] = None,
    edge_types: Optional[List[str]] = None,
) -> Dict[str, Any]:
    """
    Query a subgraph around a center node.
    
    Returns:
        {"nodes": [...], "edges": [...]}
    """
```

#### delete_node

```python
async def delete_node(self, node_id: str) -> bool:
    """Delete a node and its edges."""
```

#### get_stats

```python
def get_stats(self) -> Dict[str, int]:
    """Get graph statistics."""
    # Returns: {"node_count": N, "edge_count": M}
```

### Extension Points

Override these methods in subclasses for custom backends:

```python
async def _persist_node(self, node: GraphNode) -> None:
    """Extension point: persist node to backend."""

async def _persist_edge(self, edge: GraphEdge) -> None:
    """Extension point: persist edge to backend."""

async def _load_graph(self) -> None:
    """Extension point: load graph from backend on startup."""
```

---

## Usage Examples

### Basic Graph Operations

```python
from jeeves_memory_module.repositories.graph_stub import InMemoryGraphStorage

graph = InMemoryGraphStorage()

# Add nodes
await graph.add_node(
    node_id="user-123",
    node_type="user",
    properties={"name": "Alice"}
)

await graph.add_node(
    node_id="task-456",
    node_type="task",
    properties={"title": "Review code"}
)

# Add relationship
await graph.add_edge(
    source_id="user-123",
    target_id="task-456",
    edge_type="assigned_to",
    properties={"assigned_at": "2026-01-23"}
)
```

### Querying Relationships

```python
# Get all tasks assigned to user
neighbors = await graph.get_neighbors(
    node_id="user-123",
    edge_type="assigned_to",
    direction="out"
)

# Find path between entities
path = await graph.find_path(
    source_id="user-123",
    target_id="project-789",
    max_depth=3
)

# Get subgraph around entity
subgraph = await graph.query_subgraph(
    center_id="project-789",
    depth=2,
    node_types=["user", "task"]
)
```

### Production Pattern

```python
from jeeves_protocols import GraphStorageProtocol

# Development: use in-memory stub
from jeeves_memory_module.repositories import InMemoryGraphStorage
graph: GraphStorageProtocol = InMemoryGraphStorage()

# Production: use PostgreSQL adapter
from jeeves_avionics.database import PostgresGraphAdapter
graph: GraphStorageProtocol = PostgresGraphAdapter(db_client)
await graph.ensure_tables()
```

---

## Edge Types Reference

Common relationship types used in Jeeves:

| Edge Type | Description | Example |
|-----------|-------------|---------|
| `assigned_to` | Entity is assigned to someone | task → user |
| `created_by` | Entity was created by someone | task → user |
| `belongs_to` | Entity belongs to a container | task → project |
| `references` | Entity references another | message → task |
| `related_to` | General relationship | task → task |
| `depends_on` | Dependency relationship | task → task |
| `mentions` | Entity mentions another | journal → person |

---

## Protocol Compliance

The `InMemoryGraphStorage` class implements `GraphStorageProtocol` from `jeeves_protocols`. This is verified at module load:

```python
# Verify protocol implementation
_: GraphStorageProtocol = InMemoryGraphStorage()
```

---

## Navigation

- [Back to README](./README.md)
- [Previous: Session State (L4)](./session_state.md)
- [Next: Skills/Patterns (L6)](./skills.md)
