"""Protocol definitions - interfaces for dependency injection.

These are typing.Protocol classes for static type checking.
Implementations are in Go or Python adapters.
"""

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional, Protocol, runtime_checkable


# =============================================================================
# REQUEST CONTEXT
# =============================================================================

@dataclass(frozen=True)
class RequestContext:
    """Immutable request context for tracing and logging.

    Used with ContextVars for async-safe request tracking (ADR-001 Decision 5).

    Usage:
        ctx = RequestContext(request_id=str(uuid4()), user_id="user-123")
        with request_scope(ctx, logger):
            # All code in this scope has access to context
            ...
    """
    request_id: str
    user_id: Optional[str] = None
    session_id: Optional[str] = None


# =============================================================================
# LOGGING
# =============================================================================

@runtime_checkable
class LoggerProtocol(Protocol):
    """Structured logging interface."""

    def info(self, message: str, **kwargs: Any) -> None: ...
    def debug(self, message: str, **kwargs: Any) -> None: ...
    def warning(self, message: str, **kwargs: Any) -> None: ...
    def error(self, message: str, **kwargs: Any) -> None: ...
    def bind(self, **kwargs: Any) -> "LoggerProtocol": ...


# =============================================================================
# PERSISTENCE
# =============================================================================

@runtime_checkable
class PersistenceProtocol(Protocol):
    """Database persistence interface."""

    async def execute(self, query: str, params: Optional[Dict[str, Any]] = None) -> None: ...
    async def fetch_one(self, query: str, params: Optional[Dict[str, Any]] = None) -> Optional[Dict[str, Any]]: ...
    async def fetch_all(self, query: str, params: Optional[Dict[str, Any]] = None) -> List[Dict[str, Any]]: ...


@runtime_checkable
class DatabaseClientProtocol(Protocol):
    """Database client interface."""

    async def connect(self) -> None: ...
    async def disconnect(self) -> None: ...
    async def execute(self, query: str, params: Optional[Dict[str, Any]] = None) -> None: ...
    async def fetch_one(self, query: str, params: Optional[Dict[str, Any]] = None) -> Optional[Dict[str, Any]]: ...
    async def fetch_all(self, query: str, params: Optional[Dict[str, Any]] = None) -> List[Dict[str, Any]]: ...


@runtime_checkable
class VectorStorageProtocol(Protocol):
    """Unified vector storage interface for embeddings.

    Supports both:
    - Low-level vector storage (raw vectors)
    - High-level semantic storage (text content with automatic embedding)

    Collections allow organizing vectors by type (e.g., "chunks", "facts").
    """

    async def upsert(
        self,
        item_id: str,
        content: str,
        collection: str,
        metadata: Dict[str, Any]
    ) -> None:
        """Store or update content with automatic embedding.

        Args:
            item_id: Unique identifier for the item
            content: Text content to embed and store
            collection: Collection/namespace for the item
            metadata: Additional metadata to store
        """
        ...

    async def search(
        self,
        query: str,
        collections: List[str],
        filters: Optional[Dict[str, Any]] = None,
        limit: int = 10
    ) -> List[Dict[str, Any]]:
        """Search for similar content.

        Args:
            query: Search query (text or vector)
            collections: Collections to search in
            filters: Optional metadata filters
            limit: Maximum results to return

        Returns:
            List of matching items with similarity scores
        """
        ...

    async def delete(self, item_id: str, collection: str) -> None:
        """Delete an item from storage.

        Args:
            item_id: Unique identifier for the item
            collection: Collection the item belongs to
        """
        ...

    def close(self) -> None:
        """Clean up resources."""
        ...


# =============================================================================
# LLM
# =============================================================================

@runtime_checkable
class LLMProviderProtocol(Protocol):
    """LLM provider interface."""

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


# =============================================================================
# TOOLS
# =============================================================================

@runtime_checkable
class ToolProtocol(Protocol):
    """Tool interface."""

    @property
    def name(self) -> str: ...

    @property
    def description(self) -> str: ...

    async def execute(self, params: Dict[str, Any]) -> Dict[str, Any]: ...


@runtime_checkable
class ToolRegistryProtocol(Protocol):
    """Tool registry interface."""

    def register(self, tool: ToolProtocol) -> None: ...
    def get(self, name: str) -> Optional[ToolProtocol]: ...
    def list(self) -> List[str]: ...
    async def execute(self, name: str, params: Dict[str, Any]) -> Dict[str, Any]: ...


# =============================================================================
# APP CONTEXT
# =============================================================================

@runtime_checkable
class SettingsProtocol(Protocol):
    """Settings interface."""

    def get(self, key: str, default: Any = None) -> Any: ...
    def set(self, key: str, value: Any) -> None: ...


@runtime_checkable
class FeatureFlagsProtocol(Protocol):
    """Feature flags interface."""

    def is_enabled(self, flag: str) -> bool: ...
    def get_variant(self, flag: str) -> Optional[str]: ...


@runtime_checkable
class ClockProtocol(Protocol):
    """Clock interface for deterministic time."""

    def now(self) -> datetime: ...
    def utcnow(self) -> datetime: ...


@runtime_checkable
class AppContextProtocol(Protocol):
    """Application context interface."""

    @property
    def settings(self) -> SettingsProtocol: ...

    @property
    def feature_flags(self) -> FeatureFlagsProtocol: ...

    @property
    def logger(self) -> LoggerProtocol: ...


# =============================================================================
# MEMORY
# =============================================================================

@dataclass
class SearchResult:
    """Search result from semantic search."""
    id: str
    content: str
    score: float
    metadata: Dict[str, Any]


@runtime_checkable
class MemoryServiceProtocol(Protocol):
    """Memory service interface."""

    async def store(self, key: str, value: Any, ttl: Optional[int] = None) -> None: ...
    async def retrieve(self, key: str) -> Optional[Any]: ...
    async def delete(self, key: str) -> None: ...


@runtime_checkable
class SemanticSearchProtocol(Protocol):
    """Semantic search interface."""

    async def search(
        self,
        query: str,
        limit: int = 10,
        filters: Optional[Dict[str, Any]] = None,
    ) -> List[SearchResult]: ...

    async def index(self, id: str, content: str, metadata: Dict[str, Any]) -> None: ...


@runtime_checkable
class SessionStateProtocol(Protocol):
    """Session state interface."""

    async def get(self, session_id: str) -> Optional[Dict[str, Any]]: ...
    async def set(self, session_id: str, state: Dict[str, Any]) -> None: ...
    async def delete(self, session_id: str) -> None: ...


# =============================================================================
# CHECKPOINT (Time-travel debugging)
# =============================================================================

@dataclass
class CheckpointRecord:
    """Checkpoint record for time-travel debugging."""
    checkpoint_id: str
    envelope_id: str
    agent_name: str
    sequence: int
    state: Dict[str, Any]
    created_at: datetime


@runtime_checkable
class CheckpointProtocol(Protocol):
    """Checkpoint interface for time-travel debugging."""

    async def save(self, envelope_id: str, agent_name: str, state: Dict[str, Any]) -> str: ...
    async def load(self, checkpoint_id: str) -> Optional[CheckpointRecord]: ...
    async def list_for_envelope(self, envelope_id: str) -> List[CheckpointRecord]: ...
    async def replay_to(self, checkpoint_id: str) -> Dict[str, Any]: ...


# =============================================================================
# DISTRIBUTED BUS
# =============================================================================

@dataclass
class DistributedTask:
    """Task for distributed execution."""
    task_id: str
    task_type: str
    payload: Dict[str, Any]
    priority: int = 0
    created_at: Optional[datetime] = None


@dataclass
class QueueStats:
    """Queue statistics."""
    pending: int
    processing: int
    completed: int
    failed: int


@runtime_checkable
class DistributedBusProtocol(Protocol):
    """Distributed message bus interface."""

    async def enqueue(self, task: DistributedTask) -> str: ...
    async def dequeue(self, task_type: str, timeout: float = 0) -> Optional[DistributedTask]: ...
    async def complete(self, task_id: str, result: Dict[str, Any]) -> None: ...
    async def fail(self, task_id: str, error: str) -> None: ...
    async def stats(self, task_type: str) -> QueueStats: ...


# =============================================================================
# NLI (Natural Language Interface)
# =============================================================================

@runtime_checkable
class NLIServiceProtocol(Protocol):
    """Natural language interface service."""

    async def parse_intent(self, text: str) -> Dict[str, Any]: ...
    async def generate_response(self, intent: Dict[str, Any], context: Dict[str, Any]) -> str: ...


# =============================================================================
# EVENT BUS
# =============================================================================

@runtime_checkable
class EventBusProtocol(Protocol):
    """Event bus interface for pub/sub messaging."""

    def publish(self, event_type: str, payload: Dict[str, Any]) -> None: ...
    def subscribe(self, event_type: str, handler: Any) -> None: ...
    def unsubscribe(self, event_type: str, handler: Any) -> None: ...


# =============================================================================
# ID GENERATOR
# =============================================================================

@runtime_checkable
class IdGeneratorProtocol(Protocol):
    """ID generator interface."""

    def generate(self) -> str: ...
    def generate_prefixed(self, prefix: str) -> str: ...


# =============================================================================
# TOOL EXECUTOR
# =============================================================================

@runtime_checkable
class ToolExecutorProtocol(Protocol):
    """Tool executor interface for running tools."""

    async def execute(
        self,
        tool_name: str,
        params: Dict[str, Any],
        context: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]: ...

    def get_available_tools(self) -> List[str]: ...


# =============================================================================
# EVENT CONTEXT
# =============================================================================

@runtime_checkable
class EventContextProtocol(Protocol):
    """Event context for tracking request lifecycle."""

    @property
    def request_id(self) -> str: ...

    @property
    def session_id(self) -> str: ...

    @property
    def user_id(self) -> str: ...

    def emit(self, event_type: str, data: Dict[str, Any]) -> None: ...


# =============================================================================
# CONFIG REGISTRY
# =============================================================================

@runtime_checkable
class ConfigRegistryProtocol(Protocol):
    """Configuration registry for dependency injection."""

    def register(self, key: str, value: Any) -> None: ...
    def get(self, key: str, default: Any = None) -> Any: ...
    def has(self, key: str) -> bool: ...


# =============================================================================
# LANGUAGE CONFIG
# =============================================================================

@runtime_checkable
class LanguageConfigProtocol(Protocol):
    """Language configuration for code analysis."""

    def get_extensions(self, language: str) -> List[str]: ...
    def get_comment_patterns(self, language: str) -> Dict[str, str]: ...
    def detect_language(self, filename: str) -> Optional[str]: ...


# =============================================================================
# DISTRIBUTED NODE PROFILES
# =============================================================================

@dataclass
class NodeProfile:
    """Profile for a distributed LLM node.

    Used for routing agents to specific nodes in distributed deployments.
    Supports both simple routing and hardware-aware deployment configuration.

    Required fields:
        name: Node identifier
        base_url: LLM server endpoint
        agents: List of agents assigned to this node

    Optional hardware fields (for deployment configuration):
        model: Full model filename (e.g., "qwen2.5-7b-instruct-q4_K_M.gguf")
        vram_gb: GPU VRAM in GB
        ram_gb: System RAM in GB
        model_size_gb: Model memory footprint in GB
        max_parallel: Maximum concurrent requests
        gpu_id: GPU device index
        metadata: Additional configuration metadata
        priority: Routing priority (higher = preferred)
    """
    name: str
    base_url: str
    agents: List[str] = field(default_factory=list)
    # Model info
    model: str = ""
    # Hardware specs (optional for hardware-aware deployments)
    vram_gb: Optional[int] = None
    ram_gb: Optional[int] = None
    model_size_gb: Optional[float] = None
    max_parallel: int = 1
    gpu_id: Optional[int] = None
    metadata: Dict[str, Any] = field(default_factory=dict)
    priority: int = 0

    def __post_init__(self):
        """Validate configuration after initialization."""
        if self.model_size_gb is not None and self.vram_gb is not None:
            if self.model_size_gb > self.vram_gb:
                raise ValueError(
                    f"Model size ({self.model_size_gb}GB) exceeds "
                    f"VRAM capacity ({self.vram_gb}GB) for node {self.name}"
                )
        if self.max_parallel < 1:
            raise ValueError(f"max_parallel must be >= 1 for node {self.name}")

    @property
    def model_name(self) -> str:
        """Extract model name without .gguf extension and quantization suffix."""
        if not self.model:
            return ""
        return self.model.replace(".gguf", "").replace("-q4_k_m", "").replace("-q4_K_M", "")

    @property
    def vram_utilization(self) -> Optional[float]:
        """Calculate VRAM utilization percentage."""
        if self.vram_gb is None or self.model_size_gb is None or self.vram_gb == 0:
            return None
        return (self.model_size_gb / self.vram_gb) * 100

    def can_handle_load(self, current_requests: int) -> bool:
        """Check if node can accept more requests."""
        return current_requests < self.max_parallel


@runtime_checkable
class NodeProfilesProtocol(Protocol):
    """Node profiles interface for distributed LLM routing.

    Used to route agents to their assigned nodes in distributed deployments.
    """

    def get_profile_for_agent(self, agent_name: str) -> NodeProfile:
        """Get the node profile assigned to an agent.

        Args:
            agent_name: Name of the agent (e.g., "planner", "executor")

        Returns:
            NodeProfile with base_url, model_name, etc.

        Raises:
            KeyError: If no profile is assigned to the agent
        """
        ...

    def list_profiles(self) -> List[NodeProfile]:
        """List all configured node profiles."""
        ...


# =============================================================================
# CAPABILITY LLM CONFIGURATION (Layer Extraction Support)
# =============================================================================

@dataclass
class AgentLLMConfig:
    """LLM configuration for a specific agent.

    Used by capabilities to register their agent configurations with
    the infrastructure layer. This allows infrastructure (avionics) to
    remain capability-agnostic while capabilities own their agent definitions.

    Constitutional Reference:
        - Avionics R3: No Domain Logic
        - Mission System: Domain configs OWNED by capabilities
        - Capability: Capability OWNS all domain-specific configuration
    """
    agent_name: str
    model: str = "qwen2.5-7b-instruct-q4_k_m"
    temperature: Optional[float] = 0.3
    max_tokens: int = 2000
    server_url: Optional[str] = None  # Override for per-agent server
    provider: Optional[str] = None  # Override for per-agent provider (openai, anthropic, etc.)
    timeout_seconds: int = 120
    context_window: int = 16384


@runtime_checkable
class CapabilityLLMConfigRegistryProtocol(Protocol):
    """Registry for capability-owned agent LLM configurations.

    Capabilities register their agent configurations at startup.
    Infrastructure queries the registry instead of having hardcoded agent names.

    Constitutional Reference:
        - Avionics R3: No Domain Logic - infrastructure provides transport, not business logic
        - Capability Constitution R6: Domain Config Ownership

    Usage:
        # At capability startup
        registry.register("my_capability", "my_agent", AgentLLMConfig(
            agent_name="my_agent",
            model="qwen2.5-7b-instruct-q4_k_m",
            temperature=0.3,
        ))

        # In infrastructure (factory.py)
        config = registry.get_agent_config("my_agent")
        if config:
            base_url = config.server_url or default_url
    """

    def register(
        self,
        capability_id: str,
        agent_name: str,
        config: AgentLLMConfig
    ) -> None:
        """Register an agent's LLM configuration.

        Args:
            capability_id: Identifier for the capability (e.g., "my_capability")
            agent_name: Name of the agent (e.g., "my_agent")
            config: LLM configuration for the agent
        """
        ...

    def get_agent_config(self, agent_name: str) -> Optional[AgentLLMConfig]:
        """Get configuration for an agent.

        Args:
            agent_name: Name of the agent

        Returns:
            AgentLLMConfig if registered, None otherwise
        """
        ...

    def list_agents(self) -> List[str]:
        """List all registered agent names.

        Returns:
            List of agent names across all capabilities
        """
        ...

    def get_capability_agents(self, capability_id: str) -> List[AgentLLMConfig]:
        """Get all agent configurations for a capability.

        Args:
            capability_id: Identifier for the capability

        Returns:
            List of AgentLLMConfig for the capability
        """
        ...


@runtime_checkable
class FeatureFlagsProviderProtocol(Protocol):
    """Provider for feature flags at runtime.

    Used to break the circular dependency where memory_module imports
    from avionics.feature_flags. Instead, the provider is injected.

    Constitutional Reference:
        - Memory Module: FORBIDDEN jeeves_memory_module → jeeves_avionics.*
        - Use protocol injection instead of direct imports
    """

    def get_feature_flags(self) -> FeatureFlagsProtocol:
        """Get the current feature flags.

        Returns:
            FeatureFlagsProtocol implementation
        """
        ...


# =============================================================================
# AGENT TOOL ACCESS
# =============================================================================

@runtime_checkable
class AgentToolAccessProtocol(Protocol):
    """Agent tool access control interface (Decision 2:B).

    Enforces which agents can access which tools at runtime.
    Only Traverser should be able to execute tools; others get rejection.
    """

    def can_access(self, agent_name: str, tool_name: str) -> bool:
        """Check if an agent can access a specific tool.

        Args:
            agent_name: Name of the agent requesting access
            tool_name: Name of the tool being accessed

        Returns:
            True if access is allowed, False otherwise
        """
        ...

    def get_allowed_tools(self, agent_name: str) -> List[str]:
        """Get list of tools an agent is allowed to use.

        Args:
            agent_name: Name of the agent

        Returns:
            List of tool names the agent can access
        """
        ...
