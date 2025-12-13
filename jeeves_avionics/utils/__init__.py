"""Avionics utilities module.

Centralized utilities for:
- Serialization (datetime, JSON, UUID)
- Error handling utilities
- Database constants
"""

from jeeves_shared.serialization import (
    parse_datetime,
    serialize_datetime,
    parse_datetime_field,
    JSONEncoderWithUUID,
    to_json,
    from_json,
    utc_now,
    utc_now_iso,
)

from jeeves_avionics.utils.error_utils import (
    ErrorFormatter,
    SafeExecutor,
    create_error_response,
    enrich_error_with_suggestions,
)

__all__ = [
    # Datetime utilities
    "parse_datetime",
    "serialize_datetime",
    "parse_datetime_field",
    "utc_now",
    "utc_now_iso",
    # JSON utilities
    "JSONEncoderWithUUID",
    "to_json",
    "from_json",
    # Error utilities
    "ErrorFormatter",
    "SafeExecutor",
    "create_error_response",
    "enrich_error_with_suggestions",
]
