"""Protocol definitions - interfaces for dependency injection.

These are typing.Protocol classes for static type checking.
Implementations are in Go or Python adapters.
"""

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional, Protocol, runtime_checkable, ClassVar


# =============================================================================
# REQUEST CONTEXT
# =============================================================================

@dataclass(frozen=True)
class RequestContext:
    """Immutable request context for tracing and logging.

    Used with ContextVars for async-safe request tracking (ADR-001 Decision 5).

    Usage:
        ctx = RequestContext(
            request_id=str(uuid4()),
            capability="code_analysis",
            user_id="user-123",
        )
        with request_scope(ctx, logger):
            # All code in this scope has access to context
            ...
    """
    request_id: str
    capability: str
    session_id: Optional[str] = None
    user_id: Optional[str] = None
    agent_role: Optional[str] = None
    trace_id: Optional[str] = None
    span_id: Optional[str] = None
    tags: Dict[str, str] = field(default_factory=dict)

    # Guardrails to prevent schema creep in tags
    MAX_TAGS: ClassVar[int] = 16
    MAX_TAG_KEY_LENGTH: ClassVar[int] = 64
    MAX_TAG_VALUE_LENGTH: ClassVar[int] = 256

    def __post_init__(self) -> None:
        if not isinstance(self.request_id, str) or not self.request_id.strip():
            raise ValueError("request_id is required and must be a non-empty string")
        if not isinstance(self.capability, str) or not self.capability.strip():
            raise ValueError("capability is required and must be a non-empty string")

        for field_name in ("session_id", "user_id", "agent_role", "trace_id", "span_id"):
            value = getattr(self, field_name)
            if value is not None and not isinstance(value, str):
                raise TypeError(f"{field_name} must be a string or None")

        if not isinstance(self.tags, dict):
            raise TypeError("tags must be a dict of string keys and values")
        if len(self.tags) > self.MAX_TAGS:
            raise ValueError(f"tags exceed max count ({self.MAX_TAGS})")
        for key, value in self.tags.items():
            if not isinstance(key, str) or not key.strip():
                raise ValueError("tag keys must be non-empty strings")
            if not isinstance(value, str):
                raise ValueError("tag values must be strings")
            if len(key) > self.MAX_TAG_KEY_LENGTH:
                raise ValueError(f"tag key exceeds max length ({self.MAX_TAG_KEY_LENGTH})")
            if len(value) > self.MAX_TAG_VALUE_LENGTH:
                raise ValueError(f"tag value exceeds max length ({self.MAX_TAG_VALUE_LENGTH})")

    def to_dict(self) -> Dict[str, Any]:
        """Convert to a serializable dictionary."""
        return {
            "request_id": self.request_id,
            "capability": self.capability,
            "session_id": self.session_id,
            "user_id": self.user_id,
            "agent_role": self.agent_role,
            "trace_id": self.trace_id,
            "span_id": self.span_id,
            "tags": self.tags,
        }


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

    async def generate_stream(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> Any: ...  # AsyncIterator[TokenChunk]

    async def health_check(self) -> bool: ...


# =============================================================================
# TOOLS
# =============================================================================

@runtime_checkable
class ToolProtocol(Protocol):
    """Tool interface for individual tool implementations."""

    @property
    def name(self) -> str: ...

    @property
    def description(self) -> str: ...

    async def execute(self, params: Dict[str, Any]) -> Dict[str, Any]: ...


@runtime_checkable
class ToolDefinitionProtocol(Protocol):
    """Tool definition returned by registry lookups.

    This is the minimal interface needed by ToolExecutor to invoke a tool.
    Implementations may include additional fields (parameters, category, etc.)
    but these are the required properties.

    Constitutional Reference:
        - Avionics CONSTITUTION: R4 (Swappable Implementations)
        - Capability CONSTITUTION: P5 (Configuration over Code)
    """
    name: str
    function: Any  # Callable - using Any for protocol compatibility
    parameters: Dict[str, str]
    description: str


@runtime_checkable
class ToolRegistryProtocol(Protocol):
    """Tool registry interface for managing and accessing tools.

    Updated to match actual usage patterns in wiring.py.
    Supports both the catalog pattern (ToolCatalog) and legacy tool registration.

    Constitutional Reference:
        - Avionics CONSTITUTION: R4 (Swappable Implementations)
        - Protocols should reflect actual usage, not theoretical ideals
    """

    def has_tool(self, name: str) -> bool:
        """Check if a tool is registered.

        Args:
            name: Tool name (string)

        Returns:
            True if tool exists and has implementation
        """
        ...

    def get_tool(self, name: str) -> Optional[ToolDefinitionProtocol]:
        """Get tool definition by name.

        Args:
            name: Tool name (string)

        Returns:
            ToolDefinitionProtocol if found, None otherwise
        """
        ...


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
    """Checkpoint record for time-travel debugging.

    Design Philosophy:
    - Metadata and state are separated (state not included in record)
    - Explicit parent chain for traversing execution history
    - Optional metadata for observability (duration, memory, etc.)
    - stage_order provides sequential ordering within envelope

    Usage:
        # Save checkpoint returns full record
        record = await adapter.save_checkpoint(
            envelope_id="env_123",
            checkpoint_id="ckpt_abc",
            agent_name="planner",
            state={"foo": "bar"},
            metadata={"duration_ms": 1500},
        )

        # List checkpoints returns metadata only (efficient)
        records = await adapter.list_checkpoints("env_123")

        # Load checkpoint returns just state (for restoration)
        state = await adapter.load_checkpoint("ckpt_abc")
    """
    checkpoint_id: str
    envelope_id: str
    agent_name: str
    stage_order: int  # Sequential order within envelope (0, 1, 2, ...)
    created_at: datetime
    parent_checkpoint_id: Optional[str] = None  # Links to previous checkpoint in chain
    metadata: Optional[Dict[str, Any]] = None  # Optional debug metadata (duration, memory, etc.)


@runtime_checkable
class CheckpointProtocol(Protocol):
    """Checkpoint interface for time-travel debugging.

    Methods:
        save_checkpoint: Save execution checkpoint after agent completion
        load_checkpoint: Load checkpoint state for restoration
        list_checkpoints: List all checkpoints for an envelope (timeline)
        delete_checkpoints: Delete checkpoints for cleanup
        fork_from_checkpoint: Create new execution branch from checkpoint
    """

    async def save_checkpoint(
        self,
        envelope_id: str,
        checkpoint_id: str,
        agent_name: str,
        state: Dict[str, Any],
        metadata: Optional[Dict[str, Any]] = None,
    ) -> CheckpointRecord:
        """Save execution checkpoint.

        Returns:
            CheckpointRecord with metadata (state not included)
        """
        ...

    async def load_checkpoint(self, checkpoint_id: str) -> Optional[Dict[str, Any]]:
        """Load checkpoint state for restoration.

        Returns:
            State dict, or None if not found
        """
        ...

    async def list_checkpoints(
        self,
        envelope_id: str,
        limit: int = 100,
    ) -> List[CheckpointRecord]:
        """List all checkpoints for an envelope (execution timeline).

        Returns:
            Ordered list of checkpoints (oldest first)
        """
        ...

    async def delete_checkpoints(
        self,
        envelope_id: str,
        before_checkpoint_id: Optional[str] = None,
    ) -> int:
        """Delete checkpoints for cleanup.

        Returns:
            Number of checkpoints deleted
        """
        ...

    async def fork_from_checkpoint(
        self,
        checkpoint_id: str,
        new_envelope_id: str,
    ) -> str:
        """Create new execution branch from checkpoint (time-travel replay).

        Returns:
            New checkpoint_id for forked branch root
        """
        ...


# =============================================================================
# DISTRIBUTED BUS
# =============================================================================

@dataclass
class DistributedTask:
    """Task for distributed agent pipeline execution.

    Design Philosophy:
    - Specific to agent pipeline execution (not generic task queue)
    - Contains envelope state for agent resumption
    - Retry logic built-in (retry_count, max_retries)
    - Checkpoint-aware for consistency

    Usage:
        task = DistributedTask(
            task_id="task_123",
            envelope_state=envelope.to_state_dict(),
            agent_name="planner",
            stage_order=1,
            priority=5,  # Higher = more important
        )
        await bus.enqueue_task("agent:planner", task)
    """
    task_id: str
    envelope_state: Dict[str, Any]  # Full envelope state for agent execution
    agent_name: str  # Agent to execute
    stage_order: int  # Sequential position in pipeline
    checkpoint_id: Optional[str] = None  # Optional checkpoint for recovery
    created_at: Optional[str] = None  # ISO timestamp
    retry_count: int = 0  # Current retry attempt
    max_retries: int = 3  # Maximum retry attempts
    priority: int = 0  # Higher number = higher priority


@dataclass
class QueueStats:
    """Queue statistics for monitoring.

    Fields:
        queue_name: Name of the queue
        pending_count: Tasks waiting in queue
        in_progress_count: Tasks currently being processed
        completed_count: Total tasks completed
        failed_count: Total tasks failed (after retries)
        avg_processing_time_ms: Average processing time in milliseconds
        workers_active: Number of active workers for this queue
    """
    queue_name: str
    pending_count: int
    in_progress_count: int
    completed_count: int
    failed_count: int
    avg_processing_time_ms: float = 0.0
    workers_active: int = 0


@runtime_checkable
class DistributedBusProtocol(Protocol):
    """Distributed message bus interface for horizontal scaling.

    Methods:
        enqueue_task: Add task to queue for worker processing
        dequeue_task: Get next task from queue (blocking)
        complete_task: Mark task as successfully completed
        fail_task: Mark task as failed, optionally retry
        register_worker: Register worker with capabilities
        deregister_worker: Deregister worker and requeue tasks
        heartbeat: Send worker heartbeat to prevent timeout
        get_queue_stats: Get queue statistics for monitoring
        list_queues: List all active queues
    """

    async def enqueue_task(self, queue_name: str, task: DistributedTask) -> str:
        """Enqueue task for worker processing.

        Returns:
            Task ID for tracking
        """
        ...

    async def dequeue_task(
        self,
        queue_name: str,
        worker_id: str,
        timeout_seconds: int = 30,
    ) -> Optional[DistributedTask]:
        """Dequeue task for processing (blocking with timeout).

        Returns:
            Task to process, or None if timeout
        """
        ...

    async def complete_task(self, task_id: str, result: Dict[str, Any]) -> None:
        """Mark task as completed with result."""
        ...

    async def fail_task(self, task_id: str, error: str, retry: bool = True) -> None:
        """Mark task as failed, optionally retry."""
        ...

    async def register_worker(self, worker_id: str, capabilities: List[str]) -> None:
        """Register worker with capabilities."""
        ...

    async def deregister_worker(self, worker_id: str) -> None:
        """Deregister worker (on shutdown)."""
        ...

    async def heartbeat(self, worker_id: str) -> None:
        """Send worker heartbeat."""
        ...

    async def get_queue_stats(self, queue_name: str) -> QueueStats:
        """Get queue statistics for monitoring."""
        ...

    async def list_queues(self) -> List[str]:
        """List all active queues."""
        ...
    async def stats(self, task_type: str) -> QueueStats: ...


# =============================================================================
# NLI (Natural Language Interface)
# =============================================================================

@runtime_checkable
class IntentParsingProtocol(Protocol):
    """Intent parsing service for natural language understanding."""

    async def parse_intent(self, text: str) -> Dict[str, Any]: ...
    async def generate_response(self, intent: Dict[str, Any], context: Dict[str, Any]) -> str: ...


@runtime_checkable
class ClaimVerificationProtocol(Protocol):
    """Claim verification service using Natural Language Inference.

    Verifies that claims are entailed by their cited evidence.
    Used by memory_module.services.nli_service.NLIService.
    """

    def verify_claim(self, claim: str, evidence: str) -> Any: ...
    def verify_claims_batch(self, claims: List[tuple]) -> List[Any]: ...


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
class InferenceEndpoint:
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
class InferenceEndpointsProtocol(Protocol):
    """Node profiles interface for distributed LLM routing.

    Used to route agents to their assigned nodes in distributed deployments.
    """

    def get_profile_for_agent(self, agent_name: str) -> InferenceEndpoint:
        """Get the node profile assigned to an agent.

        Args:
            agent_name: Name of the agent (e.g., "planner", "executor")

        Returns:
            InferenceEndpoint with base_url, model_name, etc.

        Raises:
            KeyError: If no profile is assigned to the agent
        """
        ...

    def list_profiles(self) -> List[InferenceEndpoint]:
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
class DomainLLMRegistryProtocol(Protocol):
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
        - Memory Module: FORBIDDEN memory_module → avionics.*
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


# =============================================================================
# MEMORY LAYER PROTOCOLS (L5-L6)
# =============================================================================


@runtime_checkable
class GraphStorageProtocol(Protocol):
    """L5 Entity Graph storage interface.

    Provides graph-based storage for entity relationships.
    This is the extensible protocol for L5 memory layer.

    Constitutional Reference:
    - Memory Module CONSTITUTION: L5 Graph - Entity relationships

    Use Cases:
    - Entity relationship tracking (files, functions, classes)
    - Dependency graphs
    - Knowledge graphs for reasoning

    Implementations can be:
    - In-memory graph (for testing)
    - Neo4j adapter
    - PostgreSQL with recursive CTEs
    - Custom graph databases
    """

    async def add_node(
        self,
        node_id: str,
        node_type: str,
        properties: Dict[str, Any],
        user_id: Optional[str] = None,
    ) -> bool:
        """Add a node to the graph.

        Args:
            node_id: Unique node identifier
            node_type: Type of node (e.g., "file", "function", "class")
            properties: Node properties/attributes
            user_id: Optional owner user ID

        Returns:
            True if created, False if already exists
        """
        ...

    async def add_edge(
        self,
        source_id: str,
        target_id: str,
        edge_type: str,
        properties: Optional[Dict[str, Any]] = None,
    ) -> bool:
        """Add an edge between nodes.

        Args:
            source_id: Source node ID
            target_id: Target node ID
            edge_type: Type of relationship (e.g., "imports", "calls", "inherits")
            properties: Optional edge properties

        Returns:
            True if created, False if already exists
        """
        ...

    async def get_node(self, node_id: str) -> Optional[Dict[str, Any]]:
        """Get a node by ID.

        Args:
            node_id: Node identifier

        Returns:
            Node data with properties, or None if not found
        """
        ...

    async def get_neighbors(
        self,
        node_id: str,
        edge_type: Optional[str] = None,
        direction: str = "both",
        limit: int = 100,
    ) -> List[Dict[str, Any]]:
        """Get neighboring nodes.

        Args:
            node_id: Center node ID
            edge_type: Optional filter by edge type
            direction: "in", "out", or "both"
            limit: Maximum neighbors to return

        Returns:
            List of neighbor nodes with edge information
        """
        ...

    async def find_path(
        self,
        source_id: str,
        target_id: str,
        max_depth: int = 5,
    ) -> Optional[List[Dict[str, Any]]]:
        """Find path between two nodes.

        Args:
            source_id: Start node
            target_id: End node
            max_depth: Maximum path length

        Returns:
            Path as list of nodes, or None if no path exists
        """
        ...

    async def query_subgraph(
        self,
        center_id: str,
        depth: int = 2,
        node_types: Optional[List[str]] = None,
        edge_types: Optional[List[str]] = None,
    ) -> Dict[str, Any]:
        """Query a subgraph around a center node.

        Args:
            center_id: Center node ID
            depth: Expansion depth
            node_types: Optional filter by node types
            edge_types: Optional filter by edge types

        Returns:
            Subgraph with nodes and edges
        """
        ...

    async def delete_node(self, node_id: str) -> bool:
        """Delete a node and its edges.

        Args:
            node_id: Node to delete

        Returns:
            True if deleted, False if not found
        """
        ...


@runtime_checkable
class SkillStorageProtocol(Protocol):
    """L6 Skills/Patterns storage interface.

    Provides storage for learned patterns and reusable skills.
    This is the extensible protocol for L6 memory layer.

    Constitutional Reference:
    - Memory Module CONSTITUTION: L6 Skills - Learned patterns (not yet implemented)

    Use Cases:
    - Tool usage patterns (what worked before)
    - Code generation templates
    - User preference learning
    - Successful prompt patterns

    Skills differ from other memory layers:
    - They're learned/extracted, not directly stored
    - They have confidence scores
    - They can be promoted/demoted based on success
    """

    async def store_skill(
        self,
        skill_id: str,
        skill_type: str,
        pattern: Dict[str, Any],
        source_context: Optional[Dict[str, Any]] = None,
        confidence: float = 0.5,
        user_id: Optional[str] = None,
    ) -> str:
        """Store a learned skill/pattern.

        Args:
            skill_id: Unique skill identifier
            skill_type: Type of skill (e.g., "tool_sequence", "code_pattern", "prompt_template")
            pattern: The skill pattern data
            source_context: Optional context where skill was learned
            confidence: Initial confidence score (0.0 to 1.0)
            user_id: Optional owner user ID

        Returns:
            Skill ID
        """
        ...

    async def get_skill(self, skill_id: str) -> Optional[Dict[str, Any]]:
        """Get a skill by ID.

        Args:
            skill_id: Skill identifier

        Returns:
            Skill data or None if not found
        """
        ...

    async def find_skills(
        self,
        skill_type: Optional[str] = None,
        context: Optional[Dict[str, Any]] = None,
        min_confidence: float = 0.0,
        limit: int = 10,
        user_id: Optional[str] = None,
    ) -> List[Dict[str, Any]]:
        """Find relevant skills.

        Args:
            skill_type: Optional filter by skill type
            context: Optional context for relevance matching
            min_confidence: Minimum confidence threshold
            limit: Maximum skills to return
            user_id: Optional filter by owner

        Returns:
            List of matching skills, ordered by relevance/confidence
        """
        ...

    async def update_confidence(
        self,
        skill_id: str,
        delta: float,
        reason: Optional[str] = None,
    ) -> float:
        """Update skill confidence based on outcome.

        Args:
            skill_id: Skill to update
            delta: Confidence change (-1.0 to 1.0)
            reason: Optional reason for update

        Returns:
            New confidence value
        """
        ...

    async def record_usage(
        self,
        skill_id: str,
        success: bool,
        context: Optional[Dict[str, Any]] = None,
    ) -> None:
        """Record skill usage for learning.

        Args:
            skill_id: Skill that was used
            success: Whether the usage was successful
            context: Optional usage context
        """
        ...

    async def delete_skill(self, skill_id: str) -> bool:
        """Delete a skill.

        Args:
            skill_id: Skill to delete

        Returns:
            True if deleted, False if not found
        """
        ...

    async def get_skill_stats(self, skill_id: str) -> Optional[Dict[str, Any]]:
        """Get usage statistics for a skill.

        Args:
            skill_id: Skill identifier

        Returns:
            Statistics including usage count, success rate, etc.
        """
        ...


# =============================================================================
# INFRASTRUCTURE PROTOCOLS (for jeeves-infra layer injection)
# =============================================================================


@runtime_checkable
class WebSocketManagerProtocol(Protocol):
    """WebSocket event streaming interface.

    Manages WebSocket connections for real-time event broadcasting.
    Implemented by jeeves-infra/gateway, injected into kernel.
    """

    async def broadcast(self, event_type: str, payload: Dict[str, Any]) -> None:
        """Broadcast an event to all connected WebSocket clients.

        Args:
            event_type: Type of event (e.g., "agent_started", "tool_completed")
            payload: Event payload data
        """
        ...

    @property
    def connection_count(self) -> int:
        """Return the number of active WebSocket connections."""
        ...


@runtime_checkable
class EmbeddingServiceProtocol(Protocol):
    """Embedding generation interface.

    Generates vector embeddings for text content.
    Implemented by jeeves-infra/memory/services, injected into kernel.
    """

    def embed(self, content: str) -> List[float]:
        """Generate embedding vector for content.

        Args:
            content: Text content to embed

        Returns:
            Embedding vector as list of floats
        """
        ...

    def embed_batch(self, contents: List[str]) -> List[List[float]]:
        """Generate embeddings for multiple contents.

        Args:
            contents: List of text contents

        Returns:
            List of embedding vectors
        """
        ...


@runtime_checkable
class EventBridgeProtocol(Protocol):
    """Bridge for kernel events to external systems.

    Allows kernel to emit events without knowing about specific
    event bus implementations (gateway, monitoring, etc.).
    Implemented by jeeves-infra/gateway, injected into kernel.
    """

    async def emit(self, event_type: str, payload: Dict[str, Any]) -> None:
        """Emit an event to external systems.

        Args:
            event_type: Type of event
            payload: Event payload data
        """
        ...


@runtime_checkable
class ChunkServiceProtocol(Protocol):
    """Document chunking interface.

    Splits documents into chunks for semantic indexing.
    Implemented by jeeves-infra/memory/services, injected into kernel.
    """

    async def chunk_document(
        self,
        content: str,
        metadata: Dict[str, Any],
    ) -> List[Dict[str, Any]]:
        """Split document into chunks with embeddings.

        Args:
            content: Document content
            metadata: Document metadata

        Returns:
            List of chunk dictionaries with content and embeddings
        """
        ...


@runtime_checkable
class SessionStateServiceProtocol(Protocol):
    """Session state persistence interface.

    Manages session state storage and retrieval.
    Implemented by jeeves-infra/memory/services, injected into kernel.
    """

    async def get_state(self, session_id: str) -> Optional[Dict[str, Any]]:
        """Get session state.

        Args:
            session_id: Session identifier

        Returns:
            Session state dictionary or None if not found
        """
        ...

    async def set_state(self, session_id: str, state: Dict[str, Any]) -> None:
        """Set session state.

        Args:
            session_id: Session identifier
            state: State dictionary to store
        """
        ...

    async def delete_state(self, session_id: str) -> bool:
        """Delete session state.

        Args:
            session_id: Session identifier

        Returns:
            True if deleted, False if not found
        """
        ...
