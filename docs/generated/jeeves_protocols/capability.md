# jeeves_protocols.capability

**Layer**: L0 (Foundation)  
**Purpose**: Capability Registration Protocols for dynamic resource management

## Overview

This module defines protocols for capabilities to register their resources (schemas, modes, services, tools, etc.) with infrastructure without infrastructure having hardcoded knowledge of specific capabilities.

## Constitutional Reference

- **Avionics R3**: No Domain Logic - infrastructure provides transport, not business logic
- **Avionics R4**: Swappable Implementations - capabilities register their own resources
- **Mission System Constitution**: Domain configs OWNED by capabilities

---

## Tool Catalog

### ToolCatalogEntry

Immutable tool metadata for capability tools.

```python
@dataclass(frozen=True)
class ToolCatalogEntry:
    id: str                    # Tool identifier
    description: str           # Human-readable description
    parameters: Dict[str, str] # parameter_name -> type_description
    category: str              # ToolCategory value as string
    risk_level: str = "low"    # RiskLevel value as string
```

### ToolDefinition

Tool definition returned by catalog lookups.

```python
@dataclass
class ToolDefinition:
    name: str                  # Tool name
    function: Callable         # The implementing function
    parameters: Dict[str, str] # Parameter definitions
    description: str = ""      # Description
```

### CapabilityToolCatalog

Tool catalog scoped to a single capability.

```python
class CapabilityToolCatalog:
    def __init__(self, capability_id: str):
        """Initialize a catalog for a specific capability."""

    def register(
        self,
        tool_id: str,
        func: Callable,
        description: str,
        parameters: Dict[str, str],
        category: str,
        risk_level: str = "low",
    ) -> None:
        """Register a tool in this capability's catalog."""

    def get_tool(self, tool_id: str) -> Optional[ToolDefinition]:
        """Get a tool definition by ID."""

    def get_function(self, tool_id: str) -> Optional[Callable]:
        """Get tool function by ID."""

    def has_tool(self, tool_id: str) -> bool:
        """Check if tool is registered."""

    def list_tools(self) -> List[str]:
        """List all registered tool IDs."""

    def get_entries(self) -> List[ToolCatalogEntry]:
        """Get all tool entries."""

    def generate_prompt_section(self) -> str:
        """Generate tool descriptions for LLM prompts."""
```

**Example**:
```python
from jeeves_protocols.capability import CapabilityToolCatalog

catalog = CapabilityToolCatalog("code_analysis")

catalog.register(
    tool_id="search_code",
    func=search_code_impl,
    description="Search for code patterns in the repository",
    parameters={"query": "string", "path": "string?"},
    category="standalone",
    risk_level="read_only",
)

# Query tools
if catalog.has_tool("search_code"):
    tool = catalog.get_tool("search_code")
    result = await tool.function({"query": "UserService"})

# Generate prompt section
prompt = catalog.generate_prompt_section()
# "Tools for code_analysis:\n- search_code(query, path?): Search for code patterns..."
```

---

## Configuration Dataclasses

### CapabilityAgentConfig

Configuration for an agent registered by a capability.

```python
@dataclass
class CapabilityAgentConfig:
    name: str                  # Agent name
    description: str           # Agent description
    layer: str                 # "perception", "planning", "execution", etc.
    tools: List[str] = field(default_factory=list)  # Allowed tools
```

### CapabilityPromptConfig

Configuration for a prompt registered by a capability.

```python
@dataclass
class CapabilityPromptConfig:
    prompt_id: str             # Prompt identifier
    version: str               # Prompt version
    description: str           # Prompt description
    prompt_factory: Callable[[], str]  # Function that returns the prompt template
```

### CapabilityToolsConfig

Configuration for tools registered by a capability.

```python
@dataclass
class CapabilityToolsConfig:
    tool_ids: List[str] = field(default_factory=list)  # List of tool IDs
    catalog: Optional[CapabilityToolCatalog] = None    # Tool catalog
    initializer: Optional[Callable[..., CapabilityToolCatalog]] = None  # Lazy init

    def get_catalog(self, **kwargs) -> Optional[CapabilityToolCatalog]:
        """Get or initialize the tool catalog."""
```

### CapabilityOrchestratorConfig

Configuration for orchestrator service registered by a capability.

```python
@dataclass
class CapabilityOrchestratorConfig:
    factory: Callable[..., Any]  # Factory function
    result_type: Optional[type] = None  # Result type class
```

### CapabilityContractsConfig

Configuration for tool result contracts registered by a capability.

```python
@dataclass
class CapabilityContractsConfig:
    schemas: Dict[str, type] = field(default_factory=dict)  # tool_name -> schema type
    validators: Dict[str, Callable[..., Any]] = field(default_factory=dict)
    module: Optional[Any] = None  # Contracts module for import access
```

### CapabilityServiceConfig

Configuration for a capability service (Control Tower registration).

```python
@dataclass
class CapabilityServiceConfig:
    service_id: str            # Service identifier
    service_type: str = "flow" # "flow", "tool", "query"
    capabilities: List[str] = field(default_factory=list)
    max_concurrent: int = 10
    is_default: bool = False   # If True, this is the default service
```

### CapabilityModeConfig

Configuration for a capability mode.

```python
@dataclass
class CapabilityModeConfig:
    mode_id: str               # Mode identifier
    response_fields: List[str] = field(default_factory=list)  # Fields in response
    requires_repo_path: bool = False  # Whether mode requires repo_path
```

---

## CapabilityResourceRegistryProtocol

Protocol for capability resource registration.

```python
@runtime_checkable
class CapabilityResourceRegistryProtocol(Protocol):
    def register_schema(self, capability_id: str, schema_path: str) -> None: ...
    def register_mode(self, capability_id: str, mode_config: CapabilityModeConfig) -> None: ...
    def get_schemas(self, capability_id: Optional[str] = None) -> List[str]: ...
    def get_mode_config(self, mode_id: str) -> Optional[CapabilityModeConfig]: ...
    def is_mode_registered(self, mode_id: str) -> bool: ...
    def list_modes(self) -> List[str]: ...
    def register_service(self, capability_id: str, service_config: CapabilityServiceConfig) -> None: ...
    def get_services(self, capability_id: Optional[str] = None) -> List[CapabilityServiceConfig]: ...
    def get_default_service(self) -> Optional[str]: ...
    def list_capabilities(self) -> List[str]: ...
```

---

## CapabilityResourceRegistry

Default implementation of `CapabilityResourceRegistryProtocol`. Thread-safe registry for capability resources.

### Core Registration Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `register_schema` | `(capability_id, schema_path)` | Register a database schema |
| `register_mode` | `(capability_id, mode_config)` | Register a gateway mode |
| `register_service` | `(capability_id, service_config)` | Register a service |

### Extended Registration Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `register_orchestrator` | `(capability_id, config)` | Register an orchestrator factory |
| `register_tools` | `(capability_id, config)` | Register a tools initializer |
| `register_prompts` | `(capability_id, prompts)` | Register prompts |
| `register_agents` | `(capability_id, agents)` | Register agent definitions |
| `register_contracts` | `(capability_id, config)` | Register tool result contracts |

### Query Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_schemas` | `(capability_id?) -> List[str]` | Get registered schema paths |
| `get_mode_config` | `(mode_id) -> Optional[CapabilityModeConfig]` | Get mode config by ID |
| `is_mode_registered` | `(mode_id) -> bool` | Check if mode is registered |
| `list_modes` | `() -> List[str]` | List all mode IDs |
| `get_services` | `(capability_id?) -> List[CapabilityServiceConfig]` | Get service configs |
| `get_service_config` | `(service_id) -> Optional[CapabilityServiceConfig]` | Get service by ID |
| `get_default_service` | `() -> Optional[str]` | Get default service name |
| `list_capabilities` | `() -> List[str]` | List all capability IDs |
| `get_orchestrator` | `(capability_id?) -> Optional[CapabilityOrchestratorConfig]` | Get orchestrator |
| `get_tools` | `(capability_id?) -> Optional[CapabilityToolsConfig]` | Get tools config |
| `get_prompts` | `(capability_id?) -> List[CapabilityPromptConfig]` | Get prompts |
| `get_agents` | `(capability_id?) -> List[CapabilityAgentConfig]` | Get agent configs |
| `get_contracts` | `(capability_id?) -> Optional[CapabilityContractsConfig]` | Get contracts |
| `clear` | `() -> None` | Clear all registrations (for testing) |

---

## Global Registry Functions

### get_capability_resource_registry

Get the global capability resource registry instance (lazy initialization).

```python
def get_capability_resource_registry() -> CapabilityResourceRegistry:
    """Get the global capability resource registry instance."""
```

### reset_capability_resource_registry

Reset the global registry instance (primarily for testing).

```python
def reset_capability_resource_registry() -> None:
    """Reset the global registry instance."""
```

---

## Usage Examples

### Capability Startup

```python
from jeeves_protocols.capability import (
    get_capability_resource_registry,
    CapabilityModeConfig,
    CapabilityServiceConfig,
    CapabilityToolsConfig,
)

# Get the global registry
registry = get_capability_resource_registry()

# Register database schema
registry.register_schema("code_analysis", "database/schemas/code_analysis.sql")

# Register mode
registry.register_mode("code_analysis", CapabilityModeConfig(
    mode_id="code_analysis",
    response_fields=["intent", "plan", "findings", "response"],
    requires_repo_path=True,
))

# Register service
registry.register_service("code_analysis", CapabilityServiceConfig(
    service_id="code_analysis_flow",
    service_type="flow",
    capabilities=["code_search", "file_read", "symbol_lookup"],
    max_concurrent=10,
    is_default=True,
))

# Register tools with lazy initialization
def create_tool_catalog(db):
    catalog = CapabilityToolCatalog("code_analysis")
    # Register tools...
    return catalog

registry.register_tools("code_analysis", CapabilityToolsConfig(
    tool_ids=["search_code", "read_file", "get_symbol"],
    initializer=create_tool_catalog,
))
```

### Infrastructure Query

```python
from jeeves_protocols.capability import get_capability_resource_registry

registry = get_capability_resource_registry()

# Get all schemas for migration
schemas = registry.get_schemas()
for schema_path in schemas:
    run_migration(schema_path)

# Get default service for Control Tower
default_service = registry.get_default_service()

# Get mode config for gateway
mode = registry.get_mode_config("code_analysis")
if mode and mode.requires_repo_path:
    validate_repo_path(request)
```

---

## Navigation

- [Back to README](README.md)
- [Previous: Protocols](protocols.md)
- [Next: Memory](memory.md)
