# jeeves_protocols.enums

**Layer**: L0 (Foundation)  
**Purpose**: Core enums and constants - mirrors Go definitions

## Overview

This module provides fundamental enums and types used across the entire Jeeves system. These definitions mirror the Go implementations in `coreengine/` for cross-language compatibility.

## Enums

### RiskLevel

Risk level for tool execution with both semantic and severity categories.

```python
class RiskLevel(str, Enum):
    # Semantic levels (used by tool definitions)
    READ_ONLY = "read_only"    # Safe read operations, no side effects
    WRITE = "write"            # Operations that modify state
    DESTRUCTIVE = "destructive" # Operations that delete data or are irreversible
    
    # Severity levels (used by risk assessment)
    LOW = "low"               # Minimal risk
    MEDIUM = "medium"         # Moderate risk, may need review
    HIGH = "high"             # Significant risk, requires confirmation
    CRITICAL = "critical"     # Highest risk, requires explicit approval
```

**Class Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `requires_confirmation` | `(level: RiskLevel) -> bool` | Check if a risk level requires user confirmation. Returns `True` for `DESTRUCTIVE`, `HIGH`, `CRITICAL` |

**Example**:
```python
from jeeves_protocols import RiskLevel

if RiskLevel.requires_confirmation(RiskLevel.DESTRUCTIVE):
    await request_user_confirmation()
```

---

### ToolCategory

Tool categorization for both operation types and tool organization.

```python
class ToolCategory(str, Enum):
    # Operation types
    READ = "read"           # Read-only operations
    WRITE = "write"         # Modification operations
    EXECUTE = "execute"     # Code/command execution
    NETWORK = "network"     # Network operations
    SYSTEM = "system"       # System-level operations
    
    # Tool organization (for code analysis capability)
    UNIFIED = "unified"         # Single entry point tools (e.g., analyze)
    COMPOSITE = "composite"     # Multi-step orchestration tools
    RESILIENT = "resilient"     # Tools with retry/fallback strategies
    STANDALONE = "standalone"   # Independent utility tools
    INTERNAL = "internal"       # Internal tools not exposed to agents
```

---

### ToolAccess

Tool access levels for agents.

```python
class ToolAccess(str, Enum):
    NONE = "none"    # No tool access
    READ = "read"    # Read-only tools
    WRITE = "write"  # Read and write tools
    ALL = "all"      # Full tool access
```

---

### OperationStatus

Status of an operation result.

```python
class OperationStatus(str, Enum):
    # Core statuses
    SUCCESS = "success"                     # Operation completed successfully
    ERROR = "error"                         # Operation failed with error
    NOT_FOUND = "not_found"                 # Resource/target not found (valid outcome)
    TIMEOUT = "timeout"                     # Operation timed out
    VALIDATION_ERROR = "validation_error"   # Input validation failed
    
    # Extended statuses (for tool operations)
    PARTIAL = "partial"                     # Some results found, but incomplete
    INVALID_PARAMETERS = "invalid_parameters" # Semantic misuse
```

---

### HealthStatus

Health check status for services.

```python
class HealthStatus(str, Enum):
    HEALTHY = "healthy"
    DEGRADED = "degraded"
    UNHEALTHY = "unhealthy"
```

---

### TerminalReason

Reasons for envelope termination.

```python
class TerminalReason(str, Enum):
    COMPLETED = "completed"
    MAX_ITERATIONS_EXCEEDED = "max_iterations_exceeded"
    MAX_LLM_CALLS_EXCEEDED = "max_llm_calls_exceeded"
    MAX_AGENT_HOPS_EXCEEDED = "max_agent_hops_exceeded"
    USER_CANCELLED = "user_cancelled"
    TOOL_FAILED_FATALLY = "tool_failed_fatally"
    POLICY_VIOLATION = "policy_violation"
```

---

### LoopVerdict

Agent loop control verdicts for generic routing decisions.

```python
class LoopVerdict(str, Enum):
    PROCEED = "proceed"      # Continue to next stage
    LOOP_BACK = "loop_back"  # Return to an earlier stage
    ADVANCE = "advance"      # Skip ahead to a later stage
    ESCALATE = "escalate"    # Escalate to human
```

---

### RiskApproval

Risk approval status.

```python
class RiskApproval(str, Enum):
    APPROVED = "approved"
    DENIED = "denied"
    PENDING = "pending"
```

---

## Dataclasses

### OperationResult

Unified result type for operations across the codebase. Standardizes error responses and success results.

```python
@dataclass
class OperationResult:
    status: OperationStatus
    data: Optional[Dict[str, Any]] = None
    error: Optional[str] = None
    error_type: Optional[str] = None
    suggestions: List[str] = field(default_factory=list)
```

**Properties**:

| Property | Type | Description |
|----------|------|-------------|
| `is_success` | `bool` | `True` if `status == OperationStatus.SUCCESS` |
| `is_error` | `bool` | `True` if `status != OperationStatus.SUCCESS` |

**Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `to_dict` | `() -> Dict[str, Any]` | Convert to dictionary for JSON serialization |
| `success` | `(data: Optional[Dict]) -> OperationResult` | Create a success result (class method) |
| `error` | `(message: str, error_type: str, suggestions: List[str]) -> OperationResult` | Create an error result (class method) |
| `not_found` | `(message: str, suggestions: List[str]) -> OperationResult` | Create a not found result (class method) |
| `validation_error` | `(message: str, suggestions: List[str]) -> OperationResult` | Create a validation error result (class method) |

**Example**:
```python
from jeeves_protocols import OperationResult

# Success
result = OperationResult.success({"items": ["file1.py", "file2.py"]})

# Error with suggestions
result = OperationResult.error(
    "File not found",
    error_type="not_found",
    suggestions=["Check if the file path is correct", "Ensure the file exists"]
)

# Check result
if result.is_success:
    process(result.data)
else:
    handle_error(result.error)
```

---

## Navigation

- [Back to README](README.md)
- [Next: Configuration](config.md)
