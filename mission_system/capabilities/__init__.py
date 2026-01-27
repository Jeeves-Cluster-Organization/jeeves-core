"""Capabilities module for jeeves-core.

This module contains capability implementations that can be registered
with jeeves-core.
"""

# Import capabilities for easy access
try:
    from . import telegram_homelab
except ImportError:
    telegram_homelab = None

__all__ = ["telegram_homelab"]
