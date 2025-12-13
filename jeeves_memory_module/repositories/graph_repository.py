"""
Graph Repository for L5 Relationship Memory.

Stores entity relationships (edges) for knowledge graph:
- Entity-to-entity connections
- Relationship types and strengths
- Temporal relationships
- Cross-reference edges

Constitutional Alignment:
- M2: Append-only with soft-delete
- M4: Structured edge metadata
- M5: Uses database client abstraction
"""

from typing import Dict, Any, Optional, List, Tuple
from datetime import datetime, timezone
from uuid import uuid4
from enum import Enum
import json

from jeeves_shared import get_component_logger, parse_datetime
from jeeves_protocols import LoggerProtocol, DatabaseClientProtocol


class RelationType(Enum):
    """Types of relationships between entities."""
    REFERENCES = "references"  # Entity A references Entity B
    RELATED_TO = "related_to"  # General relation
    DEPENDS_ON = "depends_on"  # Dependency relationship
    BLOCKS = "blocks"  # Blocking relationship
    PARENT_OF = "parent_of"  # Hierarchical
    CHILD_OF = "child_of"  # Hierarchical (inverse)
    PRECEDED_BY = "preceded_by"  # Temporal
    FOLLOWED_BY = "followed_by"  # Temporal (inverse)
    SIMILAR_TO = "similar_to"  # Semantic similarity
    CONTRADICTS = "contradicts"  # Conflict
    SUPPORTS = "supports"  # Agreement


class Edge:
    """Represents a relationship edge between two entities."""

    def __init__(
        self,
        edge_id: Optional[str] = None,
        user_id: str = "",
        source_type: str = "",  # 'task', 'journal', 'fact', 'message'
        source_id: str = "",
        target_type: str = "",
        target_id: str = "",
        relation_type: RelationType = RelationType.RELATED_TO,
        weight: float = 1.0,  # Relationship strength (0-1)
        metadata: Optional[Dict[str, Any]] = None,
        extracted_by: str = "system",  # 'llm', 'user', 'system'
        context: Optional[str] = None,  # Text context of relationship
        created_at: Optional[datetime] = None,
        deleted_at: Optional[datetime] = None
    ):
        """
        Initialize an edge.

        Args:
            edge_id: Unique edge identifier
            user_id: Owner user ID
            source_type: Type of source entity
            source_id: Source entity ID
            target_type: Type of target entity
            target_id: Target entity ID
            relation_type: Type of relationship
            weight: Relationship strength (0-1)
            metadata: Additional edge metadata
            extracted_by: How this edge was discovered
            context: Text context where relationship was found
            created_at: Creation timestamp
            deleted_at: Soft delete timestamp
        """
        self.edge_id = edge_id or str(uuid4())
        self.user_id = user_id
        self.source_type = source_type
        self.source_id = source_id
        self.target_type = target_type
        self.target_id = target_id
        self.relation_type = relation_type
        self.weight = max(0.0, min(1.0, weight))  # Clamp to 0-1
        self.metadata = metadata or {}
        self.extracted_by = extracted_by
        self.context = context
        self.created_at = created_at or datetime.now(timezone.utc)
        self.deleted_at = deleted_at

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "edge_id": self.edge_id,
            "user_id": self.user_id,
            "source_type": self.source_type,
            "source_id": self.source_id,
            "target_type": self.target_type,
            "target_id": self.target_id,
            "relation_type": self.relation_type.value if isinstance(self.relation_type, RelationType) else self.relation_type,
            "weight": self.weight,
            "metadata": self.metadata,
            "extracted_by": self.extracted_by,
            "context": self.context,
            "created_at": self.created_at.isoformat() if self.created_at else None,
            "deleted_at": self.deleted_at.isoformat() if self.deleted_at else None
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Edge":
        """Create from dictionary."""
        relation_type = data.get("relation_type", "related_to")
        if isinstance(relation_type, str):
            try:
                relation_type = RelationType(relation_type)
            except ValueError:
                relation_type = RelationType.RELATED_TO

        return cls(
            edge_id=data.get("edge_id"),
            user_id=data.get("user_id", ""),
            source_type=data.get("source_type", ""),
            source_id=data.get("source_id", ""),
            target_type=data.get("target_type", ""),
            target_id=data.get("target_id", ""),
            relation_type=relation_type,
            weight=data.get("weight", 1.0),
            metadata=data.get("metadata"),
            extracted_by=data.get("extracted_by", "system"),
            context=data.get("context"),
            created_at=parse_datetime(data.get("created_at")),
            deleted_at=parse_datetime(data.get("deleted_at"))
        )

    @property
    def source_key(self) -> str:
        """Get source entity key."""
        return f"{self.source_type}:{self.source_id}"

    @property
    def target_key(self) -> str:
        """Get target entity key."""
        return f"{self.target_type}:{self.target_id}"


class GraphRepository:
    """
    Repository for relationship edge storage.

    Supports graph traversal and relationship queries.
    """

    CREATE_TABLE_SQL = """
        CREATE TABLE IF NOT EXISTS edges (
            edge_id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            source_type TEXT NOT NULL,
            source_id TEXT NOT NULL,
            target_type TEXT NOT NULL,
            target_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            weight REAL DEFAULT 1.0,
            metadata TEXT,  -- JSON
            extracted_by TEXT DEFAULT 'system',
            context TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            deleted_at TIMESTAMP
        )
    """

    CREATE_INDICES_SQL = [
        "CREATE INDEX IF NOT EXISTS idx_edges_user ON edges(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_type, source_id)",
        "CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_type, target_id)",
        "CREATE INDEX IF NOT EXISTS idx_edges_relation ON edges(relation_type)"
    ]

    def __init__(self, db: DatabaseClientProtocol, logger: Optional[LoggerProtocol] = None):
        """Initialize repository."""
        self._logger = get_component_logger("GraphRepository", logger)
        self.db = db

    async def ensure_table(self) -> None:
        """Ensure the edges table exists."""
        await self.db.execute(self.CREATE_TABLE_SQL)
        for index_sql in self.CREATE_INDICES_SQL:
            await self.db.execute(index_sql)

    async def create(self, edge: Edge) -> Edge:
        """
        Create a new edge.

        Args:
            edge: Edge to create

        Returns:
            Created edge
        """
        query = """
            INSERT INTO edges
            (edge_id, user_id, source_type, source_id, target_type, target_id,
             relation_type, weight, metadata, extracted_by, context,
             created_at, deleted_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """

        params = (
            edge.edge_id,
            edge.user_id,
            edge.source_type,
            edge.source_id,
            edge.target_type,
            edge.target_id,
            edge.relation_type.value if isinstance(edge.relation_type, RelationType) else edge.relation_type,
            edge.weight,
            json.dumps(edge.metadata) if edge.metadata else None,
            edge.extracted_by,
            edge.context,
            edge.created_at.isoformat() if edge.created_at else None,
            edge.deleted_at.isoformat() if edge.deleted_at else None
        )

        await self.db.execute(query, params)

        self._logger.debug(
            "edge_created",
            edge_id=edge.edge_id,
            source=f"{edge.source_type}:{edge.source_id}",
            target=f"{edge.target_type}:{edge.target_id}",
            relation=edge.relation_type.value if isinstance(edge.relation_type, RelationType) else edge.relation_type
        )

        return edge

    async def get(self, edge_id: str) -> Optional[Edge]:
        """
        Get an edge by ID.

        Args:
            edge_id: Edge identifier

        Returns:
            Edge or None if not found
        """
        query = """
            SELECT * FROM edges
            WHERE edge_id = ? AND deleted_at IS NULL
        """

        row = await self.db.fetch_one(query, (edge_id,))
        if not row:
            return None

        return self._row_to_edge(row)

    async def get_outgoing(
        self,
        user_id: str,
        source_type: str,
        source_id: str,
        relation_type: Optional[RelationType] = None
    ) -> List[Edge]:
        """
        Get all outgoing edges from an entity.

        Args:
            user_id: User identifier
            source_type: Source entity type
            source_id: Source entity ID
            relation_type: Filter by relation type (optional)

        Returns:
            List of outgoing edges
        """
        conditions = [
            "user_id = ?",
            "source_type = ?",
            "source_id = ?",
            "deleted_at IS NULL"
        ]
        params: List[Any] = [user_id, source_type, source_id]

        if relation_type:
            conditions.append("relation_type = ?")
            params.append(relation_type.value if isinstance(relation_type, RelationType) else relation_type)

        query = f"""
            SELECT * FROM edges
            WHERE {' AND '.join(conditions)}
            ORDER BY weight DESC, created_at DESC
        """

        rows = await self.db.fetch_all(query, tuple(params))
        return [self._row_to_edge(row) for row in rows]

    async def get_incoming(
        self,
        user_id: str,
        target_type: str,
        target_id: str,
        relation_type: Optional[RelationType] = None
    ) -> List[Edge]:
        """
        Get all incoming edges to an entity.

        Args:
            user_id: User identifier
            target_type: Target entity type
            target_id: Target entity ID
            relation_type: Filter by relation type (optional)

        Returns:
            List of incoming edges
        """
        conditions = [
            "user_id = ?",
            "target_type = ?",
            "target_id = ?",
            "deleted_at IS NULL"
        ]
        params: List[Any] = [user_id, target_type, target_id]

        if relation_type:
            conditions.append("relation_type = ?")
            params.append(relation_type.value if isinstance(relation_type, RelationType) else relation_type)

        query = f"""
            SELECT * FROM edges
            WHERE {' AND '.join(conditions)}
            ORDER BY weight DESC, created_at DESC
        """

        rows = await self.db.fetch_all(query, tuple(params))
        return [self._row_to_edge(row) for row in rows]

    async def get_related(
        self,
        user_id: str,
        entity_type: str,
        entity_id: str,
        relation_type: Optional[RelationType] = None
    ) -> List[Tuple[Edge, str]]:
        """
        Get all edges involving an entity (both directions).

        Args:
            user_id: User identifier
            entity_type: Entity type
            entity_id: Entity ID
            relation_type: Filter by relation type (optional)

        Returns:
            List of (Edge, direction) tuples where direction is 'outgoing' or 'incoming'
        """
        results: List[Tuple[Edge, str]] = []

        outgoing = await self.get_outgoing(user_id, entity_type, entity_id, relation_type)
        for edge in outgoing:
            results.append((edge, "outgoing"))

        incoming = await self.get_incoming(user_id, entity_type, entity_id, relation_type)
        for edge in incoming:
            results.append((edge, "incoming"))

        return results

    async def find_edge(
        self,
        user_id: str,
        source_type: str,
        source_id: str,
        target_type: str,
        target_id: str,
        relation_type: Optional[RelationType] = None
    ) -> Optional[Edge]:
        """
        Find a specific edge between two entities.

        Args:
            user_id: User identifier
            source_type: Source entity type
            source_id: Source entity ID
            target_type: Target entity type
            target_id: Target entity ID
            relation_type: Relation type (optional)

        Returns:
            Edge if found, None otherwise
        """
        conditions = [
            "user_id = ?",
            "source_type = ?",
            "source_id = ?",
            "target_type = ?",
            "target_id = ?",
            "deleted_at IS NULL"
        ]
        params: List[Any] = [user_id, source_type, source_id, target_type, target_id]

        if relation_type:
            conditions.append("relation_type = ?")
            params.append(relation_type.value if isinstance(relation_type, RelationType) else relation_type)

        query = f"""
            SELECT * FROM edges
            WHERE {' AND '.join(conditions)}
            LIMIT 1
        """

        row = await self.db.fetch_one(query, tuple(params))
        if not row:
            return None

        return self._row_to_edge(row)

    async def update_weight(
        self,
        edge_id: str,
        new_weight: float
    ) -> Optional[Edge]:
        """
        Update edge weight.

        Args:
            edge_id: Edge identifier
            new_weight: New weight value (0-1)

        Returns:
            Updated edge or None if not found
        """
        new_weight = max(0.0, min(1.0, new_weight))

        query = """
            UPDATE edges
            SET weight = ?
            WHERE edge_id = ? AND deleted_at IS NULL
        """

        await self.db.execute(query, (new_weight, edge_id))

        return await self.get(edge_id)

    async def delete(self, edge_id: str) -> bool:
        """
        Soft-delete an edge.

        Args:
            edge_id: Edge identifier

        Returns:
            True if deleted
        """
        now = datetime.now(timezone.utc)

        query = """
            UPDATE edges
            SET deleted_at = ?
            WHERE edge_id = ? AND deleted_at IS NULL
        """

        await self.db.execute(query, (now, edge_id))

        self._logger.info("edge_deleted", edge_id=edge_id)

        return True

    async def delete_by_entity(
        self,
        source_type: str,
        source_id: str
    ) -> int:
        """
        Soft-delete all edges involving an entity as source.

        Args:
            source_type: Entity type
            source_id: Entity ID

        Returns:
            Number of edges deleted (approximate)
        """
        now = datetime.now(timezone.utc)

        query = """
            UPDATE edges
            SET deleted_at = ?
            WHERE source_type = ? AND source_id = ? AND deleted_at IS NULL
        """

        await self.db.execute(query, (now, source_type, source_id))

        return 0

    async def count_by_user(self, user_id: str) -> int:
        """
        Count edges for a user.

        Args:
            user_id: User identifier

        Returns:
            Number of edges
        """
        query = """
            SELECT COUNT(*) as count FROM edges
            WHERE user_id = ? AND deleted_at IS NULL
        """

        row = await self.db.fetch_one(query, (user_id,))
        return row["count"] if row else 0

    def _row_to_edge(self, row: Dict[str, Any]) -> Edge:
        """Convert database row to Edge."""
        metadata = row.get("metadata")
        if isinstance(metadata, str):
            metadata = json.loads(metadata) if metadata else {}

        relation_type = row.get("relation_type", "related_to")
        if isinstance(relation_type, str):
            try:
                relation_type = RelationType(relation_type)
            except ValueError:
                relation_type = RelationType.RELATED_TO

        return Edge(
            edge_id=row["edge_id"],
            user_id=row["user_id"],
            source_type=row["source_type"],
            source_id=row["source_id"],
            target_type=row["target_type"],
            target_id=row["target_id"],
            relation_type=relation_type,
            weight=row.get("weight", 1.0),
            metadata=metadata,
            extracted_by=row.get("extracted_by", "system"),
            context=row.get("context"),
            created_at=parse_datetime(row.get("created_at")),
            deleted_at=parse_datetime(row.get("deleted_at"))
        )
