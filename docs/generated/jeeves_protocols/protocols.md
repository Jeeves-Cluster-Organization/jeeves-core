# protocols.protocols

**Layer**: L0 (Foundation)  
**Purpose**: Protocol definitions - interfaces for dependency injection

## Overview

This module provides `typing.Protocol` classes for static type checking. These define interfaces that implementations in other packages must satisfy. This enables the Kubernetes-style pattern of protocol-first design with swappable implementations.

## Request Context

### RequestContext

Immutable request context for tracing and logging.

```python
@dataclass(frozen=True)
class RequestContext:
    request_id: str                      # Unique request identifier
    user_id: Optional[str] = None        # User identifier
    session_id: Optional[str] = None     # Session identifier
```

**Usage with ContextVars (ADR-001 Decision 5)**:
```python
from protocols import RequestContext
from shared import request_scope

ctx = RequestContext(request_id=str(uuid4()), user_id="user-123")
with request_scope(ctx, logger):
    # All code in this scope has access to context
    process_request()
```

---

## Logging Protocols

### LoggerProtocol

Structured logging interface.

```python
@runtime_checkable
class LoggerProtocol(Protocol):
    def info(self, message: str, **kwargs: Any) -> None: ...
    def debug(self, message: str, **kwargs: Any) -> None: ...
    def warning(self, message: str, **kwargs: Any) -> None: ...
    def error(self, message: str, **kwargs: Any) -> None: ...
    def bind(self, **kwargs: Any) -> "LoggerProtocol": ...
```

---

## Persistence Protocols

### PersistenceProtocol

Database persistence interface.

```python
@runtime_checkable
class PersistenceProtocol(Protocol):
    async def execute(self, query: str, params: Optional[Dict[str, Any]] = None) -> None: ...
    async def fetch_one(self, query: str, params: Optional[Dict[str, Any]] = None) -> Optional[Dict[str, Any]]: ...
    async def fetch_all(self, query: str, params: Optional[Dict[str, Any]] = None) -> List[Dict[str, Any]]: ...
```

### DatabaseClientProtocol

Database client interface with connection management.

```python
@runtime_checkable
class DatabaseClientProtocol(Protocol):
    async def connect(self) -> None: ...
    async def disconnect(self) -> None: ...
    async def execute(self, query: str, params: Optional[Dict[str, Any]] = None) -> None: ...
    async def fetch_one(self, query: str, params: Optional[Dict[str, Any]] = None) -> Optional[Dict[str, Any]]: ...
    async def fetch_all(self, query: str, params: Optional[Dict[str, Any]] = None) -> List[Dict[str, Any]]: ...
```

### VectorStorageProtocol

Unified vector storage interface for embeddings.

```python
@runtime_checkable
class VectorStorageProtocol(Protocol):
    async def upsert(
        self,
        item_id: str,
        content: str,
        collection: str,
        metadata: Dict[str, Any]
    ) -> None:
        """Store or update content with automatic embedding."""

    async def search(
        self,
        query: str,
        collections: List[str],
        filters: Optional[Dict[str, Any]] = None,
        limit: int = 10
    ) -> List[Dict[str, Any]]:
        """Search for similar content."""

    async def delete(self, item_id: str, collection: str) -> None:
        """Delete an item from storage."""

    def close(self) -> None:
        """Clean up resources."""
```

---

## LLM Protocols

### LLMProviderProtocol

LLM provider interface.

```python
@runtime_checkable
class LLMProviderProtocol(Protocol):
    async def generate(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> str: ...

    async def generate_streaming(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> Any: ...  # AsyncIterator[str]
```

---

## Tool Protocols

### ToolProtocol

Interface for individual tool implementations.

```python
@runtime_checkable
class ToolProtocol(Protocol):
    @property
    def name(self) -> str: ...

    @property
    def description(self) -> str: ...

    async def execute(self, params: Dict[str, Any]) -> Dict[str, Any]: ...
```

### ToolDefinitionProtocol

Tool definition returned by registry lookups.

```python
@runtime_checkable
class ToolDefinitionProtocol(Protocol):
    name: str
    function: Any  # Callable
    parameters: Dict[str, str]
    description: str
```

### ToolRegistryProtocol

Tool registry interface for managing and accessing tools.

```python
@runtime_checkable
class ToolRegistryProtocol(Protocol):
    def has_tool(self, name: str) -> bool:
        """Check if a tool is registered."""

    def get_tool(self, name: str) -> Optional[ToolDefinitionProtocol]:
        """Get tool definition by name."""
```

### ToolExecutorProtocol

Tool executor interface for running tools.

```python
@runtime_checkable
class ToolExecutorProtocol(Protocol):
    async def execute(
        self,
        tool_name: str,
        params: Dict[str, Any],
        context: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]: ...

    def get_available_tools(self) -> List[str]: ...
```

---

## App Context Protocols

### SettingsProtocol

Settings interface.

```python
@runtime_checkable
class SettingsProtocol(Protocol):
    def get(self, key: str, default: Any = None) -> Any: ...
    def set(self, key: str, value: Any) -> None: ...
```

### FeatureFlagsProtocol

Feature flags interface.

```python
@runtime_checkable
class FeatureFlagsProtocol(Protocol):
    def is_enabled(self, flag: str) -> bool: ...
    def get_variant(self, flag: str) -> Optional[str]: ...
```

### ClockProtocol

Clock interface for deterministic time.

```python
@runtime_checkable
class ClockProtocol(Protocol):
    def now(self) -> datetime: ...
    def utcnow(self) -> datetime: ...
```

### AppContextProtocol

Application context interface.

```python
@runtime_checkable
class AppContextProtocol(Protocol):
    @property
    def settings(self) -> SettingsProtocol: ...

    @property
    def feature_flags(self) -> FeatureFlagsProtocol: ...

    @property
    def logger(self) -> LoggerProtocol: ...
```

---

## Memory Protocols

### SearchResult

Search result from semantic search.

```python
@dataclass
class SearchResult:
    id: str
    content: str
    score: float
    metadata: Dict[str, Any]
```

### MemoryServiceProtocol

Memory service interface.

```python
@runtime_checkable
class MemoryServiceProtocol(Protocol):
    async def store(self, key: str, value: Any, ttl: Optional[int] = None) -> None: ...
    async def retrieve(self, key: str) -> Optional[Any]: ...
    async def delete(self, key: str) -> None: ...
```

### SemanticSearchProtocol

Semantic search interface.

```python
@runtime_checkable
class SemanticSearchProtocol(Protocol):
    async def search(
        self,
        query: str,
        limit: int = 10,
        filters: Optional[Dict[str, Any]] = None,
    ) -> List[SearchResult]: ...

    async def index(self, id: str, content: str, metadata: Dict[str, Any]) -> None: ...
```

### SessionStateProtocol

Session state interface.

```python
@runtime_checkable
class SessionStateProtocol(Protocol):
    async def get(self, session_id: str) -> Optional[Dict[str, Any]]: ...
    async def set(self, session_id: str, state: Dict[str, Any]) -> None: ...
    async def delete(self, session_id: str) -> None: ...
```

---

## Checkpoint Protocol

### CheckpointRecord

Checkpoint record for time-travel debugging.

```python
@dataclass
class CheckpointRecord:
    checkpoint_id: str
    envelope_id: str
    agent_name: str
    sequence: int
    state: Dict[str, Any]
    created_at: datetime
```

### CheckpointProtocol

Checkpoint interface for time-travel debugging.

```python
@runtime_checkable
class CheckpointProtocol(Protocol):
    async def save(self, envelope_id: str, agent_name: str, state: Dict[str, Any]) -> str: ...
    async def load(self, checkpoint_id: str) -> Optional[CheckpointRecord]: ...
    async def list_for_envelope(self, envelope_id: str) -> List[CheckpointRecord]: ...
    async def replay_to(self, checkpoint_id: str) -> Dict[str, Any]: ...
```

---

## Distributed Protocols

### DistributedTask

Task for distributed execution.

```python
@dataclass
class DistributedTask:
    task_id: str
    task_type: str
    payload: Dict[str, Any]
    priority: int = 0
    created_at: Optional[datetime] = None
```

### QueueStats

Queue statistics.

```python
@dataclass
class QueueStats:
    pending: int
    processing: int
    completed: int
    failed: int
```

### DistributedBusProtocol

Distributed message bus interface.

```python
@runtime_checkable
class DistributedBusProtocol(Protocol):
    async def enqueue(self, task: DistributedTask) -> str: ...
    async def dequeue(self, task_type: str, timeout: float = 0) -> Optional[DistributedTask]: ...
    async def complete(self, task_id: str, result: Dict[str, Any]) -> None: ...
    async def fail(self, task_id: str, error: str) -> None: ...
    async def stats(self, task_type: str) -> QueueStats: ...
```

---

## Node Profile (Distributed LLM)

### InferenceEndpoint

Profile for a distributed LLM node.

```python
@dataclass
class InferenceEndpoint:
    name: str                                    # Node identifier
    base_url: str                                # LLM server endpoint
    agents: List[str] = field(default_factory=list)  # Assigned agents
    model: str = ""                              # Model filename
    vram_gb: Optional[int] = None                # GPU VRAM in GB
    ram_gb: Optional[int] = None                 # System RAM in GB
    model_size_gb: Optional[float] = None        # Model memory footprint
    max_parallel: int = 1                        # Max concurrent requests
    gpu_id: Optional[int] = None                 # GPU device index
    metadata: Dict[str, Any] = field(default_factory=dict)
    priority: int = 0                            # Routing priority
```

**Properties**:

| Property | Type | Description |
|----------|------|-------------|
| `model_name` | `str` | Model name without .gguf extension |
| `vram_utilization` | `Optional[float]` | VRAM utilization percentage |

**Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `can_handle_load` | `(current_requests: int) -> bool` | Check if node can accept more requests |

### InferenceEndpointsProtocol

Node profiles interface for distributed LLM routing.

```python
@runtime_checkable
class InferenceEndpointsProtocol(Protocol):
    def get_profile_for_agent(self, agent_name: str) -> InferenceEndpoint:
        """Get the node profile assigned to an agent."""

    def list_profiles(self) -> List[InferenceEndpoint]:
        """List all configured node profiles."""
```

---

## Agent LLM Configuration

### AgentLLMConfig

LLM configuration for a specific agent.

```python
@dataclass
class AgentLLMConfig:
    agent_name: str
    model: str = "qwen2.5-7b-instruct-q4_k_m"
    temperature: Optional[float] = 0.3
    max_tokens: int = 2000
    server_url: Optional[str] = None             # Override per-agent
    provider: Optional[str] = None               # openai, anthropic, etc.
    timeout_seconds: int = 120
    context_window: int = 16384
```

### DomainLLMRegistryProtocol

Registry for capability-owned agent LLM configurations.

```python
@runtime_checkable
class DomainLLMRegistryProtocol(Protocol):
    def register(
        self,
        capability_id: str,
        agent_name: str,
        config: AgentLLMConfig
    ) -> None: ...

    def get_agent_config(self, agent_name: str) -> Optional[AgentLLMConfig]: ...
    def list_agents(self) -> List[str]: ...
    def get_capability_agents(self, capability_id: str) -> List[AgentLLMConfig]: ...
```

---

### FeatureFlagsProviderProtocol

Provider for feature flags at runtime.

```python
@runtime_checkable
class FeatureFlagsProviderProtocol(Protocol):
    def get_feature_flags(self) -> FeatureFlagsProtocol:
        """Get the current feature flags."""
        ...
```

Used to break circular dependencies where memory_module imports from avionics.feature_flags. Instead, the provider is injected.

**Constitutional Reference**:
- Memory Module: FORBIDDEN memory_module → avionics.*
- Use protocol injection instead of direct imports

---

## Memory Layer Protocols (L5-L6)

### GraphStorageProtocol

L5 Entity Graph storage interface.

```python
@runtime_checkable
class GraphStorageProtocol(Protocol):
    async def add_node(
        self,
        node_id: str,
        node_type: str,
        properties: Dict[str, Any],
        user_id: Optional[str] = None,
    ) -> bool: ...

    async def add_edge(
        self,
        source_id: str,
        target_id: str,
        edge_type: str,
        properties: Optional[Dict[str, Any]] = None,
    ) -> bool: ...

    async def get_node(self, node_id: str) -> Optional[Dict[str, Any]]: ...

    async def get_neighbors(
        self,
        node_id: str,
        edge_type: Optional[str] = None,
        direction: str = "both",
        limit: int = 100,
    ) -> List[Dict[str, Any]]: ...

    async def find_path(
        self,
        source_id: str,
        target_id: str,
        max_depth: int = 5,
    ) -> Optional[List[Dict[str, Any]]]: ...

    async def query_subgraph(
        self,
        center_id: str,
        depth: int = 2,
        node_types: Optional[List[str]] = None,
        edge_types: Optional[List[str]] = None,
    ) -> Dict[str, Any]: ...

    async def delete_node(self, node_id: str) -> bool: ...
```

### SkillStorageProtocol

L6 Skills/Patterns storage interface.

```python
@runtime_checkable
class SkillStorageProtocol(Protocol):
    async def store_skill(
        self,
        skill_id: str,
        skill_type: str,
        pattern: Dict[str, Any],
        source_context: Optional[Dict[str, Any]] = None,
        confidence: float = 0.5,
        user_id: Optional[str] = None,
    ) -> str: ...

    async def get_skill(self, skill_id: str) -> Optional[Dict[str, Any]]: ...

    async def find_skills(
        self,
        skill_type: Optional[str] = None,
        context: Optional[Dict[str, Any]] = None,
        min_confidence: float = 0.0,
        limit: int = 10,
        user_id: Optional[str] = None,
    ) -> List[Dict[str, Any]]: ...

    async def update_confidence(
        self,
        skill_id: str,
        delta: float,
        reason: Optional[str] = None,
    ) -> float: ...

    async def record_usage(
        self,
        skill_id: str,
        success: bool,
        context: Optional[Dict[str, Any]] = None,
    ) -> None: ...

    async def delete_skill(self, skill_id: str) -> bool: ...
    async def get_skill_stats(self, skill_id: str) -> Optional[Dict[str, Any]]: ...
```

---

## Other Protocols

### EventBusProtocol

Event bus interface for pub/sub messaging.

```python
@runtime_checkable
class EventBusProtocol(Protocol):
    def publish(self, event_type: str, payload: Dict[str, Any]) -> None: ...
    def subscribe(self, event_type: str, handler: Any) -> None: ...
    def unsubscribe(self, event_type: str, handler: Any) -> None: ...
```

### IdGeneratorProtocol

ID generator interface.

```python
@runtime_checkable
class IdGeneratorProtocol(Protocol):
    def generate(self) -> str: ...
    def generate_prefixed(self, prefix: str) -> str: ...
```

### ConfigRegistryProtocol

Configuration registry for dependency injection.

```python
@runtime_checkable
class ConfigRegistryProtocol(Protocol):
    def register(self, key: str, value: Any) -> None: ...
    def get(self, key: str, default: Any = None) -> Any: ...
    def has(self, key: str) -> bool: ...
```

### LanguageConfigProtocol

Language configuration for code analysis.

```python
@runtime_checkable
class LanguageConfigProtocol(Protocol):
    def get_extensions(self, language: str) -> List[str]: ...
    def get_comment_patterns(self, language: str) -> Dict[str, str]: ...
    def detect_language(self, filename: str) -> Optional[str]: ...
```

---

## NLI Protocols

### IntentParsingProtocol

Intent parsing service for natural language understanding.

```python
@runtime_checkable
class IntentParsingProtocol(Protocol):
    async def parse_intent(self, text: str) -> Dict[str, Any]: ...
    async def generate_response(self, intent: Dict[str, Any], context: Dict[str, Any]) -> str: ...
```

---

### ClaimVerificationProtocol

Claim verification service using Natural Language Inference.

```python
@runtime_checkable
class ClaimVerificationProtocol(Protocol):
    def verify_claim(self, claim: str, evidence: str) -> Any: ...
    def verify_claims_batch(self, claims: List[tuple]) -> List[Any]: ...
```

Verifies that claims are entailed by their cited evidence. Used by memory_module.services.nli_service.NLIService.

---

### AgentToolAccessProtocol

Agent tool access control interface.

```python
@runtime_checkable
class AgentToolAccessProtocol(Protocol):
    def can_access(self, agent_name: str, tool_name: str) -> bool: ...
    def get_allowed_tools(self, agent_name: str) -> List[str]: ...
```

---

## Navigation

- [Back to README](README.md)
- [Previous: Agents](agents.md)
- [Next: Capability](capability.md)
