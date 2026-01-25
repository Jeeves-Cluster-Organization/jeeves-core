"""Shared utilities for the Jeeves runtime.

This package provides common utilities that can be used by all layers
without creating circular dependencies. It sits at L0 alongside protocols.

Exports:
- Logging: get_component_logger, get_current_logger, set_current_logger, Logger
- Serialization: parse_datetime, to_json, from_json, utc_now
- UUID: uuid_str, uuid_read, convert_uuids_to_strings, UUIDStr, OptionalUUIDStr
"""

from shared.logging import (
    Logger,
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
from shared.serialization import (
    parse_datetime,
    serialize_datetime,
    parse_datetime_field,
    to_json,
    from_json,
    utc_now,
    utc_now_iso,
    JSONEncoderWithUUID,
)
from shared.uuid_utils import (
    uuid_str,
    uuid_read,
    convert_uuids_to_strings,
    UUIDStr,
    OptionalUUIDStr,
)
from shared.fuzzy_matcher import FuzzyMatcher, MatchScore, fuzzy_match_score
from shared.id_generator import (
    UUIDGenerator,
    DeterministicIdGenerator,
    SequentialIdGenerator,
    TimestampIdGenerator,
    get_id_generator,
    reset_id_generator,
)

__all__ = [
    # Logging
    "Logger",
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
    # Fuzzy Matching
    "FuzzyMatcher",
    "MatchScore",
    "fuzzy_match_score",
    # ID Generation
    "UUIDGenerator",
    "DeterministicIdGenerator",
    "SequentialIdGenerator",
    "TimestampIdGenerator",
    "get_id_generator",
    "reset_id_generator",
]
