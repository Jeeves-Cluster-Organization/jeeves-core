"""Notes tools for homelab integration."""

import logging
from pathlib import Path
from typing import List, Optional

from .ssh_tools import ToolResult
from ..config import get_config

logger = logging.getLogger(__name__)


class NotesAccessor:
    """Notes accessor supporting multiple backends."""

    def __init__(self):
        self.config = get_config().notes

    async def search(self, query: str, limit: Optional[int] = None) -> ToolResult:
        """
        Search notes by keyword.

        Args:
            query: Search query string
            limit: Maximum number of results (default: config.search_max_results)

        Returns:
            ToolResult with matching notes
        """
        max_results = limit if limit is not None else self.config.search_max_results

        # Route to appropriate backend
        if self.config.backend == "filesystem":
            return await self._search_filesystem(query, max_results)
        elif self.config.backend == "sqlite":
            return await self._search_sqlite(query, max_results)
        elif self.config.backend == "postgresql":
            return await self._search_postgresql(query, max_results)
        else:
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Unsupported notes backend: {self.config.backend}",
            )

    async def _search_filesystem(self, query: str, max_results: int) -> ToolResult:
        """Search notes in filesystem."""
        notes_path = Path(self.config.notes_path)

        if not notes_path.exists():
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Notes path does not exist: {self.config.notes_path}",
            )

        if not notes_path.is_dir():
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Notes path is not a directory: {self.config.notes_path}",
            )

        try:
            matches = []
            query_lower = query.lower()

            # Search for matching files
            for ext in self.config.supported_extensions:
                pattern = f"**/*{ext}"
                for note_file in notes_path.glob(pattern):
                    try:
                        # Read file content
                        with open(note_file, "r", encoding="utf-8", errors="replace") as f:
                            content = f.read()

                        # Check if query matches in filename or content
                        filename_match = query_lower in note_file.name.lower()
                        content_match = query_lower in content.lower()

                        if filename_match or content_match:
                            # Find matching snippet
                            snippet = self._extract_snippet(content, query, self.config.max_note_preview_chars)

                            matches.append(
                                {
                                    "title": note_file.stem,
                                    "path": str(note_file),
                                    "snippet": snippet,
                                    "size": note_file.stat().st_size,
                                    "modified": note_file.stat().st_mtime,
                                    "match_in_title": filename_match,
                                    "match_in_content": content_match,
                                }
                            )

                            # Stop if we have enough results
                            if len(matches) >= max_results:
                                break

                    except Exception as e:
                        logger.warning(f"Failed to read note {note_file}: {e}")
                        continue

                if len(matches) >= max_results:
                    break

            return ToolResult(
                status="success",
                data={
                    "query": query,
                    "notes_path": str(notes_path),
                    "matches": matches,
                    "total_matches": len(matches),
                },
                citations=[
                    {
                        "type": "notes_search",
                        "backend": "filesystem",
                        "query": query,
                        "match_count": str(len(matches)),
                    }
                ],
            )

        except Exception as e:
            logger.exception(f"Failed to search notes for query: {query}")
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Failed to search notes: {str(e)}",
            )

    def _extract_snippet(self, content: str, query: str, max_chars: int) -> str:
        """Extract relevant snippet from content around query match."""
        query_lower = query.lower()
        content_lower = content.lower()

        # Find query position
        query_pos = content_lower.find(query_lower)

        if query_pos == -1:
            # Query not in content, return beginning
            return content[:max_chars] + ("..." if len(content) > max_chars else "")

        # Calculate snippet bounds
        snippet_start = max(0, query_pos - max_chars // 2)
        snippet_end = min(len(content), query_pos + len(query) + max_chars // 2)

        # Extract snippet
        snippet = content[snippet_start:snippet_end]

        # Add ellipsis
        if snippet_start > 0:
            snippet = "..." + snippet
        if snippet_end < len(content):
            snippet = snippet + "..."

        return snippet

    async def _search_sqlite(self, query: str, max_results: int) -> ToolResult:
        """Search notes in SQLite database."""
        return ToolResult(
            status="error",
            data={},
            citations=[],
            error_message="SQLite backend not yet implemented. Configure NOTES_BACKEND=filesystem for now.",
        )

    async def _search_postgresql(self, query: str, max_results: int) -> ToolResult:
        """Search notes in PostgreSQL database."""
        return ToolResult(
            status="error",
            data={},
            citations=[],
            error_message="PostgreSQL backend not yet implemented. Configure NOTES_BACKEND=filesystem for now.",
        )


# Global accessor instance
_notes_accessor: Optional[NotesAccessor] = None


def get_notes_accessor() -> NotesAccessor:
    """Get or create the global notes accessor instance."""
    global _notes_accessor
    if _notes_accessor is None:
        _notes_accessor = NotesAccessor()
    return _notes_accessor


# Tool function for registration
async def notes_search(query: str, limit: Optional[int] = None) -> ToolResult:
    """
    Search notes by keyword.

    Args:
        query: Search query string
        limit: Maximum number of results
    """
    accessor = get_notes_accessor()
    return await accessor.search(query, limit)
