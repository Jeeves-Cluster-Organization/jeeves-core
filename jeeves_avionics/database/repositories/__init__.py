"""Repository pattern implementations for data access."""

from jeeves_avionics.database.repositories.base import BaseRepository
from jeeves_avionics.database.repositories.task_repository import TaskRepository

__all__ = ["BaseRepository", "TaskRepository"]
