"""Wiring entry point for Telegram Homelab capability."""

import logging
from typing import Any, Callable, Dict, Optional

from protocols.capability import DomainServiceConfig
from mission_system.config.registry import get_config_registry

from .config import get_config, TelegramHomelabCapabilityConfig
from .servicer import TelegramHomelabServicer
from .tools import (
    ssh_execute,
    file_list,
    file_read,
    file_search,
    calendar_query,
    notes_search,
)

logger = logging.getLogger(__name__)


def register_capability() -> None:
    """
    Register Telegram Homelab capability with jeeves-core.

    This function is called during bootstrap to wire up the capability.
    """
    try:
        # Load configuration
        config = get_config()

        # Register configuration with config registry
        config_registry = get_config_registry()
        config_registry.register("telegram_homelab", config)

        # Create service configuration
        service_config = DomainServiceConfig(
            service_id="telegram_homelab",
            service_type="flow",
            capabilities=[
                "ssh_command",
                "file_access",
                "calendar_query",
                "notes_search",
            ],
            max_concurrent=config.max_concurrent_requests,
            is_default=False,
            is_readonly=False,  # Can execute commands and modify files
            requires_confirmation=config.enable_confirmations,
            default_session_title="Telegram Homelab Session",
            pipeline_stages=[
                "perception",
                "intent",
                "planner",
                "traverser",
                "synthesizer",
                "critic",
                "integration",
            ],
        )

        # Create orchestrator factory
        def orchestrator_factory(llm_provider=None, **kwargs) -> TelegramHomelabServicer:
            """Factory to create TelegramHomelabServicer instance."""
            return TelegramHomelabServicer(llm_provider=llm_provider)

        # Create tools initializer
        def tools_initializer() -> Dict[str, Any]:
            """Initialize and register tools for this capability."""
            return {
                "ssh_execute": {
                    "func": ssh_execute,
                    "description": "Execute SSH command on homelab server",
                    "parameters": {
                        "hostname": "string (server hostname)",
                        "command": "string (shell command to execute)",
                        "user": "string (SSH user, optional)",
                        "timeout": "integer (timeout in seconds, optional)",
                    },
                    "category": "composite",
                    "risk_level": "destructive",
                },
                "file_list": {
                    "func": file_list,
                    "description": "List files in directory",
                    "parameters": {
                        "path": "string (directory path)",
                        "pattern": "string (glob pattern, optional)",
                    },
                    "category": "standalone",
                    "risk_level": "read",
                },
                "file_read": {
                    "func": file_read,
                    "description": "Read file contents",
                    "parameters": {
                        "path": "string (file path)",
                        "start_line": "integer (start line, optional)",
                        "end_line": "integer (end line, optional)",
                    },
                    "category": "standalone",
                    "risk_level": "read",
                },
                "file_search": {
                    "func": file_search,
                    "description": "Search for files by pattern",
                    "parameters": {
                        "pattern": "string (glob pattern)",
                        "base_path": "string (base search path, optional)",
                    },
                    "category": "standalone",
                    "risk_level": "read",
                },
                "calendar_query": {
                    "func": calendar_query,
                    "description": "Query calendar events",
                    "parameters": {
                        "start_date": "string (YYYY-MM-DD, optional)",
                        "end_date": "string (YYYY-MM-DD, optional)",
                        "filter": "string (event filter, optional)",
                    },
                    "category": "standalone",
                    "risk_level": "read",
                },
                "notes_search": {
                    "func": notes_search,
                    "description": "Search notes by keyword",
                    "parameters": {
                        "query": "string (search query)",
                        "limit": "integer (max results, optional)",
                    },
                    "category": "standalone",
                    "risk_level": "read",
                },
            }

        # Register with capability system
        from mission_system.capability_wiring import register_capability as register_cap

        register_cap(
            capability_id="telegram_homelab",
            service_config=service_config,
            orchestrator_factory=orchestrator_factory,
            tools_initializer=tools_initializer,
        )

        logger.info("Telegram Homelab capability registered successfully")

    except Exception as e:
        logger.exception(f"Failed to register Telegram Homelab capability: {e}")
        raise


def wire() -> None:
    """
    Wire up the Telegram Homelab capability.

    This is the main entry point called by the capability discovery system.
    """
    register_capability()


# For backward compatibility
__all__ = ["register_capability", "wire"]
