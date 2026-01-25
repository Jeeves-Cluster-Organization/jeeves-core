"""PostgreSQL Graph Adapter for L5 Entity Graph.

Implements GraphStorageProtocol using PostgreSQL with recursive CTEs.
This is the production adapter for L5 graph storage.

Constitutional Reference:
- Memory Module CONSTITUTION: L5 Graph - Entity relationships
- Avionics CONSTITUTION R4: Swappable Implementations
- Architecture: PostgreSQL-specific code lives in jeeves_avionics (L3)

Usage:
    from avionics.database.postgres_graph import PostgresGraphAdapter

    adapter = PostgresGraphAdapter(db_client, logger)
    await adapter.ensure_tables()

    await adapter.add_node("file:main.py", "file", {"path": "main.py"})
    await adapter.add_edge("file:main.py", "file:utils.py", "imports")
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from protocols import DatabaseClientProtocol, GraphStorageProtocol, LoggerProtocol
from shared import get_component_logger


class PostgresGraphAdapter:
    """PostgreSQL adapter implementing GraphStorageProtocol.

    Uses PostgreSQL tables for nodes and edges, with recursive CTEs
    for path finding and subgraph queries.

    This adapter provides:
    - Node storage with type and properties
    - Edge storage with relationship types
    - BFS path finding via recursive CTEs
    - Subgraph expansion queries
    - Soft-delete support (deleted_at)
    """

    CREATE_NODES_TABLE = """
        CREATE TABLE IF NOT EXISTS graph_nodes (
            node_id TEXT PRIMARY KEY,
            node_type TEXT NOT NULL,
            properties JSONB DEFAULT '{}',
            user_id TEXT,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            updated_at TIMESTAMPTZ DEFAULT NOW(),
            deleted_at TIMESTAMPTZ
        )
    """

    CREATE_EDGES_TABLE = """
        CREATE TABLE IF NOT EXISTS graph_edges (
            source_id TEXT NOT NULL,
            target_id TEXT NOT NULL,
            edge_type TEXT NOT NULL,
            properties JSONB DEFAULT '{}',
            created_at TIMESTAMPTZ DEFAULT NOW(),
            deleted_at TIMESTAMPTZ,
            PRIMARY KEY (source_id, target_id, edge_type)
        )
    """

    CREATE_INDICES = [
        "CREATE INDEX IF NOT EXISTS idx_graph_nodes_type ON graph_nodes(node_type)",
        "CREATE INDEX IF NOT EXISTS idx_graph_nodes_user ON graph_nodes(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_source ON graph_edges(source_id)",
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_target ON graph_edges(target_id)",
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_type ON graph_edges(edge_type)",
    ]

    def __init__(
        self,
        db: DatabaseClientProtocol,
        logger: Optional[LoggerProtocol] = None,
    ):
        """Initialize PostgreSQL graph adapter.

        Args:
            db: Database client implementing DatabaseClientProtocol
            logger: Optional logger instance
        """
        self._db = db
        self._logger = get_component_logger("PostgresGraphAdapter", logger)

    async def ensure_tables(self) -> None:
        """Create tables and indices if they don't exist."""
        await self._db.execute(self.CREATE_NODES_TABLE)
        await self._db.execute(self.CREATE_EDGES_TABLE)
        for index_sql in self.CREATE_INDICES:
            await self._db.execute(index_sql)
        self._logger.info("graph_tables_ensured")

    async def add_node(
        self,
        node_id: str,
        node_type: str,
        properties: Dict[str, Any],
        user_id: Optional[str] = None,
    ) -> bool:
        """Add a node to the graph.

        Args:
            node_id: Unique node identifier
            node_type: Type of node (e.g., "file", "function", "class")
            properties: Node properties/attributes
            user_id: Optional owner user ID

        Returns:
            True if created, False if already exists
        """
        # Check if node exists (not deleted)
        existing = await self._db.fetch_one(
            "SELECT node_id FROM graph_nodes WHERE node_id = $1 AND deleted_at IS NULL",
            {"node_id": node_id},
        )

        if existing:
            self._logger.debug("node_exists", node_id=node_id)
            return False

        # Insert or resurrect (if was soft-deleted)
        await self._db.execute(
            """
            INSERT INTO graph_nodes (node_id, node_type, properties, user_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, NOW(), NOW())
            ON CONFLICT (node_id) DO UPDATE SET
                node_type = EXCLUDED.node_type,
                properties = EXCLUDED.properties,
                user_id = EXCLUDED.user_id,
                updated_at = NOW(),
                deleted_at = NULL
            """,
            {
                "node_id": node_id,
                "node_type": node_type,
                "properties": json.dumps(properties),
                "user_id": user_id,
            },
        )

        self._logger.debug("node_added", node_id=node_id, node_type=node_type)
        return True

    async def add_edge(
        self,
        source_id: str,
        target_id: str,
        edge_type: str,
        properties: Optional[Dict[str, Any]] = None,
    ) -> bool:
        """Add an edge between nodes.

        Args:
            source_id: Source node ID
            target_id: Target node ID
            edge_type: Type of relationship (e.g., "imports", "calls", "inherits")
            properties: Optional edge properties

        Returns:
            True if created, False if already exists
        """
        # Check if edge exists (not deleted)
        existing = await self._db.fetch_one(
            """
            SELECT source_id FROM graph_edges 
            WHERE source_id = $1 AND target_id = $2 AND edge_type = $3 AND deleted_at IS NULL
            """,
            {"source_id": source_id, "target_id": target_id, "edge_type": edge_type},
        )

        if existing:
            self._logger.debug("edge_exists", source=source_id, target=target_id)
            return False

        # Insert or resurrect
        await self._db.execute(
            """
            INSERT INTO graph_edges (source_id, target_id, edge_type, properties, created_at)
            VALUES ($1, $2, $3, $4, NOW())
            ON CONFLICT (source_id, target_id, edge_type) DO UPDATE SET
                properties = EXCLUDED.properties,
                deleted_at = NULL
            """,
            {
                "source_id": source_id,
                "target_id": target_id,
                "edge_type": edge_type,
                "properties": json.dumps(properties or {}),
            },
        )

        self._logger.debug("edge_added", source=source_id, target=target_id, edge_type=edge_type)
        return True

    async def get_node(self, node_id: str) -> Optional[Dict[str, Any]]:
        """Get a node by ID.

        Args:
            node_id: Node identifier

        Returns:
            Node data with properties, or None if not found
        """
        row = await self._db.fetch_one(
            """
            SELECT node_id, node_type, properties, user_id, created_at, updated_at
            FROM graph_nodes
            WHERE node_id = $1 AND deleted_at IS NULL
            """,
            {"node_id": node_id},
        )

        if not row:
            return None

        return self._row_to_node(row)

    async def get_neighbors(
        self,
        node_id: str,
        edge_type: Optional[str] = None,
        direction: str = "both",
        limit: int = 100,
    ) -> List[Dict[str, Any]]:
        """Get neighboring nodes.

        Args:
            node_id: Center node ID
            edge_type: Optional filter by edge type
            direction: "in", "out", or "both"
            limit: Maximum neighbors to return

        Returns:
            List of neighbor nodes with edge information
        """
        neighbors = []

        # Outgoing neighbors
        if direction in ("out", "both"):
            edge_filter = "AND e.edge_type = $2" if edge_type else ""
            params = {"node_id": node_id, "edge_type": edge_type, "limit": limit} if edge_type else {"node_id": node_id, "limit": limit}

            query = f"""
                SELECT n.node_id, n.node_type, n.properties, n.user_id, n.created_at, n.updated_at,
                       e.edge_type, 'out' as direction
                FROM graph_edges e
                JOIN graph_nodes n ON n.node_id = e.target_id AND n.deleted_at IS NULL
                WHERE e.source_id = $1 AND e.deleted_at IS NULL {edge_filter}
                LIMIT ${'3' if edge_type else '2'}
            """

            rows = await self._db.fetch_all(query, params)
            for row in rows:
                neighbors.append(self._row_to_neighbor(row))

        # Incoming neighbors
        if direction in ("in", "both"):
            edge_filter = "AND e.edge_type = $2" if edge_type else ""
            params = {"node_id": node_id, "edge_type": edge_type, "limit": limit} if edge_type else {"node_id": node_id, "limit": limit}

            query = f"""
                SELECT n.node_id, n.node_type, n.properties, n.user_id, n.created_at, n.updated_at,
                       e.edge_type, 'in' as direction
                FROM graph_edges e
                JOIN graph_nodes n ON n.node_id = e.source_id AND n.deleted_at IS NULL
                WHERE e.target_id = $1 AND e.deleted_at IS NULL {edge_filter}
                LIMIT ${'3' if edge_type else '2'}
            """

            rows = await self._db.fetch_all(query, params)
            for row in rows:
                neighbors.append(self._row_to_neighbor(row))

        return neighbors[:limit]

    async def find_path(
        self,
        source_id: str,
        target_id: str,
        max_depth: int = 5,
    ) -> Optional[List[Dict[str, Any]]]:
        """Find path between two nodes using recursive CTE.

        Args:
            source_id: Start node
            target_id: End node
            max_depth: Maximum path length

        Returns:
            Path as list of nodes, or None if no path exists
        """
        # Recursive CTE for BFS path finding
        query = """
            WITH RECURSIVE path_search AS (
                -- Base case: start from source
                SELECT 
                    source_id,
                    target_id,
                    ARRAY[source_id] as path,
                    1 as depth
                FROM graph_edges
                WHERE source_id = $1 AND deleted_at IS NULL
                
                UNION ALL
                
                -- Recursive case: expand from current frontier
                SELECT 
                    e.source_id,
                    e.target_id,
                    ps.path || e.source_id,
                    ps.depth + 1
                FROM graph_edges e
                JOIN path_search ps ON e.source_id = ps.target_id
                WHERE e.deleted_at IS NULL
                    AND e.source_id != ALL(ps.path)  -- Avoid cycles
                    AND ps.depth < $3
            )
            SELECT path || target_id as full_path
            FROM path_search
            WHERE target_id = $2
            ORDER BY array_length(path, 1)
            LIMIT 1
        """

        row = await self._db.fetch_one(
            query,
            {"source_id": source_id, "target_id": target_id, "max_depth": max_depth},
        )

        if not row or not row.get("full_path"):
            return None

        # Fetch full node data for each ID in path
        path = []
        for node_id in row["full_path"]:
            node = await self.get_node(node_id)
            if node:
                path.append(node)

        return path if path else None

    async def query_subgraph(
        self,
        center_id: str,
        depth: int = 2,
        node_types: Optional[List[str]] = None,
        edge_types: Optional[List[str]] = None,
    ) -> Dict[str, Any]:
        """Query a subgraph around a center node.

        Args:
            center_id: Center node ID
            depth: Expansion depth
            node_types: Optional filter by node types
            edge_types: Optional filter by edge types

        Returns:
            Subgraph with nodes and edges
        """
        # Recursive CTE for subgraph expansion
        node_type_filter = ""
        edge_type_filter = ""

        if node_types:
            node_type_filter = f"AND n.node_type = ANY($4)"
        if edge_types:
            edge_type_filter = f"AND e.edge_type = ANY($5)"

        params: Dict[str, Any] = {
            "center_id": center_id,
            "depth": depth,
        }
        if node_types:
            params["node_types"] = node_types
        if edge_types:
            params["edge_types"] = edge_types

        # Get all reachable nodes within depth
        query = f"""
            WITH RECURSIVE subgraph AS (
                -- Base case: center node
                SELECT node_id, 0 as depth
                FROM graph_nodes
                WHERE node_id = $1 AND deleted_at IS NULL
                
                UNION
                
                -- Recursive case: expand neighbors
                SELECT DISTINCT e.target_id as node_id, s.depth + 1
                FROM graph_edges e
                JOIN subgraph s ON e.source_id = s.node_id
                WHERE e.deleted_at IS NULL
                    AND s.depth < $2
                    {edge_type_filter}
            )
            SELECT DISTINCT n.*
            FROM graph_nodes n
            JOIN subgraph s ON n.node_id = s.node_id
            WHERE n.deleted_at IS NULL
            {node_type_filter}
        """

        node_rows = await self._db.fetch_all(query, params)
        nodes = [self._row_to_node(row) for row in node_rows]

        # Get edges between the discovered nodes
        node_ids = [n["node_id"] for n in nodes]
        if node_ids:
            edge_params: Dict[str, Any] = {"node_ids": node_ids}
            if edge_types:
                edge_params["edge_types"] = edge_types

            edge_query = f"""
                SELECT source_id, target_id, edge_type, properties
                FROM graph_edges
                WHERE source_id = ANY($1) AND target_id = ANY($1)
                    AND deleted_at IS NULL
                    {edge_type_filter if edge_types else ''}
            """
            edge_rows = await self._db.fetch_all(edge_query, edge_params)
            edges = [
                {
                    "source_id": row["source_id"],
                    "target_id": row["target_id"],
                    "edge_type": row["edge_type"],
                    "properties": row.get("properties", {}),
                }
                for row in edge_rows
            ]
        else:
            edges = []

        return {"nodes": nodes, "edges": edges}

    async def delete_node(self, node_id: str) -> bool:
        """Soft-delete a node and its edges.

        Args:
            node_id: Node to delete

        Returns:
            True if deleted, False if not found
        """
        now = datetime.now(timezone.utc)

        # Soft-delete the node
        result = await self._db.fetch_one(
            """
            UPDATE graph_nodes
            SET deleted_at = $2
            WHERE node_id = $1 AND deleted_at IS NULL
            RETURNING node_id
            """,
            {"node_id": node_id, "deleted_at": now},
        )

        if not result:
            return False

        # Soft-delete all connected edges
        await self._db.execute(
            """
            UPDATE graph_edges
            SET deleted_at = $2
            WHERE (source_id = $1 OR target_id = $1) AND deleted_at IS NULL
            """,
            {"node_id": node_id, "deleted_at": now},
        )

        self._logger.info("node_deleted", node_id=node_id)
        return True

    def _row_to_node(self, row: Dict[str, Any]) -> Dict[str, Any]:
        """Convert database row to node dictionary."""
        properties = row.get("properties", {})
        if isinstance(properties, str):
            properties = json.loads(properties)

        return {
            "node_id": row["node_id"],
            "node_type": row["node_type"],
            "properties": properties,
            "user_id": row.get("user_id"),
            "created_at": row.get("created_at").isoformat() if row.get("created_at") else None,
            "updated_at": row.get("updated_at").isoformat() if row.get("updated_at") else None,
        }

    def _row_to_neighbor(self, row: Dict[str, Any]) -> Dict[str, Any]:
        """Convert database row to neighbor dictionary."""
        node = self._row_to_node(row)
        node["edge_type"] = row.get("edge_type")
        node["direction"] = row.get("direction")
        return node


# Verify protocol implementation at module load time
_: GraphStorageProtocol = PostgresGraphAdapter(None)  # type: ignore


__all__ = ["PostgresGraphAdapter"]
