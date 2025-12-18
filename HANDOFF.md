# Jeeves-Core Handoff Document

**Purpose:** Complete internal documentation for building capabilities on jeeves-core
**Version:** 1.0.0
**Date:** 2025-12-18

This document provides everything needed to build a new capability (such as a finetuning capability) on top of jeeves-core. It covers architecture, protocols, wiring, and integration patterns.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Layer Structure](#2-layer-structure)
3. [Core Protocols Reference](#3-core-protocols-reference)
4. [Configuration Types](#4-configuration-types)
5. [Agent Runtime](#5-agent-runtime)
6. [Tool System](#6-tool-system)
7. [Memory Architecture](#7-memory-architecture)
8. [Event System](#8-event-system)
9. [Interrupt Handling](#9-interrupt-handling)
10. [Capability Registration](#10-capability-registration)
11. [Wiring & Integration](#11-wiring--integration)
12. [Go Engine Integration](#12-go-engine-integration)
13. [Building a New Capability](#13-building-a-new-capability)
14. [Testing Patterns](#14-testing-patterns)
15. [Quick Reference](#15-quick-reference)

---

## 1. Architecture Overview

Jeeves-core is a **layered agentic runtime** combining Python application logic with a Go orchestration engine. It provides:

- **Pipeline Orchestration**: Sequential and DAG-based agent execution
- **LLM Provider Abstraction**: OpenAI, Anthropic, Azure, LlamaServer, LlamaCpp
- **Four-Layer Memory**: Episodic → Event Log → Working Memory → Persistent Cache
- **Tool Registry**: Risk-classified tools with per-agent access control
- **Interrupt System**: Clarification, confirmation, human-in-the-loop
- **Enterprise Features**: Rate limiting, circuit breakers, checkpointing

### Key Design Principles

1. **Protocol-First**: All components depend on protocols, not implementations
2. **Configuration Over Code**: Agents defined via config, not inheritance
3. **Layered Architecture**: Strict import boundaries (L0 → L4)
4. **Capability Ownership**: Capabilities own their domain config
5. **Bounded Efficiency**: All operations have resource limits

---

## 2. Layer Structure

```
┌─────────────────────────────────────────────────────────────────┐
│ YOUR CAPABILITY (e.g., jeeves-capability-finetuning)            │
│     - Domain-specific agents, tools, configs                    │
├─────────────────────────────────────────────────────────────────┤
│ L4: jeeves_mission_system                                       │
│     - Orchestration framework, HTTP/gRPC API                    │
│     - Capability registration, adapters                         │
├─────────────────────────────────────────────────────────────────┤
│ L3: jeeves_avionics                                             │
│     - Infrastructure (LLM providers, DB, Gateway)               │
│     - Tool execution, settings, feature flags                   │
├─────────────────────────────────────────────────────────────────┤
│ L2: jeeves_memory_module                                        │
│     - Event sourcing, semantic memory, session state            │
│     - Entity graphs, tool metrics                               │
├─────────────────────────────────────────────────────────────────┤
│ L1: jeeves_control_tower                                        │
│     - OS-like kernel (process lifecycle, resources)             │
│     - Rate limiting, IPC coordination                           │
├─────────────────────────────────────────────────────────────────┤
│ L0: jeeves_protocols + jeeves_shared                            │
│     - Type contracts (zero dependencies)                        │
│     - Shared utilities (UUID, logging, serialization)           │
├─────────────────────────────────────────────────────────────────┤
│ GO: coreengine + commbus                                        │
│     - Pipeline orchestration (DAG executor)                     │
│     - Unified agent execution, communication bus                │
└─────────────────────────────────────────────────────────────────┘
```

### Import Rules

| Layer | May Import From |
|-------|-----------------|
| Capability | L4, L3, L2, L1, L0 |
| L4 (mission_system) | L3, L2, L1, L0 |
| L3 (avionics) | L2, L1, L0 |
| L2 (memory_module) | L1, L0 |
| L1 (control_tower) | L0 only |
| L0 (protocols/shared) | Nothing |

---

## 3. Core Protocols Reference

All protocols are defined in `jeeves_protocols/` and are `@runtime_checkable`.

### 3.1 Logging

```python
class LoggerProtocol(Protocol):
    def info(self, message: str, **kwargs: Any) -> None: ...
    def debug(self, message: str, **kwargs: Any) -> None: ...
    def warning(self, message: str, **kwargs: Any) -> None: ...
    def error(self, message: str, **kwargs: Any) -> None: ...
    def bind(self, **kwargs: Any) -> "LoggerProtocol": ...
```

### 3.2 Persistence

```python
class PersistenceProtocol(Protocol):
    async def execute(self, query: str, params: Optional[Dict] = None) -> None: ...
    async def fetch_one(self, query: str, params: Optional[Dict] = None) -> Optional[Dict]: ...
    async def fetch_all(self, query: str, params: Optional[Dict] = None) -> List[Dict]: ...

class DatabaseClientProtocol(Protocol):
    async def connect(self) -> None: ...
    async def disconnect(self) -> None: ...
    async def execute(self, query: str, params: Optional[Dict] = None) -> None: ...
    async def fetch_one(self, query: str, params: Optional[Dict] = None) -> Optional[Dict]: ...
    async def fetch_all(self, query: str, params: Optional[Dict] = None) -> List[Dict]: ...
```

### 3.3 LLM Provider

```python
class LLMProviderProtocol(Protocol):
    async def generate(self, model: str, prompt: str,
                      options: Optional[Dict] = None) -> str: ...
    async def generate_streaming(self, model: str, prompt: str,
                                options: Optional[Dict] = None) -> Any: ...
```

### 3.4 Tool System

```python
class ToolProtocol(Protocol):
    @property
    def name(self) -> str: ...
    @property
    def description(self) -> str: ...
    async def execute(self, params: Dict[str, Any]) -> Dict[str, Any]: ...

class ToolRegistryProtocol(Protocol):
    def has_tool(self, name: str) -> bool: ...
    def get_tool(self, name: str) -> Optional[ToolDefinitionProtocol]: ...

class ToolExecutorProtocol(Protocol):
    async def execute(self, tool_name: str, params: Dict,
                     context: Optional[Dict] = None) -> Dict: ...
    def get_available_tools(self) -> List[str]: ...
```

### 3.5 Memory & Search

```python
@dataclass
class SearchResult:
    id: str
    content: str
    score: float
    metadata: Dict[str, Any]

class MemoryServiceProtocol(Protocol):
    async def store(self, key: str, value: Any, ttl: Optional[int] = None) -> None: ...
    async def retrieve(self, key: str) -> Optional[Any]: ...
    async def delete(self, key: str) -> None: ...

class SemanticSearchProtocol(Protocol):
    async def search(self, query: str, limit: int = 10,
                    filters: Optional[Dict] = None) -> List[SearchResult]: ...
    async def index(self, id: str, content: str, metadata: Dict) -> None: ...

class SessionStateProtocol(Protocol):
    async def get(self, session_id: str) -> Optional[Dict]: ...
    async def set(self, session_id: str, state: Dict) -> None: ...
    async def delete(self, session_id: str) -> None: ...
```

### 3.6 Vector Storage

```python
class VectorStorageProtocol(Protocol):
    async def upsert(self, item_id: str, content: str, collection: str,
                    metadata: Dict) -> None: ...
    async def search(self, query: str, collections: List[str],
                    filters: Optional[Dict] = None, limit: int = 10) -> List[Dict]: ...
    async def delete(self, item_id: str, collection: str) -> None: ...
    def close(self) -> None: ...
```

### 3.7 Checkpointing

```python
@dataclass
class CheckpointRecord:
    checkpoint_id: str
    envelope_id: str
    agent_name: str
    sequence: int
    state: Dict[str, Any]
    created_at: datetime

class CheckpointProtocol(Protocol):
    async def save(self, envelope_id: str, agent_name: str, state: Dict) -> str: ...
    async def load(self, checkpoint_id: str) -> Optional[CheckpointRecord]: ...
    async def list_for_envelope(self, envelope_id: str) -> List[CheckpointRecord]: ...
    async def replay_to(self, checkpoint_id: str) -> Dict: ...
```

### 3.8 Distributed Systems

```python
@dataclass
class DistributedTask:
    task_id: str
    task_type: str
    payload: Dict[str, Any]
    priority: int = 0
    created_at: Optional[datetime] = None

@dataclass
class QueueStats:
    pending: int
    processing: int
    completed: int
    failed: int

class DistributedBusProtocol(Protocol):
    async def enqueue(self, task: DistributedTask) -> str: ...
    async def dequeue(self, task_type: str, timeout: float = 0) -> Optional[DistributedTask]: ...
    async def complete(self, task_id: str, result: Dict) -> None: ...
    async def fail(self, task_id: str, error: str) -> None: ...
    async def stats(self, task_type: str) -> QueueStats: ...
```

### 3.9 Rate Limiting

```python
@dataclass
class RateLimitConfig:
    requests_per_minute: int = 60
    requests_per_hour: int = 1000
    requests_per_day: int = 10000
    burst_size: int = 10

@dataclass
class RateLimitResult:
    allowed: bool
    exceeded: bool = False
    reason: Optional[str] = None
    retry_after_seconds: float = 0.0
    remaining: int = 0

class RateLimiterProtocol(Protocol):
    def check_rate_limit(self, user_id: str, endpoint: str) -> RateLimitResult: ...
    def set_user_limits(self, user_id: str, config: RateLimitConfig) -> None: ...
```

---

## 4. Configuration Types

### 4.1 Agent Configuration

```python
@dataclass
class AgentConfig:
    name: str
    stage_order: int = 0

    # Capabilities
    has_llm: bool = False
    has_tools: bool = False
    has_policies: bool = False

    # Tool access
    tool_access: ToolAccess = ToolAccess.NONE  # NONE, READ, WRITE, ALL
    allowed_tools: Optional[Set[str]] = None

    # LLM config
    model_role: Optional[str] = None
    prompt_key: Optional[str] = None
    temperature: Optional[float] = None
    max_tokens: Optional[int] = None

    # Output
    output_key: str = ""
    required_output_fields: List[str] = field(default_factory=list)

    # Routing
    routing_rules: List[RoutingRule] = field(default_factory=list)
    default_next: Optional[str] = None
    error_next: Optional[str] = None

    # Bounds
    timeout_seconds: Optional[int] = None
    max_retries: int = 0

    # Hooks (capability layer sets these)
    pre_process: Optional[Callable] = None
    post_process: Optional[Callable] = None
    mock_handler: Optional[Callable] = None
```

### 4.2 Pipeline Configuration

```python
@dataclass
class PipelineConfig:
    name: str
    agents: List[AgentConfig] = field(default_factory=list)

    # Global limits
    max_iterations: int = 3
    max_llm_calls: int = 10
    max_agent_hops: int = 21
    default_timeout_seconds: int = 300

    # Feature flags
    enable_arbiter: bool = True
    skip_arbiter_for_read_only: bool = True

    # Interrupt resume stages
    clarification_resume_stage: Optional[str] = None
    confirmation_resume_stage: Optional[str] = None

    def get_stage_order(self) -> List[str]: ...
```

### 4.3 Routing Rules

```python
@dataclass
class RoutingRule:
    condition: str   # e.g., "verdict == 'approved'"
    value: Any       # Value to match
    target: str      # Next agent name
```

### 4.4 Context Bounds

```python
@dataclass
class ContextBounds:
    max_input_tokens: int = 4096
    max_output_tokens: int = 2048
    max_context_tokens: int = 16384
    reserved_tokens: int = 512
```

### 4.5 Agent LLM Configuration

```python
@dataclass
class AgentLLMConfig:
    agent_name: str
    model: str = "qwen2.5-7b-instruct-q4_k_m"
    temperature: Optional[float] = 0.3
    max_tokens: int = 2000
    server_url: Optional[str] = None      # Per-agent server override
    provider: Optional[str] = None        # Per-agent provider override
    timeout_seconds: int = 120
    context_window: int = 16384
```

---

## 5. Agent Runtime

### 5.1 UnifiedAgent

The `UnifiedAgent` is a **configuration-driven agent** (no inheritance required):

```python
@dataclass
class UnifiedAgent:
    config: AgentConfig
    logger: Logger
    llm: Optional[LLMProvider] = None
    tools: Optional[ToolExecutor] = None
    prompt_registry: Optional[PromptRegistry] = None
    event_context: Optional[EventContext] = None
    use_mock: bool = False
    pre_process: Optional[PreProcessHook] = None
    post_process: Optional[PostProcessHook] = None
    mock_handler: Optional[MockHandler] = None

    @property
    def name(self) -> str: ...

    async def process(self, envelope: GenericEnvelope) -> GenericEnvelope:
        """Main processing: pre_process → LLM/tools → post_process → routing"""
        ...
```

### 5.2 UnifiedRuntime

The runtime orchestrates agent execution:

```python
@dataclass
class UnifiedRuntime:
    config: PipelineConfig
    llm_factory: Optional[LLMProviderFactory] = None
    tool_executor: Optional[ToolExecutor] = None
    logger: Optional[Logger] = None
    persistence: Optional[Persistence] = None
    prompt_registry: Optional[PromptRegistry] = None
    use_mock: bool = False

    async def run(self, envelope: GenericEnvelope, thread_id: str = "") -> GenericEnvelope:
        """Execute pipeline until completion or termination."""
        ...

    async def run_streaming(self, envelope: GenericEnvelope,
                           thread_id: str = "") -> AsyncIterator[Tuple[str, Dict]]:
        """Execute with streaming updates."""
        ...

    async def resume(self, envelope: GenericEnvelope, thread_id: str = "") -> GenericEnvelope:
        """Resume after interrupt."""
        ...
```

### 5.3 GenericEnvelope

The envelope carries all state through the pipeline:

```python
@dataclass
class GenericEnvelope:
    # Identity
    envelope_id: str = ""
    request_id: str = ""
    user_id: str = ""
    session_id: str = ""

    # Input
    raw_input: str = ""
    received_at: Optional[datetime] = None

    # Dynamic outputs (agent_name → output dict)
    outputs: Dict[str, Dict[str, Any]] = field(default_factory=dict)

    # Pipeline state
    current_stage: str = "start"
    stage_order: List[str] = field(default_factory=list)
    iteration: int = 0
    max_iterations: int = 3

    # Bounds tracking
    llm_call_count: int = 0
    max_llm_calls: int = 10
    agent_hop_count: int = 0
    max_agent_hops: int = 21
    terminal_reason: Optional[TerminalReason] = None

    # Interrupt handling
    interrupt_pending: bool = False
    interrupt: Optional[Dict[str, Any]] = None

    # DAG execution
    active_stages: Dict[str, bool] = field(default_factory=dict)
    completed_stage_set: Dict[str, bool] = field(default_factory=dict)
    failed_stages: Dict[str, str] = field(default_factory=dict)
    dag_mode: bool = False

    # Multi-goal tracking
    all_goals: List[str] = field(default_factory=list)
    remaining_goals: List[str] = field(default_factory=list)
    goal_completion_status: Dict[str, str] = field(default_factory=dict)

    # Retry state
    prior_plans: List[Dict] = field(default_factory=list)
    critic_feedback: List[str] = field(default_factory=list)

    # Audit trail
    processing_history: List[ProcessingRecord] = field(default_factory=list)
    errors: List[Dict] = field(default_factory=list)

    # Metadata
    metadata: Dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: Dict) -> "GenericEnvelope": ...
    def to_dict(self) -> Dict: ...
```

### 5.4 Factory Functions

```python
def create_runtime_from_config(
    config: PipelineConfig,
    llm_provider_factory: Optional[LLMProviderFactory] = None,
    tool_executor: Optional[ToolExecutor] = None,
    logger: Optional[Logger] = None,
    persistence: Optional[Persistence] = None,
    prompt_registry: Optional[PromptRegistry] = None,
    use_mock: bool = False,
) -> UnifiedRuntime: ...

def create_generic_envelope(
    raw_input: str,
    user_id: str = "",
    session_id: str = "",
    request_id: str = "",
    metadata: Optional[Dict] = None,
) -> GenericEnvelope: ...
```

---

## 6. Tool System

### 6.1 Core Enums

```python
class RiskLevel(str, Enum):
    READ_ONLY = "read_only"
    WRITE = "write"
    DESTRUCTIVE = "destructive"
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"

class ToolCategory(str, Enum):
    READ = "read"
    WRITE = "write"
    EXECUTE = "execute"
    NETWORK = "network"
    SYSTEM = "system"
    UNIFIED = "unified"
    COMPOSITE = "composite"
    RESILIENT = "resilient"
    STANDALONE = "standalone"

class ToolAccess(str, Enum):
    NONE = "none"
    READ = "read"
    WRITE = "write"
    ALL = "all"
```

### 6.2 Tool Registration

```python
from jeeves_avionics.tools.catalog import ToolCatalog, ToolId

catalog = ToolCatalog.get_instance()

@catalog.register(
    tool_id=ToolId.MY_TOOL,  # Or use string "my_capability.my_tool"
    description="What this tool does",
    parameters={"param1": "string", "param2": "int?"},  # ? = optional
    category=ToolCategory.COMPOSITE,
    risk_level=RiskLevel.LOW,
)
async def my_tool(param1: str, param2: int = None) -> Dict[str, Any]:
    """Tool implementation."""
    return {
        "status": "success",
        "data": {...},
        "citations": [...]
    }
```

### 6.3 Tool Execution with Access Control

```python
from jeeves_avionics.wiring import ToolExecutor, AgentContext

executor = ToolExecutor(registry=tool_registry, logger=logger)

# Context for access enforcement
context = AgentContext(
    agent_name="MyTraverserAgent",
    request_id="req-123",
    session_id="sess-456"
)

# Execute with access control
result = await executor.execute_with_context(
    tool_id=ToolId.MY_TOOL,
    params={"param1": "value"},
    context=context
)

# Result structure
{
    "status": "success" | "error" | "rejected" | "not_found" | "partial",
    "data": {...},
    "error": "...",  # If error
    "error_type": "...",
    "execution_time_ms": 123
}
```

### 6.4 Resilient Tool Mapping

```python
# Built-in fallback strategies
RESILIENT_OPS_MAP = {
    "read_file": "read_code",        # Base → Resilient
    "find_symbol": "locate",
    "find_similar_files": "find_related",
}

# Automatically uses resilient version
result = await executor.execute_resilient("read_file", params)
```

---

## 7. Memory Architecture

### 7.1 Four-Layer Memory

| Layer | Scope | Storage | Purpose |
|-------|-------|---------|---------|
| L1 Episodic | Per-request | In-memory | Working memory during execution |
| L2 Event Log | Permanent | PostgreSQL | Audit trail, replay |
| L3 Working Memory | Per-session | PostgreSQL | Session state persistence |
| L4 Persistent Cache | Permanent | PostgreSQL + pgvector | Semantic search |

### 7.2 Working Memory

```python
@dataclass
class WorkingMemory:
    session_id: str
    user_id: str
    current_focus: Optional[FocusState] = None
    focus_history: List[FocusState] = field(default_factory=list)
    entities: List[EntityRef] = field(default_factory=list)
    findings: List[Finding] = field(default_factory=list)
    context: Dict[str, Any] = field(default_factory=dict)

    def add_entity(self, entity: EntityRef) -> None: ...
    def add_finding(self, finding: Finding) -> None: ...
    def set_focus(self, focus: FocusState) -> None: ...

@dataclass
class FocusState:
    focus_type: FocusType  # FILE, FUNCTION, CLASS, MODULE, CONCEPT
    focus_id: str
    focus_name: str
    context: Dict[str, Any]

@dataclass
class Finding:
    finding_id: str
    finding_type: str
    title: str
    description: str
    severity: str = "info"
    evidence: List[Dict] = field(default_factory=list)

@dataclass
class EntityRef:
    entity_type: str
    entity_id: str
    name: str
    context: Optional[str] = None
```

### 7.3 Memory Operations

```python
from jeeves_protocols.memory import (
    create_working_memory,
    merge_working_memory,
    set_focus,
    add_entity_ref,
    serialize_working_memory,
    deserialize_working_memory,
)

# Create memory
memory = create_working_memory(session_id="sess-123", user_id="user-456")

# Add focus
memory = set_focus(memory, FocusState(
    focus_type=FocusType.FILE,
    focus_id="src/main.py",
    focus_name="main.py",
    context={}
))

# Merge memories
combined = merge_working_memory(memory1, memory2)

# Serialize/deserialize
data = serialize_working_memory(memory)
memory = deserialize_working_memory(data)
```

---

## 8. Event System

### 8.1 Unified Event Schema

```python
class EventCategory(Enum):
    AGENT_LIFECYCLE = "agent_lifecycle"
    TOOL_EXECUTION = "tool_execution"
    PIPELINE_FLOW = "pipeline_flow"
    CRITIC_DECISION = "critic_decision"
    STAGE_TRANSITION = "stage_transition"
    DOMAIN_EVENT = "domain_event"
    SESSION_EVENT = "session_event"

class EventSeverity(Enum):
    DEBUG = "debug"
    INFO = "info"
    WARNING = "warning"
    ERROR = "error"
    CRITICAL = "critical"

@dataclass
class UnifiedEvent:
    event_id: str
    event_type: str
    category: EventCategory
    timestamp_iso: str
    timestamp_ms: int
    request_id: str
    session_id: str
    payload: Dict[str, Any]
    severity: EventSeverity = EventSeverity.INFO
    source: str = "unknown"
    correlation_id: Optional[str] = None

    @classmethod
    def create_now(cls, event_type: str, category: EventCategory,
                   request_id: str, session_id: str,
                   payload: Optional[Dict] = None) -> "UnifiedEvent": ...
```

### 8.2 Standard Event Types

```python
class StandardEventTypes:
    # Agent lifecycle
    AGENT_STARTED = "agent.started"
    AGENT_COMPLETED = "agent.completed"
    AGENT_FAILED = "agent.failed"

    # Tool execution
    TOOL_STARTED = "tool.started"
    TOOL_COMPLETED = "tool.completed"
    TOOL_FAILED = "tool.failed"

    # Pipeline flow
    FLOW_STARTED = "flow.started"
    FLOW_COMPLETED = "flow.completed"
    FLOW_ERROR = "flow.error"
    STAGE_TRANSITION = "flow.stage_transition"
```

### 8.3 Event Emitter Protocol

```python
class EventEmitterProtocol(Protocol):
    async def emit(self, event: UnifiedEvent) -> None: ...
    async def subscribe(self, pattern: str,
                       handler: Callable[[UnifiedEvent], Awaitable[None]]) -> str: ...
    async def unsubscribe(self, subscription_id: str) -> None: ...
```

---

## 9. Interrupt Handling

### 9.1 Interrupt Types

```python
class InterruptKind(str, Enum):
    CLARIFICATION = "clarification"      # Need user input
    CONFIRMATION = "confirmation"        # Approve/deny action
    CRITIC_REVIEW = "critic_review"      # Agent requests re-evaluation
    CHECKPOINT = "checkpoint"            # State persistence point
    RESOURCE_EXHAUSTED = "resource_exhausted"
    TIMEOUT = "timeout"
    SYSTEM_ERROR = "system_error"

class InterruptStatus(str, Enum):
    PENDING = "pending"
    RESOLVED = "resolved"
    EXPIRED = "expired"
    CANCELLED = "cancelled"
```

### 9.2 Flow Interrupt

```python
@dataclass
class FlowInterrupt:
    id: str
    kind: InterruptKind
    request_id: str
    user_id: str
    session_id: str
    envelope_id: Optional[str] = None
    question: Optional[str] = None
    message: Optional[str] = None
    data: Dict[str, Any] = field(default_factory=dict)
    response: Optional[InterruptResponse] = None
    status: InterruptStatus = InterruptStatus.PENDING
    created_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    expires_at: Optional[datetime] = None

@dataclass
class InterruptResponse:
    text: Optional[str] = None
    approved: Optional[bool] = None
    decision: Optional[str] = None
    data: Optional[Dict] = None
```

### 9.3 Interrupt Service Protocol

```python
class InterruptServiceProtocol(Protocol):
    async def create_interrupt(self, kind: InterruptKind, request_id: str,
                              user_id: str, session_id: str,
                              question: Optional[str] = None,
                              data: Optional[Dict] = None) -> FlowInterrupt: ...

    async def get_interrupt(self, interrupt_id: str) -> Optional[FlowInterrupt]: ...

    async def respond(self, interrupt_id: str,
                     response: InterruptResponse) -> Optional[FlowInterrupt]: ...

    async def list_pending(self, user_id: Optional[str] = None) -> List[FlowInterrupt]: ...

    async def cancel(self, interrupt_id: str) -> bool: ...
```

### 9.4 Using Interrupts in Pipelines

```python
# In pipeline, check for pending interrupt
if envelope.interrupt_pending:
    # Wait for resolution
    return envelope

# Create clarification interrupt
envelope.interrupt_pending = True
envelope.interrupt = {
    "kind": InterruptKind.CLARIFICATION.value,
    "question": "Which approach do you prefer?",
    "options": ["option_a", "option_b"],
}

# Resume after response
envelope = await runtime.resume(envelope, thread_id)
```

---

## 10. Capability Registration

### 10.1 Capability Resource Registry

```python
from jeeves_protocols.capability import (
    get_capability_resource_registry,
    CapabilityServiceConfig,
    CapabilityModeConfig,
    CapabilityOrchestratorConfig,
    CapabilityToolsConfig,
    CapabilityPromptConfig,
    CapabilityAgentConfig,
    CapabilityContractsConfig,
)

registry = get_capability_resource_registry()
```

### 10.2 Registration Functions

```python
# Register service (for Control Tower dispatch)
registry.register_service(
    capability_id="finetuning",
    service_config=CapabilityServiceConfig(
        service_id="finetuning_service",
        service_type="flow",
        capabilities=["finetuning"],
        max_concurrent=10,
        is_default=False
    )
)

# Register mode
registry.register_mode(
    capability_id="finetuning",
    mode_config=CapabilityModeConfig(
        mode_id="finetune",
        response_fields=["model_id", "metrics", "status"],
        requires_repo_path=False
    )
)

# Register orchestrator factory
registry.register_orchestrator(
    capability_id="finetuning",
    config=CapabilityOrchestratorConfig(
        factory=lambda llm_factory, tool_executor, logger, persistence, control_tower:
            create_finetuning_service(...),
        result_type=FinetuningResult
    )
)

# Register tools
registry.register_tools(
    capability_id="finetuning",
    config=CapabilityToolsConfig(
        initializer=lambda db: {"catalog": tool_catalog},
        tool_ids=["prepare_dataset", "train_model", "evaluate_model"]
    )
)

# Register prompts
registry.register_prompts(
    capability_id="finetuning",
    prompts=[
        CapabilityPromptConfig(
            prompt_id="finetuning_planner",
            version="1.0",
            description="System prompt for finetuning planner",
            prompt_factory=lambda: "You are a model finetuning expert..."
        )
    ]
)

# Register agents (for governance discovery)
registry.register_agents(
    capability_id="finetuning",
    agents=[
        CapabilityAgentConfig(
            name="finetuning_planner",
            description="Plans finetuning workflows",
            layer="planning",
            tools=["prepare_dataset", "train_model"]
        )
    ]
)

# Register contracts
registry.register_contracts(
    capability_id="finetuning",
    config=CapabilityContractsConfig(
        schemas={"train_model": TrainingResult},
        validators={"train_model": validate_training_result}
    )
)
```

### 10.3 LLM Configuration Registry

```python
from jeeves_avionics.capability_registry import get_capability_registry
from jeeves_protocols import AgentLLMConfig

llm_registry = get_capability_registry()

# Register agent LLM configurations
llm_registry.register(
    capability_id="finetuning",
    agent_name="finetuning_planner",
    config=AgentLLMConfig(
        agent_name="finetuning_planner",
        model="qwen2.5-7b-instruct-q4_k_m",
        temperature=0.3,
        max_tokens=2000,
        timeout_seconds=120,
    )
)

llm_registry.register(
    capability_id="finetuning",
    agent_name="finetuning_executor",
    config=AgentLLMConfig(
        agent_name="finetuning_executor",
        model="qwen2.5-7b-instruct-q4_k_m",
        temperature=0.1,
        max_tokens=4000,
        timeout_seconds=300,
    )
)

# Query (used by infrastructure)
config = llm_registry.get_agent_config("finetuning_planner")
```

---

## 11. Wiring & Integration

### 11.1 Mission System Adapters

```python
from jeeves_mission_system.adapters import (
    MissionSystemAdapters,
    get_logger,
    get_settings,
    create_database_client,
    create_event_emitter,
    create_embedding_service,
    create_nli_service,
    create_vector_adapter,
)

# Create adapters
adapters = MissionSystemAdapters(
    db=await create_database_client(),
    llm_factory=lambda role: create_llm_provider(role),
    tool_registry=tool_registry,
    settings=get_settings(),
    context_bounds=ContextBounds(),
)

# Access services
llm = adapters.get_llm_provider("planner")
db = adapters.db
settings = adapters.settings
```

### 11.2 LLM Provider Factory

```python
from jeeves_avionics.wiring import create_llm_provider_factory
from jeeves_avionics.llm.factory import (
    create_llm_provider,
    create_agent_provider,
    LLMFactory,
)

# Simple factory
llm_factory = create_llm_provider_factory(settings=settings)
llm = llm_factory("planner")

# With node awareness (distributed)
llm = create_agent_provider_with_node_awareness(
    settings=settings,
    agent_name="planner",
    node_profiles=node_profiles
)

# Factory class with caching
factory = LLMFactory(settings=settings, node_profiles=node_profiles)
llm = factory.get_provider_for_agent("planner", use_cache=True)
```

### 11.3 Tool Executor

```python
from jeeves_avionics.wiring import ToolExecutor, create_tool_executor, AgentContext

executor = create_tool_executor(registry=tool_registry)

# Direct execution
result = await executor.execute("my_tool", {"param": "value"})

# With access control
context = AgentContext(agent_name="MyAgent", request_id="req-123")
result = await executor.execute_with_context(ToolId.MY_TOOL, params, context)

# Resilient (with fallback)
result = await executor.execute_resilient("read_file", params)
```

---

## 12. Go Engine Integration

### 12.1 Architecture

The Go engine provides:
- **Pipeline orchestration** (sequential and DAG)
- **Envelope state management**
- **Communication bus** (pub/sub, query/response)

### 12.2 CLI Interface

Python communicates with Go via subprocess:

```bash
# Create envelope
echo '{"raw_input": "...", "user_id": "..."}' | go-envelope create

# Process through pipeline
echo '{"envelope": {...}, "config": {...}}' | go-envelope process

# Validate envelope
echo '{"...}' | go-envelope validate

# Check if can continue
echo '{"...}' | go-envelope can-continue

# Get result for API response
echo '{"...}' | go-envelope result
```

### 12.3 Go Structures (Mirror Python)

```go
// GenericEnvelope - mirrors Python
type GenericEnvelope struct {
    EnvelopeID     string
    RequestID      string
    UserID         string
    SessionID      string
    RawInput       string
    Outputs        map[string]map[string]any
    CurrentStage   string
    StageOrder     []string
    Iteration      int
    MaxIterations  int
    LLMCallCount   int
    MaxLLMCalls    int
    AgentHopCount  int
    MaxAgentHops   int
    InterruptPending bool
    Interrupt      *FlowInterrupt
    // ... DAG state
}

// AgentConfig - mirrors Python
type AgentConfig struct {
    Name        string
    StageOrder  int
    HasLLM      bool
    HasTools    bool
    HasPolicies bool
    ToolAccess  string
    AllowedTools map[string]bool
    RoutingRules []RoutingRule
    // ...
}
```

### 12.4 Go Runtime

```go
// UnifiedRuntime executes pipelines
type UnifiedRuntime struct {
    Config        *PipelineConfig
    LLMFactory    LLMProviderFactory
    ToolExecutor  ToolExecutor
    Logger        Logger
    Persistence   PersistenceAdapter
    dagExecutor   *DAGExecutor
}

func (r *UnifiedRuntime) Run(ctx context.Context, env *GenericEnvelope, threadID string) (*GenericEnvelope, error)
func (r *UnifiedRuntime) RunStreaming(ctx context.Context, env *GenericEnvelope, threadID string) <-chan StageOutput
func (r *UnifiedRuntime) Resume(ctx context.Context, env *GenericEnvelope, response *InterruptResponse, threadID string) (*GenericEnvelope, error)
```

### 12.5 DAG Executor

```go
// DAGExecutor for parallel execution
type DAGExecutor struct {
    agents      map[string]*UnifiedAgent
    pipeline    *PipelineConfig
    maxParallel int
}

func (d *DAGExecutor) Execute(ctx context.Context, env *GenericEnvelope) (*GenericEnvelope, error)
```

---

## 13. Building a New Capability

### 13.1 Directory Structure

```
jeeves-capability-finetuning/
├── __init__.py
├── wiring.py              # Capability registration
├── config/
│   ├── __init__.py
│   ├── agents.py          # Agent LLM configurations
│   ├── pipelines.py       # Pipeline configurations
│   └── tools.py           # Tool access matrix
├── agents/
│   ├── __init__.py
│   ├── planner.py         # Planner agent hooks
│   ├── executor.py        # Executor agent hooks
│   └── critic.py          # Critic agent hooks
├── tools/
│   ├── __init__.py
│   ├── dataset.py         # Dataset tools
│   ├── training.py        # Training tools
│   └── evaluation.py      # Evaluation tools
├── services/
│   ├── __init__.py
│   └── finetuning_service.py
├── contracts/
│   ├── __init__.py
│   └── schemas.py         # Result schemas
├── prompts/
│   ├── __init__.py
│   └── templates.py       # Prompt templates
└── server.py              # Entry point
```

### 13.2 wiring.py Template

```python
"""Capability registration for finetuning."""
from jeeves_protocols import (
    get_capability_resource_registry,
    CapabilityModeConfig,
    CapabilityServiceConfig,
    CapabilityAgentConfig,
)
from jeeves_avionics.capability_registry import get_capability_registry

from .config.agents import AGENT_LLM_CONFIGS
from .tools import register_tools

CAPABILITY_ID = "finetuning"

def register_capability():
    """Register finetuning capability with jeeves-core."""

    # 1. Register LLM configurations
    llm_registry = get_capability_registry()
    for agent_name, config in AGENT_LLM_CONFIGS.items():
        llm_registry.register(
            capability_id=CAPABILITY_ID,
            agent_name=agent_name,
            config=config,
        )

    # 2. Register capability resources
    resource_registry = get_capability_resource_registry()

    resource_registry.register_mode(
        CAPABILITY_ID,
        CapabilityModeConfig(
            mode_id="finetune",
            response_fields=["model_id", "metrics"],
        )
    )

    resource_registry.register_service(
        CAPABILITY_ID,
        CapabilityServiceConfig(
            service_id="finetuning_service",
            service_type="flow",
            capabilities=["finetuning"],
            max_concurrent=5,
        )
    )

    # 3. Register tools
    register_tools()
```

### 13.3 config/agents.py Template

```python
"""Agent LLM configurations for finetuning capability."""
from jeeves_protocols import AgentLLMConfig

AGENT_LLM_CONFIGS = {
    "finetuning_planner": AgentLLMConfig(
        agent_name="finetuning_planner",
        model="qwen2.5-7b-instruct-q4_k_m",
        temperature=0.3,
        max_tokens=2000,
        timeout_seconds=120,
    ),
    "finetuning_executor": AgentLLMConfig(
        agent_name="finetuning_executor",
        model="qwen2.5-7b-instruct-q4_k_m",
        temperature=0.1,
        max_tokens=4000,
        timeout_seconds=300,
    ),
    "finetuning_critic": AgentLLMConfig(
        agent_name="finetuning_critic",
        model="qwen2.5-7b-instruct-q4_k_m",
        temperature=0.1,
        max_tokens=1500,
        timeout_seconds=60,
    ),
}
```

### 13.4 config/pipelines.py Template

```python
"""Pipeline configurations for finetuning capability."""
from jeeves_protocols import AgentConfig, PipelineConfig, RoutingRule, ToolAccess

# Agent configurations
PLANNER_CONFIG = AgentConfig(
    name="finetuning_planner",
    stage_order=1,
    has_llm=True,
    has_tools=False,
    tool_access=ToolAccess.NONE,
    prompt_key="finetuning_planner",
    output_key="plan",
    routing_rules=[
        RoutingRule(condition="has_plan", value=True, target="finetuning_executor"),
        RoutingRule(condition="needs_clarification", value=True, target="clarification"),
    ],
    default_next="finetuning_executor",
)

EXECUTOR_CONFIG = AgentConfig(
    name="finetuning_executor",
    stage_order=2,
    has_llm=True,
    has_tools=True,
    tool_access=ToolAccess.ALL,
    allowed_tools={"prepare_dataset", "train_model", "evaluate_model"},
    prompt_key="finetuning_executor",
    output_key="execution",
    default_next="finetuning_critic",
)

CRITIC_CONFIG = AgentConfig(
    name="finetuning_critic",
    stage_order=3,
    has_llm=True,
    has_tools=False,
    tool_access=ToolAccess.NONE,
    prompt_key="finetuning_critic",
    output_key="verdict",
    routing_rules=[
        RoutingRule(condition="verdict", value="satisfied", target="end"),
        RoutingRule(condition="verdict", value="retry", target="finetuning_executor"),
        RoutingRule(condition="verdict", value="replan", target="finetuning_planner"),
    ],
    default_next="end",
)

# Pipeline configuration
FINETUNING_PIPELINE = PipelineConfig(
    name="finetuning",
    agents=[PLANNER_CONFIG, EXECUTOR_CONFIG, CRITIC_CONFIG],
    max_iterations=3,
    max_llm_calls=15,
    max_agent_hops=21,
    clarification_resume_stage="finetuning_planner",
)
```

### 13.5 tools/__init__.py Template

```python
"""Tool registration for finetuning capability."""
from jeeves_avionics.tools.catalog import ToolCatalog
from jeeves_protocols import ToolCategory, RiskLevel

from .dataset import prepare_dataset
from .training import train_model
from .evaluation import evaluate_model

def register_tools():
    """Register finetuning tools with catalog."""
    catalog = ToolCatalog.get_instance()

    @catalog.register(
        tool_id="finetuning.prepare_dataset",
        description="Prepare and validate training dataset",
        parameters={
            "data_path": "string",
            "format": "string?",
            "validation_split": "float?",
        },
        category=ToolCategory.WRITE,
        risk_level=RiskLevel.MEDIUM,
    )
    async def _prepare_dataset(data_path: str, format: str = "jsonl",
                               validation_split: float = 0.1):
        return await prepare_dataset(data_path, format, validation_split)

    @catalog.register(
        tool_id="finetuning.train_model",
        description="Train/finetune a model",
        parameters={
            "base_model": "string",
            "dataset_id": "string",
            "epochs": "int?",
            "learning_rate": "float?",
        },
        category=ToolCategory.EXECUTE,
        risk_level=RiskLevel.HIGH,
    )
    async def _train_model(base_model: str, dataset_id: str,
                          epochs: int = 3, learning_rate: float = 2e-5):
        return await train_model(base_model, dataset_id, epochs, learning_rate)

    @catalog.register(
        tool_id="finetuning.evaluate_model",
        description="Evaluate trained model",
        parameters={
            "model_id": "string",
            "eval_dataset": "string?",
        },
        category=ToolCategory.READ,
        risk_level=RiskLevel.LOW,
    )
    async def _evaluate_model(model_id: str, eval_dataset: str = None):
        return await evaluate_model(model_id, eval_dataset)
```

### 13.6 server.py Template

```python
"""Entry point for finetuning capability."""
import asyncio
from .wiring import register_capability

async def main():
    # 1. Register capability first
    register_capability()

    # 2. Import runtime after registration
    from jeeves_mission_system.adapters import get_logger, get_settings
    from jeeves_avionics.wiring import create_llm_provider_factory, create_tool_executor
    from jeeves_protocols import create_runtime_from_config

    from .config.pipelines import FINETUNING_PIPELINE

    logger = get_logger()
    settings = get_settings()

    logger.info("finetuning_capability_started")

    # 3. Create runtime
    llm_factory = create_llm_provider_factory(settings)
    tool_executor = create_tool_executor(tool_registry)

    runtime = create_runtime_from_config(
        config=FINETUNING_PIPELINE,
        llm_provider_factory=llm_factory,
        tool_executor=tool_executor,
        logger=logger,
    )

    # 4. Start server or process requests
    # ...

if __name__ == "__main__":
    asyncio.run(main())
```

---

## 14. Testing Patterns

### 14.1 Mock Provider

```python
from jeeves_protocols import create_runtime_from_config

# Use mock mode for testing
runtime = create_runtime_from_config(
    config=pipeline_config,
    use_mock=True,  # Uses MockProvider
)

# Custom mock handler
def my_mock_handler(envelope: GenericEnvelope) -> Dict[str, Any]:
    return {"verdict": "satisfied", "confidence": 0.9}

agent_config.mock_handler = my_mock_handler
```

### 14.2 Test Fixtures

```python
import pytest
from jeeves_protocols import GenericEnvelope, create_generic_envelope

@pytest.fixture
def test_envelope():
    return create_generic_envelope(
        raw_input="Test input",
        user_id="test-user",
        session_id="test-session",
    )

@pytest.fixture
def mock_llm_factory():
    async def mock_generate(model, prompt, options=None):
        return '{"verdict": "satisfied"}'
    return lambda role: MockProvider(mock_generate)
```

### 14.3 Integration Test

```python
import pytest
from jeeves_protocols import create_runtime_from_config, create_generic_envelope

@pytest.mark.asyncio
async def test_finetuning_pipeline():
    from finetuning.config.pipelines import FINETUNING_PIPELINE
    from finetuning.wiring import register_capability

    register_capability()

    runtime = create_runtime_from_config(
        config=FINETUNING_PIPELINE,
        use_mock=True,
    )

    envelope = create_generic_envelope(
        raw_input="Finetune GPT-2 on my dataset",
        user_id="test",
    )

    result = await runtime.run(envelope)

    assert result.current_stage == "end"
    assert "verdict" in result.outputs.get("finetuning_critic", {})
```

---

## 15. Quick Reference

### 15.1 Import Cheatsheet

```python
# Protocols (L0)
from jeeves_protocols import (
    # Types
    AgentConfig, PipelineConfig, GenericEnvelope, RoutingRule,
    ContextBounds, AgentLLMConfig,

    # Enums
    RiskLevel, ToolAccess, ToolCategory, TerminalReason,
    InterruptKind, InterruptStatus,

    # Runtime
    UnifiedAgent, UnifiedRuntime,
    create_runtime_from_config, create_generic_envelope,

    # Memory
    WorkingMemory, FocusState, Finding,
    create_working_memory, merge_working_memory,

    # Events
    UnifiedEvent, EventCategory, EventSeverity,

    # Capability registration
    get_capability_resource_registry, CapabilityModeConfig,
    CapabilityServiceConfig, CapabilityAgentConfig,
)

# Mission System (L4)
from jeeves_mission_system.adapters import (
    get_logger, get_settings, get_feature_flags,
    create_database_client, create_event_emitter,
    MissionSystemAdapters,
)

# Avionics (L3)
from jeeves_avionics.capability_registry import get_capability_registry
from jeeves_avionics.wiring import (
    ToolExecutor, AgentContext,
    create_tool_executor, create_llm_provider_factory,
)
from jeeves_avionics.tools.catalog import ToolCatalog, ToolId
from jeeves_avionics.llm.factory import create_llm_provider
```

### 15.2 Pipeline Pattern

```
start → planner → executor → critic → end
                    ↑          │
                    └──[retry]─┘
           ↑                    │
           └────[replan]────────┘
```

### 15.3 Result Contract

```python
# Tool result
{
    "status": "success" | "error" | "not_found" | "partial",
    "data": {...},
    "error": "...",
    "error_type": "...",
    "citations": [...],
    "execution_time_ms": 123
}

# Agent output
{
    "verdict": "satisfied" | "retry" | "replan" | "escalate",
    "confidence": 0.95,
    "reasoning": "...",
    "next_steps": [...]
}
```

### 15.4 Environment Variables

```bash
# LLM Provider
LLM_PROVIDER=llamaserver
LLAMASERVER_HOST=http://localhost:8080
DEFAULT_MODEL=qwen2.5-7b-instruct-q4_k_m

# Database
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_DATABASE=jeeves
POSTGRES_USER=postgres
POSTGRES_PASSWORD=...

# Per-agent overrides
PLANNER_MODEL=gpt-4
PLANNER_TEMPERATURE=0.3
LLAMASERVER_PLANNER_URL=http://node1:8080
```

---

## Appendix A: Complete Protocol List

| Protocol | Location | Purpose |
|----------|----------|---------|
| `LoggerProtocol` | protocols.py | Structured logging |
| `PersistenceProtocol` | protocols.py | Database operations |
| `DatabaseClientProtocol` | protocols.py | Full DB client |
| `VectorStorageProtocol` | protocols.py | Vector embeddings |
| `LLMProviderProtocol` | protocols.py | LLM completions |
| `ToolProtocol` | protocols.py | Individual tool |
| `ToolRegistryProtocol` | protocols.py | Tool registry |
| `ToolExecutorProtocol` | protocols.py | Tool execution |
| `SettingsProtocol` | protocols.py | Settings storage |
| `FeatureFlagsProtocol` | protocols.py | Feature flags |
| `ClockProtocol` | protocols.py | Time abstraction |
| `MemoryServiceProtocol` | protocols.py | Memory storage |
| `SemanticSearchProtocol` | protocols.py | Vector search |
| `SessionStateProtocol` | protocols.py | Session state |
| `CheckpointProtocol` | protocols.py | State checkpoints |
| `DistributedBusProtocol` | protocols.py | Task queues |
| `EventBusProtocol` | protocols.py | Event pub/sub |
| `EventEmitterProtocol` | events.py | Event emission |
| `InterruptServiceProtocol` | interrupts.py | Flow interrupts |
| `RateLimiterProtocol` | interrupts.py | Rate limiting |
| `CapabilityLLMConfigRegistryProtocol` | protocols.py | Agent LLM configs |
| `AgentToolAccessProtocol` | protocols.py | Tool access control |
| `CapabilityResourceRegistryProtocol` | capability.py | Resource registration |

---

## Appendix B: File Locations

| Component | Path |
|-----------|------|
| Protocols | `jeeves_protocols/` |
| Shared utilities | `jeeves_shared/` |
| Control Tower (kernel) | `jeeves_control_tower/` |
| Memory Module | `jeeves_memory_module/` |
| Avionics (infrastructure) | `jeeves_avionics/` |
| Mission System (API) | `jeeves_mission_system/` |
| Go engine | `coreengine/` |
| Go comm bus | `commbus/` |
| Go CLI | `cmd/envelope/` |
| Docker | `docker/` |
| Tests | `tests/`, `*/tests/` |
| Main contract | `CONTRACT.md` |
| Integration guide | `docs/CAPABILITY_LAYER_INTEGRATION.md` |
| Architectural contracts | `docs/CONTRACTS.md` |

---

*End of Handoff Document*
