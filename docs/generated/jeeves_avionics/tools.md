# Tools Module

Tool catalog and executor infrastructure.

## Navigation

- [README](./README.md) - Overview
- [Settings](./settings.md)
- [Gateway](./gateway.md)
- [Database](./database.md)
- [LLM](./llm.md)
- [Observability](./observability.md)
- **Tools** (this file)
- [Infrastructure](./infrastructure.md)

---

## tools/catalog.py

Canonical tool catalog - single source of truth for all tools.

### Enum: ToolId

```python
class ToolId(str, Enum):
    """Typed tool identifiers.
    
    All tool references must use these enum values, not raw strings.
    """
    
    # CODE ANALYSIS TOOLS
    SEARCH_CODE = "search_code"
    ANALYZE = "analyze"
    
    # COMPOSITE TOOLS (multi-step workflows)
    LOCATE = "locate"
    EXPLORE_SYMBOL_USAGE = "explore_symbol_usage"
    MAP_MODULE = "map_module"
    TRACE_ENTRY_POINT = "trace_entry_point"
    EXPLAIN_CODE_HISTORY = "explain_code_history"
    
    # RESILIENT TOOLS (with fallbacks)
    READ_CODE = "read_code"
    FIND_RELATED = "find_related"
    
    # STANDALONE TOOLS
    GIT_STATUS = "git_status"
    LIST_TOOLS = "list_tools"
    
    # INTERNAL/BASE TOOLS (not exposed to agents)
    READ_FILE = "read_file"
    GLOB_FILES = "glob_files"
    GREP_SEARCH = "grep_search"
    TREE_STRUCTURE = "tree_structure"
    FIND_SYMBOL = "find_symbol"
    GET_FILE_SYMBOLS = "get_file_symbols"
    # ... and more
```

### Imported Enums

```python
from protocols import RiskLevel, ToolCategory

class ToolCategory(str, Enum):
    UNIFIED = "unified"
    COMPOSITE = "composite"
    RESILIENT = "resilient"
    STANDALONE = "standalone"
    INTERNAL = "internal"

class RiskLevel(str, Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
```

### Dataclass: ToolCatalogEntry

```python
@dataclass(frozen=True)
class ToolCatalogEntry:
    """Immutable tool metadata."""
    id: ToolId
    description: str
    parameters: Dict[str, str]
    category: ToolCategory
    risk_level: RiskLevel = RiskLevel.LOW
    accepts: str = ""
    returns: str = ""
    
    def to_prompt_line(self) -> str:
        """Format entry for LLM prompt."""
```

### Dataclass: ToolDefinition

```python
@dataclass
class ToolDefinition:
    """Tool definition for ToolExecutor."""
    name: str
    function: Callable
    parameters: Dict[str, str]
    description: str = ""
```

### Constant: EXPOSED_TOOL_IDS

Tools visible to agents (Decision 2:B - only Traverser can execute, but Planner can see for planning):

```python
EXPOSED_TOOL_IDS: FrozenSet[ToolId] = frozenset({
    # Unified Analyzer (Amendment XXII) - Primary entry point
    ToolId.ANALYZE,
    # Composite (5) - Now internal, orchestrated by analyze
    ToolId.LOCATE,
    ToolId.EXPLORE_SYMBOL_USAGE,
    ToolId.MAP_MODULE,
    ToolId.TRACE_ENTRY_POINT,
    ToolId.EXPLAIN_CODE_HISTORY,
    # Resilient (2)
    ToolId.READ_CODE,
    ToolId.FIND_RELATED,
    # Standalone (2)
    ToolId.GIT_STATUS,
    ToolId.LIST_TOOLS,
})
```

### Class: ToolCatalog

```python
class ToolCatalog:
    """Canonical tool catalog - single source of truth.
    
    Implements ToolRegistryProtocol.
    """
```

#### Registration Methods

```python
def register(
    self,
    tool_id: ToolId,
    description: str,
    parameters: Dict[str, str],
    category: ToolCategory,
    risk_level: RiskLevel = RiskLevel.LOW,
    accepts: str = "",
    returns: str = "",
) -> Callable:
    """Register tool. Can be used as decorator."""

def register_function(
    self,
    tool_id: ToolId,
    func: Callable,
    description: str,
    parameters: Dict[str, str],
    category: ToolCategory,
    risk_level: RiskLevel = RiskLevel.LOW,
) -> None:
    """Register tool without decorator syntax."""
```

#### Protocol Implementation (string-based)

```python
def has_tool(self, name: str) -> bool:
    """Check if tool is registered by string name."""

def get_tool(self, name: str) -> Optional[ToolDefinition]:
    """Get tool by string name."""
```

#### ToolId-Based Access

```python
def has_tool_id(self, tool_id: ToolId) -> bool:
    """Check if tool is registered by ToolId."""

def get_entry(self, tool_id: ToolId) -> Optional[ToolCatalogEntry]:
    """Get tool metadata by ToolId."""

def get_function(self, tool_id: ToolId) -> Optional[Callable]:
    """Get tool implementation by ToolId."""
```

#### Catalog Operations

```python
def get_exposed_entries(self) -> List[ToolCatalogEntry]:
    """Get entries for all exposed tools."""

def get_entries_by_category(self, category: ToolCategory) -> List[ToolCatalogEntry]:
    """Get all entries in a category."""

def generate_planner_prompt(self, allowed_tools: FrozenSet[ToolId]) -> str:
    """Generate tool descriptions for Planner prompt."""

def list_all_ids(self) -> List[ToolId]:
    """List all registered tool IDs."""

def to_dict(self) -> Dict[str, Dict[str, Any]]:
    """Export catalog as dictionary."""
```

#### Singleton Access

```python
@classmethod
def get_instance(cls) -> ToolCatalog:
    """Get singleton instance."""

@classmethod
def reset_instance(cls) -> None:
    """Reset singleton instance (for testing)."""
```

### Helper Functions

```python
def resolve_tool_id(name: str) -> Optional[ToolId]:
    """Resolve string tool name to ToolId."""

def is_exposed_tool(tool_id: ToolId) -> bool:
    """Check if tool is in exposed set."""
```

### Global Instance

```python
tool_catalog = ToolCatalog.get_instance()
```

---

## tools/executor.py

Pure, testable execution logic.

### Class: ToolExecutionCore

```python
class ToolExecutionCore:
    """Pure execution logic for tools - easily testable.
    
    Handles:
    - Parameter validation
    - None filtering
    - Execution timing
    - Result normalization
    
    Does NOT handle:
    - Registry lookups
    - Access control
    - Resilient fallbacks
    """
    
    def __init__(self, logger: Optional[LoggerProtocol] = None):
        ...
```

#### Methods

```python
def validate_params(
    self,
    parameter_schemas: Optional[Dict[str, Any]],
    params: Dict[str, Any],
) -> List[str]:
    """Validate parameters against schemas. Returns error messages."""

def filter_none_params(self, params: Dict[str, Any]) -> Dict[str, Any]:
    """Filter out None values so function defaults apply."""

def normalize_result(
    self,
    result: Dict[str, Any],
    execution_time_ms: int,
) -> Dict[str, Any]:
    """Normalize tool result into standard format."""

async def execute_tool(
    self,
    tool_function: Callable[..., Awaitable[Dict[str, Any]]],
    params: Dict[str, Any],
    parameter_schemas: Optional[Dict[str, Any]] = None,
    tool_name: str = "unknown",
) -> Dict[str, Any]:
    """Execute a tool function with validation, filtering, and timing."""
```

#### Result Format

```python
# Success result
{
    "status": "success",
    "data": {...},
    "execution_time_ms": 150,
}

# Not found result
{
    "status": "not_found",
    "data": {...},
    "message": "Tool returned not_found",
    "execution_time_ms": 50,
}

# Error result
{
    "status": "error",
    "error": "Error message",
    "error_type": "tool_error",
    "execution_time_ms": 10,
}

# Validation error
{
    "status": "error",
    "error": "Parameter validation failed: ...",
    "error_type": "validation_error",
    "validation_errors": ["error1", "error2"],
    "execution_time_ms": 5,
}
```

---

## Usage Examples

### Registering Tools

```python
from avionics.tools import tool_catalog, ToolId, ToolCategory

@tool_catalog.register(
    tool_id=ToolId.LOCATE,
    description="Find code anywhere in the codebase",
    parameters={"symbol": "string", "scope": "string?"},
    category=ToolCategory.COMPOSITE,
)
async def locate(symbol: str, scope: str = None):
    # Implementation
    return {"status": "success", "matches": [...]}
```

### Checking Tool Availability

```python
from avionics.tools import tool_catalog, ToolId

if tool_catalog.has_tool_id(ToolId.LOCATE):
    entry = tool_catalog.get_entry(ToolId.LOCATE)
    print(f"Tool: {entry.description}")
```

### Generating Planner Prompt

```python
from avionics.tools import tool_catalog, EXPOSED_TOOL_IDS

prompt = tool_catalog.generate_planner_prompt(EXPOSED_TOOL_IDS)
print(prompt)
# Output:
# Available Tools:
# 
# ═══ UNIFIED (primary entry point) ═══
# - analyze(query): Unified code analysis...
# 
# ═══ COMPOSITE (multi-step workflows) ═══
# - locate(symbol, scope?): Find code anywhere
# ...
```

### Using ToolExecutionCore

```python
from avionics.tools.executor_core import ToolExecutionCore

core = ToolExecutionCore(logger)

result = await core.execute_tool(
    tool_function=my_tool_function,
    params={"symbol": "UserService"},
    parameter_schemas=schemas,
    tool_name="locate",
)

if result["status"] == "success":
    print(result["data"])
else:
    print(f"Error: {result['error']}")
```
