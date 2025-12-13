"""Shared utilities for the Jeeves runtime.

This package provides common utilities that can be used by all layers
without creating circular dependencies. It sits at L0 alongside jeeves_protocols.

Exports:
- Logging: get_component_logger, get_current_logger, set_current_logger, JeevesLogger
- Serialization: parse_datetime, to_json, from_json, utc_now
- UUID: uuid_str, uuid_read, convert_uuids_to_strings, UUIDStr, OptionalUUIDStr
"""

from jeeves_shared.logging import (
    JeevesLogger,
    configure_logging,
    create_logger,
    create_agent_logger,
    create_capability_logger,
    create_tool_logger,
    get_component_logger,
    get_current_logger,
    set_current_logger,
    request_scope,
)
from jeeves_shared.serialization import (
    parse_datetime,
    serialize_datetime,
    parse_datetime_field,
    to_json,
    from_json,
    utc_now,
    utc_now_iso,
    JSONEncoderWithUUID,
)
from jeeves_shared.uuid_utils import (
    uuid_str,
    uuid_read,
    convert_uuids_to_strings,
    UUIDStr,
    OptionalUUIDStr,
)

__all__ = [
    # Logging
    "JeevesLogger",
    "configure_logging",
    "create_logger",
    "create_agent_logger",
    "create_capability_logger",
    "create_tool_logger",
    "get_component_logger",
    "get_current_logger",
    "set_current_logger",
    "request_scope",
    # Serialization
    "parse_datetime",
    "serialize_datetime",
    "parse_datetime_field",
    "to_json",
    "from_json",
    "utc_now",
    "utc_now_iso",
    "JSONEncoderWithUUID",
    # UUID
    "uuid_str",
    "uuid_read",
    "convert_uuids_to_strings",
    "UUIDStr",
    "OptionalUUIDStr",
]
