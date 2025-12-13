"""
Base repository interface for data access layer.

This module provides the Repository pattern to abstract database operations,
making code more testable and reducing direct database coupling.
"""

from abc import ABC, abstractmethod
from typing import Generic, TypeVar, Optional, List, Dict, Any, TYPE_CHECKING
from datetime import datetime

from jeeves_avionics.logging import get_current_logger

if TYPE_CHECKING:
    from jeeves_protocols import LoggerProtocol

# Generic type for domain models
T = TypeVar('T')


class BaseRepository(ABC, Generic[T]):
    """
    Abstract base repository providing common CRUD operations.

    Subclasses implement domain-specific repositories (e.g., TaskRepository).
    """

    def __init__(
        self,
        db_client,
        table_name: str,
        logger: Optional["LoggerProtocol"] = None,
    ):
        """Initialize repository.

        Args:
            db_client: Database client instance
            table_name: Name of database table
            logger: Logger instance (ADR-001 DI, uses context logger if not provided)
        """
        self.db = db_client
        self.table_name = table_name
        self.logger = (logger or get_current_logger()).bind(repository=self.__class__.__name__)

    @abstractmethod
    async def find_by_id(self, id: str) -> Optional[T]:
        """Find entity by ID.

        Args:
            id: Entity ID

        Returns:
            Entity or None if not found
        """
        pass

    @abstractmethod
    async def find_all(self, limit: Optional[int] = None, offset: int = 0) -> List[T]:
        """Find all entities.

        Args:
            limit: Maximum number of results
            offset: Number of results to skip

        Returns:
            List of entities
        """
        pass

    @abstractmethod
    async def save(self, entity: T) -> T:
        """Save entity (insert or update).

        Args:
            entity: Entity to save

        Returns:
            Saved entity with updated fields
        """
        pass

    @abstractmethod
    async def delete(self, id: str) -> bool:
        """Delete entity by ID.

        Args:
            id: Entity ID

        Returns:
            True if deleted, False if not found
        """
        pass

    @abstractmethod
    async def exists(self, id: str) -> bool:
        """Check if entity exists.

        Args:
            id: Entity ID

        Returns:
            True if exists, False otherwise
        """
        pass

    @abstractmethod
    async def count(self, filters: Optional[Dict[str, Any]] = None) -> int:
        """Count entities matching filters.

        Args:
            filters: Optional filter criteria

        Returns:
            Count of matching entities
        """
        pass

    # Common helper methods

    def _add_timestamps(self, data: Dict[str, Any], is_update: bool = False) -> Dict[str, Any]:
        """Add timestamp fields to data.

        Args:
            data: Data dictionary
            is_update: Whether this is an update operation

        Returns:
            Data with timestamps
        """
        now = datetime.now().isoformat()

        if not is_update and "created_at" not in data:
            data["created_at"] = now

        data["updated_at"] = now
        return data

    def _build_where_clause(self, filters: Dict[str, Any]) -> tuple:
        """Build WHERE clause from filters.

        Args:
            filters: Filter dictionary

        Returns:
            Tuple of (where_clause, params)
        """
        if not filters:
            return "", {}

        conditions = []
        params = {}

        for key, value in filters.items():
            param_name = f"filter_{key}"
            if value is None:
                conditions.append(f"{key} IS NULL")
            else:
                conditions.append(f"{key} = :{param_name}")
                params[param_name] = value

        where_clause = " AND ".join(conditions)
        return where_clause, params

    async def _execute_query(self, query: str, params: Dict[str, Any] = None) -> List[Dict]:
        """Execute query and return results.

        Args:
            query: SQL query
            params: Query parameters

        Returns:
            List of result dictionaries
        """
        try:
            result = await self.db.query(query, params or {})
            return result
        except Exception as e:
            self.logger.error("query_failed", query=query, error=str(e))
            raise

    async def _execute_command(self, command: str, params: Dict[str, Any] = None) -> int:
        """Execute command and return affected rows.

        Args:
            command: SQL command
            params: Command parameters

        Returns:
            Number of affected rows
        """
        try:
            result = await self.db.execute(command, params or {})
            return result
        except Exception as e:
            self.logger.error("command_failed", command=command, error=str(e))
            raise


class ReadOnlyRepository(BaseRepository[T]):
    """
    Read-only repository for views or external data sources.
    """

    async def save(self, entity: T) -> T:
        """Not supported in read-only repository."""
        raise NotImplementedError("Save not supported in read-only repository")

    async def delete(self, id: str) -> bool:
        """Not supported in read-only repository."""
        raise NotImplementedError("Delete not supported in read-only repository")


class CachedRepository(BaseRepository[T]):
    """
    Repository with caching support.

    Wraps another repository and adds caching layer.
    """

    def __init__(self, inner_repository: BaseRepository[T], cache_ttl: int = 300):
        """Initialize cached repository.

        Args:
            inner_repository: Repository to wrap
            cache_ttl: Cache TTL in seconds
        """
        self.inner = inner_repository
        self.cache = {}
        self.cache_ttl = cache_ttl
        self.logger = logger.bind(repository="CachedRepository")

    async def find_by_id(self, id: str) -> Optional[T]:
        """Find by ID with caching."""
        cache_key = f"id:{id}"

        if cache_key in self.cache:
            entry = self.cache[cache_key]
            if datetime.now().timestamp() - entry["timestamp"] < self.cache_ttl:
                self.logger.debug("cache_hit", id=id)
                return entry["value"]

        result = await self.inner.find_by_id(id)

        if result:
            self.cache[cache_key] = {
                "value": result,
                "timestamp": datetime.now().timestamp()
            }

        return result

    async def save(self, entity: T) -> T:
        """Save and invalidate cache."""
        result = await self.inner.save(entity)

        # Invalidate cache
        entity_id = getattr(entity, "id", None) or getattr(entity, "task_id", None)
        if entity_id:
            cache_key = f"id:{entity_id}"
            self.cache.pop(cache_key, None)

        return result

    async def delete(self, id: str) -> bool:
        """Delete and invalidate cache."""
        result = await self.inner.delete(id)

        # Invalidate cache
        cache_key = f"id:{id}"
        self.cache.pop(cache_key, None)

        return result

    async def find_all(self, limit: Optional[int] = None, offset: int = 0) -> List[T]:
        """Find all (no caching for list operations)."""
        return await self.inner.find_all(limit, offset)

    async def exists(self, id: str) -> bool:
        """Check existence (no caching)."""
        return await self.inner.exists(id)

    async def count(self, filters: Optional[Dict[str, Any]] = None) -> int:
        """Count (no caching)."""
        return await self.inner.count(filters)
