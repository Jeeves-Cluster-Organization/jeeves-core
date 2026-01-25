# cmd/ - Command Line Tools

> **Layer:** Go CLI binaries for Python-Go interop

[← Back to Index](../INDEX.md)

---

## Overview

The `cmd/` directory contains Go CLI binaries that enable Python-Go hybrid execution via subprocess-based interop. These binaries are designed to be invoked by Python code, reading JSON from stdin and writing JSON to stdout.

---

## Directory Structure

```
cmd/
└── envelope/
    ├── main.go       # CLI implementation
    └── main_test.go  # CLI tests
```

---

## envelope CLI

**File:** `cmd/envelope/main.go`

The envelope CLI provides envelope operations for Python code to invoke Go's authoritative envelope handling.

### Installation

```bash
# Build the binary
go build -o go-envelope ./cmd/envelope

# Or use the Makefile
make build-cli
```

### Commands

| Command | Description | Input | Output |
|---------|-------------|-------|--------|
| `create` | Create a new envelope | Creation params | Full envelope state |
| `process` | Validate and transform envelope | Envelope state | Processed envelope state |
| `can-continue` | Check if envelope can continue | Envelope state | Continuation status |
| `result` | Get envelope result dictionary | Envelope state | Result dict |
| `validate` | Validate envelope JSON structure | Envelope state | Validation result |
| `version` | Print version information | None | Version info |

### Usage Examples

#### Create Envelope

```bash
echo '{"raw_input":"Analyze this code","user_id":"user-123","session_id":"sess-456"}' | go-envelope create
```

**Output:**
```json
{
  "envelope_id": "env-abc123",
  "request_id": "req-def456",
  "user_id": "user-123",
  "session_id": "sess-456",
  "raw_input": "Analyze this code",
  "current_stage": "start",
  "iteration": 0,
  "llm_call_count": 0,
  "agent_hop_count": 0,
  ...
}
```

#### Check Continuation

```bash
cat envelope.json | go-envelope can-continue
```

**Output:**
```json
{
  "can_continue": true,
  "terminal_reason": null,
  "iteration": 1,
  "llm_call_count": 3,
  "agent_hop_count": 5
}
```

#### Get Result

```bash
cat envelope.json | go-envelope result
```

**Output:**
```json
{
  "status": "success",
  "final_output": {...},
  "processing_history": [...]
}
```

#### Validate Structure

```bash
echo '{"envelope_id":"test"}' | go-envelope validate
```

**Output:**
```json
{
  "valid": true,
  "errors": [],
  "envelope_id": "test"
}
```

---

## Python Integration

The CLI is invoked by `avionics.interop.go_bridge.GoEnvelopeBridge`:

```python
from avionics.interop.go_bridge import GoEnvelopeBridge

bridge = GoEnvelopeBridge()

# Create envelope via Go
envelope = bridge.create_envelope(
    raw_input="User query",
    user_id="user-123",
    session_id="sess-456"
)

# Check bounds via Go (authoritative)
can_continue = bridge.can_continue(envelope)

# Get result via Go
result = bridge.get_result(envelope)
```

---

## Error Handling

All errors return JSON with standard format:

```json
{
  "error": true,
  "code": "parse_error",
  "message": "Invalid JSON: unexpected end of input"
}
```

### Error Codes

| Code | Description |
|------|-------------|
| `read_error` | Failed to read from stdin |
| `parse_error` | Invalid JSON input |
| `bounds_exceeded` | Envelope cannot continue (bounds violated) |

---

## Version Information

```bash
go-envelope version
```

```json
{
  "version": "1.0.0",
  "build_time": "2025-12-10",
  "go_version": "1.21+"
}
```

---

## Key Functions

### `main()`
Entry point that dispatches to command handlers.

### `handleCreate()`
Creates a new Envelope from input parameters.

**Input Fields:**
- `raw_input` (string): User's input text
- `user_id` (string): User identifier
- `session_id` (string): Session identifier
- `request_id` (string, optional): Custom request ID
- `metadata` (object, optional): Additional metadata
- `stage_order` (array, optional): Pipeline stage order

### `handleProcess()`
Parses envelope from state dict, validates bounds, and returns processed state.

### `handleCanContinue()`
Checks if envelope can continue processing. Returns:
- `can_continue`: Boolean
- `terminal_reason`: Why stopped (if applicable)
- `iteration`: Current iteration count
- `llm_call_count`: Total LLM calls made
- `agent_hop_count`: Total agent transitions

### `handleResult()`
Returns the envelope's result dictionary for final output.

### `handleValidate()`
Validates envelope JSON structure without processing.

---

## Related Documentation

- [coreengine/envelope.md](../coreengine/envelope.md) - Envelope structure
- [avionics/infrastructure.md](../avionics/infrastructure.md) - Go bridge

---

[← Back to Index](../INDEX.md)
