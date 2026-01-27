"""Configuration models for Telegram Homelab capability."""

from dataclasses import dataclass, field
from typing import List, Optional


@dataclass
class TelegramConfig:
    """Telegram bot configuration."""

    bot_token: str
    admin_user_ids: List[int] = field(default_factory=list)
    webhook_url: Optional[str] = None
    max_message_length: int = 4096
    polling_timeout: int = 30
    allowed_updates: List[str] = field(
        default_factory=lambda: ["message", "callback_query"]
    )


@dataclass
class SSHConfig:
    """SSH connection configuration."""

    hosts: List[str] = field(default_factory=list)
    private_key_path: Optional[str] = None
    known_hosts_path: Optional[str] = None
    default_user: str = "root"
    timeout_seconds: int = 30
    max_output_chars: int = 8000
    strict_host_key_checking: bool = True


@dataclass
class HomelabConfig:
    """Homelab file system configuration."""

    base_path: str = "/home/homelab"
    allowed_dirs: List[str] = field(default_factory=list)
    file_read_limit_kb: int = 500
    search_max_results: int = 50
    max_file_listing_depth: int = 3


@dataclass
class CalendarConfig:
    """Calendar integration configuration."""

    api_type: str = "ics"  # ics, caldav, google
    calendar_url: Optional[str] = None
    caldav_url: Optional[str] = None
    caldav_username: Optional[str] = None
    caldav_password: Optional[str] = None
    google_credentials_path: Optional[str] = None
    max_events_per_query: int = 50
    default_days_ahead: int = 7


@dataclass
class NotesConfig:
    """Notes system configuration."""

    backend: str = "filesystem"  # filesystem, sqlite, postgresql
    notes_path: str = "/home/homelab/notes"
    search_max_results: int = 20
    max_note_preview_chars: int = 500
    supported_extensions: List[str] = field(
        default_factory=lambda: [".md", ".txt", ".org"]
    )
    database_url: Optional[str] = None


@dataclass
class TelegramHomelabCapabilityConfig:
    """Main configuration container for Telegram Homelab capability."""

    telegram: TelegramConfig
    ssh: SSHConfig
    homelab: HomelabConfig
    calendar: CalendarConfig
    notes: NotesConfig
    enable_confirmations: bool = True
    max_concurrent_requests: int = 5
