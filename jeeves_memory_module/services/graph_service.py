"""
Graph Service for L5 Relationship Memory.

High-level service for managing entity relationships:
- Relationship creation and discovery
- Graph traversal and path finding
- Context building from relationships
- Cross-reference management

Constitutional Alignment:
- P1: Uses LLM for relationship extraction
- P5: Deterministic graph operations
- M5: Uses repository abstraction
"""

from typing import Dict, Any, Optional, List, Set, Tuple
from datetime import datetime, timezone

from jeeves_protocols import DatabaseClientProtocol
from jeeves_memory_module.repositories.graph_repository import (
    GraphRepository,
    Edge,
    RelationType
)
from jeeves_shared import get_component_logger
from jeeves_protocols import LoggerProtocol


class GraphService:
    """
    Service for managing the entity relationship graph.

    Provides:
    - Relationship creation and querying
    - Graph traversal (BFS/DFS)
    - Related entity discovery
    - Context building from relationships
    """

    def __init__(
        self,
        db: DatabaseClientProtocol,
        repository: Optional[GraphRepository] = None,
        logger: Optional[LoggerProtocol] = None
    ):
        """
        Initialize service.

        Args:
            db: Database client instance
            repository: Optional pre-configured repository
            logger: Optional logger instance (ADR-001 DI)
        """
        self._logger = get_component_logger("GraphService", logger)
        self.db = db
        self.repository = repository or GraphRepository(db)

    async def ensure_initialized(self) -> None:
        """Ensure the repository table exists."""
        await self.repository.ensure_table()

    async def create_relationship(
        self,
        user_id: str,
        source_type: str,
        source_id: str,
        target_type: str,
        target_id: str,
        relation_type: RelationType,
        weight: float = 1.0,
        metadata: Optional[Dict[str, Any]] = None,
        extracted_by: str = "system",
        context: Optional[str] = None
    ) -> Edge:
        """
        Create a relationship between two entities.

        Args:
            user_id: Owner user ID
            source_type: Source entity type
            source_id: Source entity ID
            target_type: Target entity type
            target_id: Target entity ID
            relation_type: Type of relationship
            weight: Relationship strength (0-1)
            metadata: Additional metadata
            extracted_by: How this was discovered
            context: Text context of relationship

        Returns:
            Created edge
        """
        # Check if edge already exists
        existing = await self.repository.find_edge(
            user_id=user_id,
            source_type=source_type,
            source_id=source_id,
            target_type=target_type,
            target_id=target_id,
            relation_type=relation_type
        )

        if existing:
            # Update weight if stronger
            if weight > existing.weight:
                await self.repository.update_weight(existing.edge_id, weight)
                existing.weight = weight
            return existing

        edge = Edge(
            user_id=user_id,
            source_type=source_type,
            source_id=source_id,
            target_type=target_type,
            target_id=target_id,
            relation_type=relation_type,
            weight=weight,
            metadata=metadata or {},
            extracted_by=extracted_by,
            context=context
        )

        created = await self.repository.create(edge)

        self._logger.info(
            "relationship_created",
            source=f"{source_type}:{source_id}",
            target=f"{target_type}:{target_id}",
            relation=relation_type.value
        )

        return created

    async def get_related_entities(
        self,
        user_id: str,
        entity_type: str,
        entity_id: str,
        relation_type: Optional[RelationType] = None,
        direction: str = "both"  # 'outgoing', 'incoming', 'both'
    ) -> List[Dict[str, Any]]:
        """
        Get entities related to a given entity.

        Args:
            user_id: User identifier
            entity_type: Entity type
            entity_id: Entity ID
            relation_type: Filter by relation type (optional)
            direction: Edge direction to consider

        Returns:
            List of related entities with relationship info
        """
        results: List[Dict[str, Any]] = []
        seen: Set[str] = set()

        if direction in ("outgoing", "both"):
            outgoing = await self.repository.get_outgoing(
                user_id=user_id,
                source_type=entity_type,
                source_id=entity_id,
                relation_type=relation_type
            )
            for edge in outgoing:
                key = f"{edge.target_type}:{edge.target_id}"
                if key not in seen:
                    seen.add(key)
                    results.append({
                        "entity_type": edge.target_type,
                        "entity_id": edge.target_id,
                        "relation": edge.relation_type.value if isinstance(edge.relation_type, RelationType) else edge.relation_type,
                        "direction": "outgoing",
                        "weight": edge.weight
                    })

        if direction in ("incoming", "both"):
            incoming = await self.repository.get_incoming(
                user_id=user_id,
                target_type=entity_type,
                target_id=entity_id,
                relation_type=relation_type
            )
            for edge in incoming:
                key = f"{edge.source_type}:{edge.source_id}"
                if key not in seen:
                    seen.add(key)
                    results.append({
                        "entity_type": edge.source_type,
                        "entity_id": edge.source_id,
                        "relation": edge.relation_type.value if isinstance(edge.relation_type, RelationType) else edge.relation_type,
                        "direction": "incoming",
                        "weight": edge.weight
                    })

        # Sort by weight descending
        results.sort(key=lambda x: -x["weight"])

        return results

    async def find_path(
        self,
        user_id: str,
        source_type: str,
        source_id: str,
        target_type: str,
        target_id: str,
        max_depth: int = 4
    ) -> Optional[List[Edge]]:
        """
        Find a path between two entities using BFS.

        Args:
            user_id: User identifier
            source_type: Source entity type
            source_id: Source entity ID
            target_type: Target entity type
            target_id: Target entity ID
            max_depth: Maximum path length

        Returns:
            List of edges forming the path, or None if no path found
        """
        source_key = f"{source_type}:{source_id}"
        target_key = f"{target_type}:{target_id}"

        if source_key == target_key:
            return []

        # BFS
        visited: Set[str] = {source_key}
        queue: List[Tuple[str, str, str, List[Edge]]] = [(source_type, source_id, source_key, [])]

        while queue and len(queue[0][3]) < max_depth:
            current_type, current_id, current_key, path = queue.pop(0)

            # Get neighbors
            edges = await self.repository.get_outgoing(
                user_id=user_id,
                source_type=current_type,
                source_id=current_id
            )

            for edge in edges:
                neighbor_key = f"{edge.target_type}:{edge.target_id}"

                if neighbor_key == target_key:
                    return path + [edge]

                if neighbor_key not in visited:
                    visited.add(neighbor_key)
                    queue.append((
                        edge.target_type,
                        edge.target_id,
                        neighbor_key,
                        path + [edge]
                    ))

        return None

    async def get_neighborhood(
        self,
        user_id: str,
        entity_type: str,
        entity_id: str,
        depth: int = 2,
        max_entities: int = 50
    ) -> Dict[str, Any]:
        """
        Get the neighborhood graph around an entity.

        Args:
            user_id: User identifier
            entity_type: Entity type
            entity_id: Entity ID
            depth: How many hops to explore
            max_entities: Maximum entities to return

        Returns:
            Dictionary with nodes and edges
        """
        nodes: Dict[str, Dict[str, Any]] = {}
        edges: List[Dict[str, Any]] = []
        to_explore: List[Tuple[str, str, int]] = [(entity_type, entity_id, 0)]
        explored: Set[str] = set()

        root_key = f"{entity_type}:{entity_id}"
        nodes[root_key] = {
            "type": entity_type,
            "id": entity_id,
            "is_root": True
        }

        while to_explore and len(nodes) < max_entities:
            current_type, current_id, current_depth = to_explore.pop(0)
            current_key = f"{current_type}:{current_id}"

            if current_key in explored:
                continue
            explored.add(current_key)

            if current_depth >= depth:
                continue

            # Get related edges
            related = await self.repository.get_related(
                user_id=user_id,
                entity_type=current_type,
                entity_id=current_id
            )

            for edge, direction in related:
                # Add edge
                edges.append({
                    "edge_id": edge.edge_id,
                    "source": edge.source_key,
                    "target": edge.target_key,
                    "relation": edge.relation_type.value if isinstance(edge.relation_type, RelationType) else edge.relation_type,
                    "weight": edge.weight
                })

                # Determine neighbor
                if direction == "outgoing":
                    neighbor_type = edge.target_type
                    neighbor_id = edge.target_id
                else:
                    neighbor_type = edge.source_type
                    neighbor_id = edge.source_id

                neighbor_key = f"{neighbor_type}:{neighbor_id}"

                # Add node if not seen
                if neighbor_key not in nodes:
                    nodes[neighbor_key] = {
                        "type": neighbor_type,
                        "id": neighbor_id,
                        "is_root": False
                    }

                    # Schedule for exploration
                    if neighbor_key not in explored:
                        to_explore.append((neighbor_type, neighbor_id, current_depth + 1))

        return {
            "nodes": list(nodes.values()),
            "edges": edges,
            "root": root_key
        }

    async def build_context_from_graph(
        self,
        user_id: str,
        entity_type: str,
        entity_id: str,
        max_related: int = 5
    ) -> Dict[str, Any]:
        """
        Build context information from graph relationships.

        Useful for enriching LLM prompts with relationship context.

        Args:
            user_id: User identifier
            entity_type: Entity type
            entity_id: Entity ID
            max_related: Maximum related entities to include

        Returns:
            Context dictionary for prompts
        """
        related = await self.get_related_entities(
            user_id=user_id,
            entity_type=entity_type,
            entity_id=entity_id,
            direction="both"
        )

        context = {
            "entity": {
                "type": entity_type,
                "id": entity_id
            },
            "related_count": len(related),
            "relationships": []
        }

        for item in related[:max_related]:
            context["relationships"].append({
                "type": item["entity_type"],
                "id": item["entity_id"],
                "relation": item["relation"],
                "direction": item["direction"]
            })

        return context

    async def remove_entity_relationships(
        self,
        source_type: str,
        source_id: str
    ) -> None:
        """
        Remove all relationships for an entity.

        Called when an entity is deleted.

        Args:
            source_type: Entity type
            source_id: Entity ID
        """
        await self.repository.delete_by_entity(source_type, source_id)

        self._logger.info(
            "entity_relationships_removed",
            entity=f"{source_type}:{source_id}"
        )

    async def get_stats(self, user_id: str) -> Dict[str, Any]:
        """
        Get graph statistics for a user.

        Args:
            user_id: User identifier

        Returns:
            Statistics dictionary
        """
        total_edges = await self.repository.count_by_user(user_id)

        return {
            "user_id": user_id,
            "total_edges": total_edges
        }
