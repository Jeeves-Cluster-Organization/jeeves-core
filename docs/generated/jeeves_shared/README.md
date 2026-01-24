# jeeves_shared Package

**Layer**: L0 (Foundation Layer - Alongside jeeves_protocols)

## Overview

The `jeeves_shared` package provides common utilities that can be used by all layers without creating circular dependencies. It sits at L0 alongside `jeeves_protocols`.

## Package Exports

### Logging

| Export | Description |
|--------|-------------|
| `Logger` | LoggerProtocol implementation backed by structlog |
| `configure_logging` | Configure logging for the Jeeves runtime |
| `create_logger` | Create a logger for dependency injection |
| `create_agent_logger` | Create a logger for an agent |
| `create_capability_logger` | Create a logger for a capability |
| `create_tool_logger` | Create a logger for a tool |
| `get_component_logger` | Get a logger bound to a component name |
| `get_current_logger` | Get current logger for context-based access |
| `set_current_logger` | Set current logger for context-based access |
| `request_scope` | Context manager for request scope |

### Serialization

| Export | Description |
|--------|-------------|
| `parse_datetime` | Parse datetime from various formats |
| `serialize_datetime` | Serialize datetime to ISO format |
| `parse_datetime_field` | Parse datetime field from dictionary |
| `to_json` | Convert Python object to JSON string |
| `from_json` | Convert JSON string to Python object |
| `utc_now` | Get current UTC datetime |
| `utc_now_iso` | Get current UTC time as ISO string |
| `JSONEncoderWithUUID` | JSON encoder handling UUID and datetime |

### UUID Utilities

| Export | Description |
|--------|-------------|
| `uuid_str` | Universal UUID converter |
| `uuid_read` | Database row UUID reader |
| `convert_uuids_to_strings` | Convert all UUIDs in a dict |
| `UUIDStr` | Pydantic type for UUID fields |
| `OptionalUUIDStr` | Optional variant of UUIDStr |

### Fuzzy Matching

| Export | Description |
|--------|-------------|
| `FuzzyMatcher` | Multi-strategy fuzzy text matcher |
| `MatchScore` | Detailed scoring breakdown |
| `fuzzy_match_score` | Simple fuzzy match score function |

### ID Generation

| Export | Description |
|--------|-------------|
| `UUIDGenerator` | UUID-based ID generator |
| `DeterministicIdGenerator` | Deterministic ID generator for testing |
| `SequentialIdGenerator` | Sequential ID generator for debugging |
| `TimestampIdGenerator` | Timestamp-based ID generator |
| `get_id_generator` | Get default ID generator |
| `reset_id_generator` | Reset default ID generator |

### Testing

| Export | Description |
|--------|-------------|
| `is_running_in_docker` | Check if running in Docker container |
| `parse_postgres_url` | Parse PostgreSQL URL into components |

---

## Module Documentation

### Logging (`jeeves_shared.logging`)

Centralized logging infrastructure implementing `LoggerProtocol` from `jeeves_protocols`.

```python
from jeeves_shared import configure_logging, create_logger, get_component_logger

# Configure at startup
configure_logging(level="INFO", json_output=True)

# Create logger for injection
logger = create_logger("my_component", envelope_id="env-123")

# Get component-bound logger
logger = get_component_logger("ChunkService")
logger.info("Processing chunk", chunk_id="chunk-456")
```

**Logger Class**:
```python
class Logger:
    def debug(self, msg: str, **kwargs) -> None: ...
    def info(self, msg: str, **kwargs) -> None: ...
    def warning(self, msg: str, **kwargs) -> None: ...
    def error(self, msg: str, **kwargs) -> None: ...
    def critical(self, msg: str, **kwargs) -> None: ...
    def exception(self, msg: str, **kwargs) -> None: ...
    def bind(self, **kwargs) -> "Logger": ...
```

**Request Scope**:
```python
from jeeves_shared import request_scope
from jeeves_protocols import RequestContext

ctx = RequestContext(request_id="req-123", user_id="user-456")
with request_scope(ctx, logger):
    # All code in this scope has access to context
    process_request()
```

---

### Serialization (`jeeves_shared.serialization`)

Datetime, JSON, and UUID serialization utilities.

```python
from jeeves_shared import parse_datetime, to_json, from_json, utc_now

# Parse various datetime formats
dt = parse_datetime("2025-01-15T10:30:00+00:00")
dt = parse_datetime("2025-01-15T10:30:00Z")
dt = parse_datetime(existing_datetime)

# JSON with UUID/datetime support
json_str = to_json({"id": uuid4(), "created": utc_now()})
data = from_json(json_str)

# Current UTC time
now = utc_now()  # datetime with timezone
now_iso = utc_now_iso()  # "2025-01-15T10:30:00+00:00"
```

---

### UUID Utilities (`jeeves_shared.uuid_utils`)

Consistent UUID handling across the codebase.

```python
from jeeves_shared import uuid_str, uuid_read, UUIDStr

# Universal converter
session_id = uuid_str("test-session-1")  # Deterministic UUID from string
request_id = uuid_str(uuid4())           # UUID object to string
task_id = uuid_str("550e8400-...")       # Already valid UUID - passthrough
parent_id = uuid_str(None)               # None for optional fields

# Database reading
from_db = uuid_read(row["user_id"])      # UUID or string to string

# Pydantic models
from pydantic import BaseModel

class Task(BaseModel):
    task_id: UUIDStr
    parent_id: Optional[UUIDStr] = None
```

---

### ID Generation (`jeeves_shared.id_generator`)

UUID-based ID generation implementing `IdGeneratorProtocol`.

```python
from jeeves_shared import (
    UUIDGenerator,
    DeterministicIdGenerator,
    get_id_generator,
)

# Production use
generator = UUIDGenerator()
id = generator.generate()  # "550e8400-e29b-41d4-a716-446655440000"
prefixed = generator.generate_prefixed("req")  # "req_550e8400..."

# Testing - deterministic IDs
test_gen = DeterministicIdGenerator(seed="test")
id1 = test_gen.generate()  # Same every time with same seed
id2 = test_gen.generate()  # Next in sequence
test_gen.reset()           # Reset for reproducibility

# Default singleton
gen = get_id_generator()
```

**ID Generator Classes**:

| Class | Description | Thread-Safe |
|-------|-------------|-------------|
| `UUIDGenerator` | Random UUIDs for production | Yes |
| `DeterministicIdGenerator` | Reproducible IDs for testing | No |
| `SequentialIdGenerator` | Human-readable sequential IDs | No |
| `TimestampIdGenerator` | Timestamp-prefixed IDs for sorting | Yes |

---

### Fuzzy Matching (`jeeves_shared.fuzzy_matcher`)

Multi-strategy fuzzy text matching.

```python
from jeeves_shared import FuzzyMatcher, fuzzy_match_score

# Simple matching
score = fuzzy_match_score("auth", "authentication")  # 0.0-1.0

# Advanced matching with configuration
matcher = FuzzyMatcher(
    min_score_threshold=0.5,
    substring_weight=1.0,
    word_overlap_weight=0.9,
    char_similarity_weight=0.7,
)

# Score with primary and secondary text
result = matcher.score_match(
    search_query="login auth",
    target_text="User Authentication",
    secondary_text="Handles user login and session management",
)
print(result.total)  # Combined score
print(result.matched_field)  # "title" or "description"

# Find best matches from candidates
candidates = [
    {"title": "Login Handler", "description": "..."},
    {"title": "Auth Service", "description": "..."},
]
matches = matcher.find_best_matches(
    search_query="authentication",
    candidates=candidates,
    primary_field="title",
    secondary_field="description",
    limit=5,
)
for candidate, score in matches:
    print(f"{candidate['title']}: {score}")
```

**Matching Strategies**:
1. **Substring matching** (weight: 1.0) - Exact substring presence
2. **Word-level overlap** (weight: 0.9) - Set intersection of words
3. **Character-level similarity** (weight: 0.7) - SequenceMatcher ratio

---

### Testing Utilities (`jeeves_shared.testing`)

Helpers for test suites.

```python
from jeeves_shared.testing import is_running_in_docker, parse_postgres_url

# Check execution environment
if is_running_in_docker():
    postgres_host = "postgres"  # Docker network hostname
else:
    postgres_host = "localhost"

# Parse database URL for environment setup
url = "postgresql+asyncpg://user:pass@localhost:5432/testdb"
env = parse_postgres_url(url)
# {
#     "POSTGRES_HOST": "localhost",
#     "POSTGRES_PORT": "5432",
#     "POSTGRES_DATABASE": "testdb",
#     "POSTGRES_USER": "user",
#     "POSTGRES_PASSWORD": "pass",
# }
```

---

## Import Examples

### Common Imports

```python
# Logging
from jeeves_shared import (
    Logger,
    configure_logging,
    create_logger,
    get_component_logger,
)

# Serialization
from jeeves_shared import (
    parse_datetime,
    to_json,
    from_json,
    utc_now,
)

# UUID
from jeeves_shared import uuid_str, UUIDStr

# ID Generation
from jeeves_shared import UUIDGenerator, get_id_generator

# Fuzzy Matching
from jeeves_shared import FuzzyMatcher, fuzzy_match_score
```

### Full Package Import

```python
from jeeves_shared import (
    # Logging
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
    # Serialization
    parse_datetime,
    serialize_datetime,
    parse_datetime_field,
    to_json,
    from_json,
    utc_now,
    utc_now_iso,
    JSONEncoderWithUUID,
    # UUID
    uuid_str,
    uuid_read,
    convert_uuids_to_strings,
    UUIDStr,
    OptionalUUIDStr,
    # Fuzzy Matching
    FuzzyMatcher,
    MatchScore,
    fuzzy_match_score,
    # ID Generation
    UUIDGenerator,
    DeterministicIdGenerator,
    SequentialIdGenerator,
    TimestampIdGenerator,
    get_id_generator,
    reset_id_generator,
)
```

---

## Design Principles

1. **Zero Jeeves Dependencies**: Does not import from other Jeeves packages (except `jeeves_protocols`)
2. **Protocol Compliance**: Implements protocols defined in `jeeves_protocols`
3. **Testing Support**: Provides deterministic alternatives for testing
4. **Type Safety**: Full type annotations with Pydantic integration
5. **Constitutional Reference**: Follows Contracts C3 (Dependency Injection)
