"""Telegram Homelab capability for jeeves-core.

This capability provides Telegram bot integration for homelab management,
supporting SSH command execution, file access, calendar queries, and notes search.
"""

from .bot import TelegramBot, run_bot
from .config import get_config, TelegramHomelabCapabilityConfig
from .servicer import TelegramHomelabServicer
from .wiring import register_capability, wire

__version__ = "0.1.0"

__all__ = [
    "TelegramBot",
    "run_bot",
    "TelegramHomelabServicer",
    "TelegramHomelabCapabilityConfig",
    "get_config",
    "register_capability",
    "wire",
]
