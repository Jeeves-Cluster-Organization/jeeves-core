"""Configuration loader for Telegram Homelab capability."""

import json
import os
from typing import Optional

from .models import (
    CalendarConfig,
    HomelabConfig,
    NotesConfig,
    SSHConfig,
    TelegramConfig,
    TelegramHomelabCapabilityConfig,
)


def load_telegram_config() -> TelegramConfig:
    """Load Telegram configuration from environment variables."""
    bot_token = os.getenv("TELEGRAM_BOT_TOKEN", "")
    if not bot_token:
        raise ValueError("TELEGRAM_BOT_TOKEN environment variable is required")

    admin_ids_str = os.getenv("TELEGRAM_ADMIN_USER_IDS", "")
    admin_ids = (
        [int(x.strip()) for x in admin_ids_str.split(",") if x.strip()]
        if admin_ids_str
        else []
    )

    return TelegramConfig(
        bot_token=bot_token,
        admin_user_ids=admin_ids,
        webhook_url=os.getenv("TELEGRAM_WEBHOOK_URL"),
        max_message_length=int(os.getenv("TELEGRAM_MAX_MESSAGE_LENGTH", "4096")),
        polling_timeout=int(os.getenv("TELEGRAM_POLLING_TIMEOUT", "30")),
    )


def load_ssh_config() -> SSHConfig:
    """Load SSH configuration from environment variables."""
    hosts_str = os.getenv("SSH_HOSTS", "[]")
    try:
        hosts = json.loads(hosts_str)
    except json.JSONDecodeError:
        hosts = []

    return SSHConfig(
        hosts=hosts,
        private_key_path=os.getenv("SSH_PRIVATE_KEY_PATH"),
        known_hosts_path=os.getenv("SSH_KNOWN_HOSTS_PATH"),
        default_user=os.getenv("SSH_DEFAULT_USER", "root"),
        timeout_seconds=int(os.getenv("SSH_TIMEOUT_SECONDS", "30")),
        max_output_chars=int(os.getenv("SSH_MAX_OUTPUT_CHARS", "8000")),
        strict_host_key_checking=os.getenv("SSH_STRICT_HOST_KEY_CHECKING", "true").lower() == "true",
    )


def load_homelab_config() -> HomelabConfig:
    """Load homelab configuration from environment variables."""
    allowed_dirs_str = os.getenv("HOMELAB_ALLOWED_DIRS", "[]")
    try:
        allowed_dirs = json.loads(allowed_dirs_str)
    except json.JSONDecodeError:
        allowed_dirs = []

    return HomelabConfig(
        base_path=os.getenv("HOMELAB_BASE_PATH", "/home/homelab"),
        allowed_dirs=allowed_dirs,
        file_read_limit_kb=int(os.getenv("HOMELAB_FILE_READ_LIMIT_KB", "500")),
        search_max_results=int(os.getenv("HOMELAB_SEARCH_MAX_RESULTS", "50")),
        max_file_listing_depth=int(os.getenv("HOMELAB_MAX_FILE_LISTING_DEPTH", "3")),
    )


def load_calendar_config() -> CalendarConfig:
    """Load calendar configuration from environment variables."""
    return CalendarConfig(
        api_type=os.getenv("CALENDAR_API_TYPE", "ics"),
        calendar_url=os.getenv("CALENDAR_URL"),
        caldav_url=os.getenv("CALDAV_URL"),
        caldav_username=os.getenv("CALDAV_USERNAME"),
        caldav_password=os.getenv("CALDAV_PASSWORD"),
        google_credentials_path=os.getenv("GOOGLE_CALENDAR_CREDENTIALS_PATH"),
        max_events_per_query=int(os.getenv("CALENDAR_MAX_EVENTS_PER_QUERY", "50")),
        default_days_ahead=int(os.getenv("CALENDAR_DEFAULT_DAYS_AHEAD", "7")),
    )


def load_notes_config() -> NotesConfig:
    """Load notes configuration from environment variables."""
    extensions_str = os.getenv("NOTES_SUPPORTED_EXTENSIONS", ".md,.txt,.org")
    extensions = [ext.strip() for ext in extensions_str.split(",") if ext.strip()]

    return NotesConfig(
        backend=os.getenv("NOTES_BACKEND", "filesystem"),
        notes_path=os.getenv("NOTES_PATH", "/home/homelab/notes"),
        search_max_results=int(os.getenv("NOTES_SEARCH_MAX_RESULTS", "20")),
        max_note_preview_chars=int(os.getenv("NOTES_MAX_NOTE_PREVIEW_CHARS", "500")),
        supported_extensions=extensions,
        database_url=os.getenv("NOTES_DATABASE_URL"),
    )


def load_capability_config() -> TelegramHomelabCapabilityConfig:
    """Load complete capability configuration."""
    return TelegramHomelabCapabilityConfig(
        telegram=load_telegram_config(),
        ssh=load_ssh_config(),
        homelab=load_homelab_config(),
        calendar=load_calendar_config(),
        notes=load_notes_config(),
        enable_confirmations=os.getenv("TELEGRAM_HOMELAB_ENABLE_CONFIRMATIONS", "true").lower() == "true",
        max_concurrent_requests=int(os.getenv("TELEGRAM_HOMELAB_MAX_CONCURRENT_REQUESTS", "5")),
    )


# Global configuration instance
_config: Optional[TelegramHomelabCapabilityConfig] = None


def get_config() -> TelegramHomelabCapabilityConfig:
    """Get or create the global configuration instance."""
    global _config
    if _config is None:
        _config = load_capability_config()
    return _config


def reset_config() -> None:
    """Reset the global configuration instance (mainly for testing)."""
    global _config
    _config = None
