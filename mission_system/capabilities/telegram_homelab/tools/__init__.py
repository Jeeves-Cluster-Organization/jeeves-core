"""Tools module for Telegram Homelab capability."""

from .calendar_tools import calendar_query, get_calendar_accessor
from .file_tools import file_list, file_read, file_search, get_file_accessor
from .notes_tools import get_notes_accessor, notes_search
from .ssh_tools import get_ssh_executor, ssh_execute, ToolResult

__all__ = [
    "ToolResult",
    # SSH tools
    "ssh_execute",
    "get_ssh_executor",
    # File tools
    "file_list",
    "file_read",
    "file_search",
    "get_file_accessor",
    # Calendar tools
    "calendar_query",
    "get_calendar_accessor",
    # Notes tools
    "notes_search",
    "get_notes_accessor",
]
