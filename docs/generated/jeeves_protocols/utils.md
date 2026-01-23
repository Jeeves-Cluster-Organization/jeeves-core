# jeeves_protocols.utils

**Layer**: L0 (Foundation)  
**Purpose**: JSON repair, string normalization, datetime helpers

## Overview

This module provides utility functions and classes for common operations. These utilities are L0-safe with no dependencies on other Jeeves packages.

Note: Datetime utilities (`utc_now`, `utc_now_iso`) are also available in `jeeves_shared`. Having them here allows `jeeves_protocols` to remain at L0 without depending on `jeeves_shared`.

---

## JSONRepairKit

Utilities for repairing malformed JSON from LLM outputs.

```python
class JSONRepairKit:
    @staticmethod
    def extract_json(text: str) -> Optional[str]:
        """Extract JSON from text that may contain markdown or other content."""

    @staticmethod
    def repair_json(text: str) -> str:
        """Attempt to repair common JSON issues."""

    @staticmethod
    def parse_lenient(text: str) -> Any:
        """Parse JSON leniently, attempting repairs if needed."""
```

### extract_json

Extracts JSON from text that may contain markdown code blocks or other content.

**Behavior**:
1. Strips leading/trailing whitespace
2. Looks for JSON in code blocks (```json ... ```)
3. Finds balanced JSON objects/arrays by tracking braces
4. Falls back to greedy regex match

**Example**:
```python
from jeeves_protocols import JSONRepairKit

# Extract from markdown
text = """
Here's the result:
```json
{"status": "success", "items": [1, 2, 3]}
```
That's all.
"""
json_str = JSONRepairKit.extract_json(text)
# '{"status": "success", "items": [1, 2, 3]}'

# Extract from mixed content
text = 'The response is {"value": 42} as expected.'
json_str = JSONRepairKit.extract_json(text)
# '{"value": 42}'
```

---

### repair_json

Attempts to repair common JSON issues.

**Repairs**:
1. Trailing commas before `}` or `]`
2. Single quotes to double quotes (if no double quotes present)
3. Unquoted keys to quoted keys

**Example**:
```python
# Fix trailing comma
text = '{"a": 1, "b": 2,}'
fixed = JSONRepairKit.repair_json(text)
# '{"a": 1, "b": 2}'

# Fix single quotes
text = "{'key': 'value'}"
fixed = JSONRepairKit.repair_json(text)
# '{"key": "value"}'

# Fix unquoted keys
text = '{key: "value", count: 42}'
fixed = JSONRepairKit.repair_json(text)
# '{"key": "value", "count": 42}'
```

---

### parse_lenient

Parses JSON leniently, attempting repairs if needed.

**Process**:
1. Try direct `json.loads()`
2. Try extracting JSON first, then parsing
3. Try repairing extracted JSON, then parsing
4. Try repairing original text, then parsing
5. Return `None` if all attempts fail

**Example**:
```python
# Parse valid JSON
result = JSONRepairKit.parse_lenient('{"key": "value"}')
# {"key": "value"}

# Parse from markdown
result = JSONRepairKit.parse_lenient('```json\n{"key": "value"}\n```')
# {"key": "value"}

# Parse with repairs
result = JSONRepairKit.parse_lenient("{'key': 'value',}")
# {"key": "value"}

# Failed parse
result = JSONRepairKit.parse_lenient("not json at all")
# None
```

---

## String Functions

### normalize_string_list

Normalize a value that could be a string, list, or None to a list of strings.

```python
def normalize_string_list(value: Union[str, List[str], None]) -> List[str]:
    """Normalize a value to a list of strings."""
```

**Behavior**:
- `None` → `[]`
- String with commas → split by commas
- String with newlines → split by newlines
- Single string → `[string]`
- List → convert each item to string

**Example**:
```python
from jeeves_protocols import normalize_string_list

normalize_string_list(None)
# []

normalize_string_list("a, b, c")
# ["a", "b", "c"]

normalize_string_list("line1\nline2\nline3")
# ["line1", "line2", "line3"]

normalize_string_list("single")
# ["single"]

normalize_string_list(["a", "b", 42])
# ["a", "b", "42"]
```

---

### truncate_string

Truncate string to max length with suffix.

```python
def truncate_string(text: str, max_length: int = 100, suffix: str = "...") -> str:
    """Truncate string to max length with suffix."""
```

**Example**:
```python
from jeeves_protocols import truncate_string

truncate_string("Hello, World!", max_length=10)
# "Hello, ..."

truncate_string("Short", max_length=10)
# "Short"

truncate_string("Long text here", max_length=10, suffix="…")
# "Long text…"
```

---

## Datetime Functions

### utc_now

Return timezone-aware UTC datetime.

```python
def utc_now() -> datetime:
    """Return timezone-aware UTC datetime."""
```

**Example**:
```python
from jeeves_protocols import utc_now

now = utc_now()
# datetime.datetime(2025, 1, 15, 10, 30, 0, tzinfo=timezone.utc)
```

---

### utc_now_iso

Return current UTC time as ISO format string.

```python
def utc_now_iso() -> str:
    """Return current UTC time as ISO format string."""
```

**Example**:
```python
from jeeves_protocols import utc_now_iso

now_str = utc_now_iso()
# "2025-01-15T10:30:00.123456+00:00"
```

---

### parse_datetime

Parse a datetime value from various formats.

```python
def parse_datetime(value: Any) -> Optional[datetime]:
    """Parse a datetime value from various formats."""
```

**Handles**:
- ISO format strings with "Z" suffix (Zulu/UTC time)
- ISO format strings with timezone offset
- datetime objects (pass-through)
- None values

**Example**:
```python
from jeeves_protocols import parse_datetime

# Parse ISO with Z
dt = parse_datetime("2025-01-15T10:30:00Z")
# datetime.datetime(2025, 1, 15, 10, 30, tzinfo=timezone.utc)

# Parse ISO with offset
dt = parse_datetime("2025-01-15T10:30:00+05:30")

# Pass-through datetime
dt = parse_datetime(existing_datetime)

# None returns None
dt = parse_datetime(None)
# None
```

---

## Usage in Agents

The `JSONRepairKit` is particularly useful for parsing LLM outputs:

```python
from jeeves_protocols.utils import JSONRepairKit

async def _call_llm(self, envelope):
    # Get raw LLM response
    response = await self.llm.generate(model="", prompt=prompt)
    
    # Parse with repair kit for robust handling
    result = JSONRepairKit.parse_lenient(response)
    
    if result is not None:
        return result
    
    # Fallback to raw response
    return {"response": response}
```

---

## Navigation

- [Back to README](README.md)
- [Previous: gRPC](grpc.md)
