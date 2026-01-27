"""File system tools for homelab access."""

import logging
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional

from .ssh_tools import ToolResult
from ..config import get_config

logger = logging.getLogger(__name__)


class FileAccessValidator:
    """Validates file system access against security boundaries."""

    def __init__(self):
        self.config = get_config().homelab

    def _normalize_path(self, path: str) -> Path:
        """Normalize and resolve path, preventing traversal attacks."""
        # Resolve to absolute path
        abs_path = Path(path).resolve()
        return abs_path

    def _is_path_allowed(self, path: Path) -> bool:
        """Check if path is within allowed directories."""
        if not self.config.allowed_dirs:
            logger.warning("No allowed directories configured - rejecting file access")
            return False

        # Check if path is within any allowed directory
        for allowed_dir in self.config.allowed_dirs:
            allowed_path = Path(allowed_dir).resolve()
            try:
                # Check if path is relative to allowed directory
                path.relative_to(allowed_path)
                return True
            except ValueError:
                # Not relative to this allowed directory
                continue

        return False

    def validate_path(self, path: str) -> tuple[bool, Optional[str], Optional[Path]]:
        """
        Validate file path against security boundaries.

        Returns:
            (is_valid, error_message, normalized_path)
        """
        try:
            normalized_path = self._normalize_path(path)
        except Exception as e:
            return False, f"Invalid path: {str(e)}", None

        # Check for path traversal attempts
        if ".." in path:
            return False, "Path traversal detected (..)", None

        # Check if path is allowed
        if not self._is_path_allowed(normalized_path):
            return (
                False,
                f"Path '{path}' is not within allowed directories: {self.config.allowed_dirs}",
                None,
            )

        return True, None, normalized_path


class FileAccessor:
    """File system accessor with security boundaries."""

    def __init__(self):
        self.config = get_config().homelab
        self.validator = FileAccessValidator()

    async def list_files(
        self, path: str, pattern: Optional[str] = None, max_depth: Optional[int] = None
    ) -> ToolResult:
        """
        List files in directory.

        Args:
            path: Directory path to list
            pattern: Optional glob pattern (e.g., "*.py")
            max_depth: Maximum directory depth (default: config.max_file_listing_depth)

        Returns:
            ToolResult with file listing
        """
        # Validate path
        is_valid, error_msg, normalized_path = self.validator.validate_path(path)
        if not is_valid:
            return ToolResult(
                status="error", data={}, citations=[], error_message=error_msg
            )

        # Check if path exists
        if not normalized_path.exists():
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Path does not exist: {path}",
            )

        # Check if path is a directory
        if not normalized_path.is_dir():
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Path is not a directory: {path}",
            )

        depth = max_depth if max_depth is not None else self.config.max_file_listing_depth

        try:
            files = []
            dirs = []

            # List directory contents
            for entry in normalized_path.iterdir():
                if entry.is_file():
                    # Apply pattern filter if specified
                    if pattern and not entry.match(pattern):
                        continue
                    files.append(
                        {
                            "name": entry.name,
                            "path": str(entry),
                            "size": entry.stat().st_size,
                            "modified": entry.stat().st_mtime,
                        }
                    )
                elif entry.is_dir():
                    dirs.append({"name": entry.name, "path": str(entry)})

            return ToolResult(
                status="success",
                data={
                    "path": str(normalized_path),
                    "files": files,
                    "directories": dirs,
                    "total_files": len(files),
                    "total_directories": len(dirs),
                },
                citations=[
                    {
                        "type": "directory_listing",
                        "path": str(normalized_path),
                        "file_count": str(len(files)),
                    }
                ],
            )

        except PermissionError:
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Permission denied: {path}",
            )
        except Exception as e:
            logger.exception(f"Failed to list files in {path}")
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Failed to list files: {str(e)}",
            )

    async def read_file(
        self,
        path: str,
        start_line: Optional[int] = None,
        end_line: Optional[int] = None,
    ) -> ToolResult:
        """
        Read file contents.

        Args:
            path: File path to read
            start_line: Optional start line (1-indexed)
            end_line: Optional end line (1-indexed, inclusive)

        Returns:
            ToolResult with file contents
        """
        # Validate path
        is_valid, error_msg, normalized_path = self.validator.validate_path(path)
        if not is_valid:
            return ToolResult(
                status="error", data={}, citations=[], error_message=error_msg
            )

        # Check if file exists
        if not normalized_path.exists():
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"File does not exist: {path}",
            )

        # Check if path is a file
        if not normalized_path.is_file():
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Path is not a file: {path}",
            )

        # Check file size
        file_size_kb = normalized_path.stat().st_size / 1024
        if file_size_kb > self.config.file_read_limit_kb:
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"File size ({file_size_kb:.1f}KB) exceeds limit ({self.config.file_read_limit_kb}KB)",
            )

        try:
            # Read file
            with open(normalized_path, "r", encoding="utf-8", errors="replace") as f:
                lines = f.readlines()

            # Apply line range if specified
            if start_line is not None or end_line is not None:
                start_idx = (start_line - 1) if start_line else 0
                end_idx = end_line if end_line else len(lines)
                selected_lines = lines[start_idx:end_idx]
                content = "".join(selected_lines)
                line_range = f"{start_idx + 1}-{min(end_idx, len(lines))}"
            else:
                content = "".join(lines)
                line_range = f"1-{len(lines)}"

            return ToolResult(
                status="success",
                data={
                    "path": str(normalized_path),
                    "content": content,
                    "total_lines": len(lines),
                    "line_range": line_range,
                    "size_bytes": normalized_path.stat().st_size,
                },
                citations=[
                    {
                        "type": "file_read",
                        "file": str(normalized_path),
                        "lines": line_range,
                    }
                ],
            )

        except PermissionError:
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Permission denied: {path}",
            )
        except UnicodeDecodeError:
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"File is not a text file: {path}",
            )
        except Exception as e:
            logger.exception(f"Failed to read file {path}")
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Failed to read file: {str(e)}",
            )

    async def search_files(
        self, pattern: str, base_path: Optional[str] = None
    ) -> ToolResult:
        """
        Search for files by name pattern.

        Args:
            pattern: Glob pattern (e.g., "*.py", "config*.yaml")
            base_path: Optional base path to search (default: homelab.base_path)

        Returns:
            ToolResult with matching files
        """
        # Determine search base
        search_base = base_path or self.config.base_path

        # Validate path
        is_valid, error_msg, normalized_path = self.validator.validate_path(search_base)
        if not is_valid:
            return ToolResult(
                status="error", data={}, citations=[], error_message=error_msg
            )

        # Check if path exists
        if not normalized_path.exists():
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Path does not exist: {search_base}",
            )

        try:
            # Search for matching files
            matches = list(normalized_path.rglob(pattern))

            # Filter out non-allowed paths
            allowed_matches = []
            for match in matches:
                is_allowed, _, _ = self.validator.validate_path(str(match))
                if is_allowed:
                    allowed_matches.append(match)

            # Limit results
            limited_matches = allowed_matches[: self.config.search_max_results]

            # Build results
            results = [
                {
                    "name": match.name,
                    "path": str(match),
                    "size": match.stat().st_size if match.is_file() else None,
                    "is_file": match.is_file(),
                }
                for match in limited_matches
            ]

            return ToolResult(
                status="success",
                data={
                    "pattern": pattern,
                    "base_path": str(normalized_path),
                    "matches": results,
                    "total_matches": len(allowed_matches),
                    "limited_to": len(limited_matches),
                },
                citations=[
                    {
                        "type": "file_search",
                        "pattern": pattern,
                        "base_path": str(normalized_path),
                        "match_count": str(len(allowed_matches)),
                    }
                ],
            )

        except Exception as e:
            logger.exception(f"Failed to search files with pattern {pattern}")
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Failed to search files: {str(e)}",
            )


# Global accessor instance
_file_accessor: Optional[FileAccessor] = None


def get_file_accessor() -> FileAccessor:
    """Get or create the global file accessor instance."""
    global _file_accessor
    if _file_accessor is None:
        _file_accessor = FileAccessor()
    return _file_accessor


# Tool functions for registration
async def file_list(path: str, pattern: Optional[str] = None) -> ToolResult:
    """List files in directory."""
    accessor = get_file_accessor()
    return await accessor.list_files(path, pattern)


async def file_read(
    path: str, start_line: Optional[int] = None, end_line: Optional[int] = None
) -> ToolResult:
    """Read file contents."""
    accessor = get_file_accessor()
    return await accessor.read_file(path, start_line, end_line)


async def file_search(pattern: str, base_path: Optional[str] = None) -> ToolResult:
    """Search for files by pattern."""
    accessor = get_file_accessor()
    return await accessor.search_files(pattern, base_path)
