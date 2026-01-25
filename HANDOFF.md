# Jeeves-Core Handoff Document

**Purpose:** Complete internal documentation for building capabilities on jeeves-core
**Version:** 2.0.0
**Date:** 2026-01-18

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

- **Pipeline Orchestration**: Sequential and parallel agent execution with cyclic routing
- **LLM Provider Abstraction**: OpenAI, Anthropic, Azure, LlamaServer, LlamaCpp
- **Four-Layer Memory**: Episodic → Event Log → Working Memory → Persistent Cache
- **Tool Registry**: Risk-classified tools with per-agent access control
- **Interrupt System**: Clarification, confirmation, agent review (human-in-the-loop)
- **Enterprise Features**: Rate limiting, circuit breakers, checkpointing

### Key Design Principles

1. **Protocol-First**: All components depend on protocols, not implementations
2. **Configuration Over Code**: Agents defined via config, not inheritance
3. **Layered Architecture**: Strict import boundaries (L0 → L4)
4. **Capability Ownership**: Capabilities own their domain config
5. **Bounded Efficiency**: All operations have resource limits
6. **Go-Only Mode**: Go runtime is authoritative, no Python fallbacks
7. **Fail Loud**: No silent errors, all failures surface immediately

---

## 2. Layer Structure

```
┌─────────────────────────────────────────────────────────────────┐
│ YOUR CAPABILITY (e.g., jeeves-capability-finetuning)            │
│     - Domain-specific agents, tools, configs                    │
├─────────────────────────────────────────────────────────────────┤
│ L4: mission_system                                       │
│     - Orchestration framework, HTTP/gRPC API                    │
│     - Capability registration, adapters                         │
├─────────────────────────────────────────────────────────────────┤
│ L3: avionics                                             │
│     - Infrastructure (LLM providers, DB, Gateway)               │
│     - Tool execution, settings, feature flags                   │
├─────────────────────────────────────────────────────────────────┤
│ L2: memory_module                                        │
│     - Event sourcing, semantic memory, session state            │
│     - Entity graphs, tool metrics                               │
├─────────────────────────────────────────────────────────────────┤
│ L1: control_tower                                        │
│     - OS-like kernel (process lifecycle, resources)             │
│     - Rate limiting, IPC coordination                           │
├─────────────────────────────────────────────────────────────────┤
│ L0: protocols + shared                            │
│     - Type contracts (zero dependencies)                        │
│     - Shared utilities (UUID, logging, serialization)           │
├─────────────────────────────────────────────────────────────────┤
│ GO: coreengine + commbus                                        │
│     - Pipeline orchestration (sequential + parallel)            │
│     - Unified agent execution, communication bus                │
│     - AUTHORITATIVE for envelope operations                     │
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

All protocols are defined in `protocols/` and are `@runtime_checkable`.

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
```

---

## 4. Configuration Types

### 4.1 Agent Configuration

```python
@dataclass
class AgentConfig:
    name: str
    stage_order: int = 0

    # Dependencies (for parallel execution)
    requires: List[str] = field(default_factory=list)  # Hard dependencies
    after: List[str] = field(default_factory=list)     # Soft ordering
    join_strategy: str = "all"  # "all" or "any"

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

    # Edge limits for cyclic routing
    edge_limits: List[EdgeLimit] = field(default_factory=list)

    # Interrupt resume stages (capability sets these)
    clarification_resume_stage: Optional[str] = None
    confirmation_resume_stage: Optional[str] = None
    agent_review_resume_stage: Optional[str] = None

    def get_stage_order(self) -> List[str]: ...
    def get_ready_stages(self, completed: Dict[str, bool]) -> List[str]: ...
```

### 4.3 Routing Rules

```python
@dataclass
class RoutingRule:
    condition: str   # e.g., "verdict == 'proceed'"
    value: Any       # Value to match
    target: str      # Next agent name

@dataclass
class EdgeLimit:
    from_stage: str  # Source stage
    to_stage: str    # Target stage
    max_count: int   # Maximum transitions on this edge
```

---

## 5. Agent Runtime

### 5.1 Agent

The `Agent` is a **configuration-driven agent** (no inheritance required):

```python
@dataclass
class Agent:
    config: AgentConfig
    logger: Logger
    llm: Optional[LLMProvider] = None
    tools: Optional[ToolExecutor] = None
    prompt_registry: Optional[PromptRegistry] = None
    event_context: Optional[EventContext] = None
    use_mock: bool = False

    @property
    def name(self) -> str: ...

    async def process(self, envelope: Envelope) -> Envelope:
        """Main processing: pre_process → LLM/tools → post_process → routing"""
        ...
```

### 5.2 Runtime

The runtime orchestrates agent execution:

```python
@dataclass
class Runtime:
    config: PipelineConfig
    llm_factory: Optional[LLMProviderFactory] = None
    tool_executor: Optional[ToolExecutor] = None
    logger: Optional[Logger] = None
    persistence: Optional[Persistence] = None

    # Main entry point
    async def run(self, envelope: Envelope, thread_id: str = "") -> Envelope:
        """Execute pipeline sequentially."""
        ...

    # Parallel execution
    async def run_parallel(self, envelope: Envelope, thread_id: str = "") -> Envelope:
        """Execute independent stages concurrently."""
        ...

    # Streaming execution
    async def run_with_stream(self, envelope: Envelope,
                              thread_id: str = "") -> AsyncIterator[Tuple[str, Dict]]:
        """Execute with streaming updates."""
        ...

    # Resume after interrupt
    async def resume(self, envelope: Envelope, thread_id: str = "") -> Envelope:
        """Resume after interrupt."""
        ...
```

### 5.3 Envelope

The envelope carries all state through the pipeline:

```python
@dataclass
class Envelope:
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

    # Parallel execution state
    active_stages: Dict[str, bool] = field(default_factory=dict)
    completed_stage_set: Dict[str, bool] = field(default_factory=dict)
    failed_stages: Dict[str, str] = field(default_factory=dict)

    # Audit trail
    processing_history: List[ProcessingRecord] = field(default_factory=list)
    errors: List[Dict] = field(default_factory=list)
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

class ToolAccess(str, Enum):
    NONE = "none"
    READ = "read"
    WRITE = "write"
    ALL = "all"
```

### 6.2 Tool Registration

**Architecture Decision:** `ToolId` enums are CAPABILITY-OWNED. Each capability defines
its own ToolId enum in `tools/catalog.py`.

```python
# In your capability's tools/catalog.py
from enum import Enum
from protocols import ToolCategory, RiskLevel

class ToolId(str, Enum):
    """Capability-owned tool identifiers."""
    MY_TOOL = "my_capability.my_tool"

@catalog.register(
    tool_id=ToolId.MY_TOOL,
    description="What this tool does",
    parameters={"param1": "string", "param2": "int?"},
    category=ToolCategory.COMPOSITE,
    risk_level=RiskLevel.LOW,
)
async def my_tool(param1: str, param2: int = None) -> Dict[str, Any]:
    return {"status": "success", "data": {...}}
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
    findings: List[Finding] = field(default_factory=list)
    context: Dict[str, Any] = field(default_factory=dict)

@dataclass
class Finding:
    finding_id: str
    finding_type: str
    title: str
    description: str
    severity: str = "info"
    evidence: List[Dict] = field(default_factory=list)
```

---

## 8. Event System

### 8.1 Unified Event Schema

```python
class EventCategory(Enum):
    AGENT_LIFECYCLE = "agent_lifecycle"
    TOOL_EXECUTION = "tool_execution"
    PIPELINE_FLOW = "pipeline_flow"
    STAGE_TRANSITION = "stage_transition"
    DOMAIN_EVENT = "domain_event"

@dataclass
class Event:
    event_id: str
    event_type: str
    category: EventCategory
    timestamp_iso: str
    timestamp_ms: int
    request_id: str
    session_id: str
    payload: Dict[str, Any]
```

---

## 9. Interrupt Handling

### 9.1 Interrupt Types

```python
class InterruptKind(str, Enum):
    CLARIFICATION = "clarification"      # Need user input
    CONFIRMATION = "confirmation"        # Approve/deny action
    AGENT_REVIEW = "agent_review"        # Human review of agent output
    CHECKPOINT = "checkpoint"            # State persistence point
    RESOURCE_EXHAUSTED = "resource_exhausted"
    TIMEOUT = "timeout"
    SYSTEM_ERROR = "system_error"
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
    question: Optional[str] = None
    message: Optional[str] = None
    data: Dict[str, Any] = field(default_factory=dict)
    response: Optional[InterruptResponse] = None
    status: InterruptStatus = InterruptStatus.PENDING

@dataclass
class InterruptResponse:
    text: Optional[str] = None
    approved: Optional[bool] = None
    decision: Optional[str] = None
    data: Optional[Dict] = None
```

### 9.3 Configurable Resume Stages

Resume stages are configured per-pipeline, not hardcoded:

```python
PIPELINE_CONFIG = PipelineConfig(
    name="my_pipeline",
    agents=[...],
    # Capability sets these - core doesn't know agent names
    clarification_resume_stage="stage_a",
    confirmation_resume_stage="stage_b",
    agent_review_resume_stage="stage_a",
)
```

---

## 10. Capability Registration

### 10.1 Capability Resource Registry

```python
from protocols.capability import (
    get_capability_resource_registry,
    CapabilityServiceConfig,
    CapabilityModeConfig,
)

registry = get_capability_resource_registry()

registry.register_mode(
    capability_id="finetuning",
    mode_config=CapabilityModeConfig(
        mode_id="finetune",
        response_fields=["model_id", "metrics", "status"],
    )
)
```

---

## 11. Wiring & Integration

### 11.1 Mission System Adapters

```python
from mission_system.adapters import MissionSystemAdapters

adapters = MissionSystemAdapters(
    db=await create_database_client(),
    llm_factory=lambda role: create_llm_provider(role),
    tool_registry=tool_registry,
    settings=get_settings(),
    context_bounds=ContextBounds(),
)
```

---

## 12. Go Engine Integration

### 12.1 Architecture

The Go engine is **AUTHORITATIVE** for:
- Pipeline orchestration (sequential and parallel)
- Envelope state management
- Bounds checking and enforcement
- Communication bus (pub/sub, query/response)

**Go-Only Mode:** Python does not duplicate Go logic. If Go is unavailable, operations fail.

### 12.2 Execution Modes

```go
// Main entry point with options
func (r *Runtime) Execute(ctx context.Context, env *Envelope, opts RunOptions) (*Envelope, <-chan StageOutput, error)

// Convenience methods
func (r *Runtime) Run(ctx, env, threadID) (*Envelope, error)         // Sequential
func (r *Runtime) RunParallel(ctx, env, threadID) (*Envelope, error) // Parallel
func (r *Runtime) RunWithStream(ctx, env, threadID) (<-chan StageOutput, error)
func (r *Runtime) Resume(ctx, env, response, threadID) (*Envelope, error)

// Run mode can be configured in PipelineConfig
type RunMode string // "sequential" or "parallel"
```

### 12.3 Parallel Execution

The runtime supports parallel execution of independent stages:

```go
// GetReadyStages returns stages with all dependencies satisfied
readyStages := config.GetReadyStages(completed)

// Execute ready stages in parallel using goroutines
for _, stage := range readyStages {
    go func(s string) {
        result := agent.Process(ctx, envelope.Clone())
        results <- result
    }(stage)
}
```

Parallel execution uses:
- `AgentConfig.Requires`: Hard dependencies that must complete first
- `AgentConfig.After`: Soft ordering preferences
- `AgentConfig.JoinStrategy`: "all" (wait for all) or "any" (proceed when one completes)

### 12.4 Cyclic Routing

The Go runtime supports cyclic execution via routing rules:

```go
// RoutingRules can route to ANY stage, including earlier ones
RoutingRules: []RoutingRule{
    {Condition: "verdict", Value: "loop_back", Target: "stageA"},  // Loop back!
    {Condition: "verdict", Value: "proceed", Target: "end"},
}

// EdgeLimits control per-edge cycle counts
EdgeLimits: []EdgeLimit{
    {From: "stageC", To: "stageA", MaxCount: 3},  // Max 3 loops
}
```

### 12.5 Loop Verdicts (Generic)

Core uses generic verdict terminology:

```go
type LoopVerdict string

const (
    LoopVerdictProceed  LoopVerdict = "proceed"    // Continue to next stage
    LoopVerdictLoopBack LoopVerdict = "loop_back"  // Return to earlier stage
    LoopVerdictAdvance  LoopVerdict = "advance"    // Skip ahead
)
```

Capability layer maps domain concepts to generic verdicts:
- Domain "satisfied" → `LoopVerdictProceed`
- Domain "retry" → `LoopVerdictLoopBack`

### 12.6 Go Bridge (Go-Only)

```python
from avionics.interop.go_bridge import GoEnvelopeBridge

# Go binary is REQUIRED - raises GoNotAvailableError if not found
bridge = GoEnvelopeBridge()

# All operations go through Go
envelope = bridge.create_envelope(raw_input="...", user_id="...", session_id="...")
can_continue = bridge.can_continue(envelope)
result = bridge.get_result(envelope)
```

### 12.7 gRPC Client (Go-Only)

```python
from protocols.grpc_client import GrpcGoClient

# Go server is REQUIRED - raises GoServerNotRunningError if not available
with GrpcGoClient() as client:
    client.connect()  # Fails fast if server not running
    
    envelope = client.create_envelope(...)
    bounds = client.check_bounds(envelope)
    
    for event in client.execute_pipeline(envelope, thread_id):
        handle_event(event)
```

---

## 13. Building a New Capability

### 13.1 Directory Structure

```
jeeves-capability-finetuning/
├── __init__.py
├── wiring.py              # Capability registration
├── config/
│   ├── agents.py          # Agent LLM configurations
│   ├── pipelines.py       # Pipeline configurations
│   └── tools.py           # Tool access matrix
├── agents/                 # Agent hooks (pre/post process)
├── tools/                  # Tool implementations
├── services/
└── server.py              # Entry point
```

### 13.2 Pipeline Configuration

```python
from protocols import AgentConfig, PipelineConfig, RoutingRule, ToolAccess

STAGE_A = AgentConfig(
    name="stage_a",
    stage_order=1,
    has_llm=True,
    routing_rules=[
        RoutingRule(condition="has_plan", value=True, target="stage_b"),
    ],
    default_next="stage_b",
)

STAGE_B = AgentConfig(
    name="stage_b",
    stage_order=2,
    has_llm=True,
    has_tools=True,
    requires=["stage_a"],  # Waits for stage_a
    default_next="stage_c",
)

STAGE_C = AgentConfig(
    name="stage_c",
    stage_order=3,
    has_llm=True,
    routing_rules=[
        RoutingRule(condition="verdict", value="proceed", target="end"),
        RoutingRule(condition="verdict", value="loop_back", target="stage_a"),
    ],
    default_next="end",
)

PIPELINE = PipelineConfig(
    name="my_pipeline",
    agents=[STAGE_A, STAGE_B, STAGE_C],
    max_iterations=3,
    edge_limits=[
        EdgeLimit(from_stage="stage_c", to_stage="stage_a", max_count=3),
    ],
    clarification_resume_stage="stage_a",
)
```

---

## 14. Testing Patterns

### 14.1 Mock Provider

```python
from protocols import create_pipeline_runner

# Use mock mode for testing
runtime = create_pipeline_runner(
    config=pipeline_config,
    use_mock=True,  # Uses MockProvider
)
```

### 14.2 Integration Test

```python
@pytest.mark.asyncio
async def test_pipeline():
    runtime = create_pipeline_runner(config=PIPELINE, use_mock=True)
    envelope = create_envelope(raw_input="Test", user_id="test")
    
    result = await runtime.run(envelope)
    
    assert result.current_stage == "end"
```

---

## 15. Quick Reference

### 15.1 Import Cheatsheet

```python
# Protocols (L0)
from protocols import (
    AgentConfig, PipelineConfig, Envelope, RoutingRule,
    ContextBounds, LoopVerdict, InterruptKind,
    Agent, Runtime,
    create_pipeline_runner, create_envelope,
)

# Go Bridge (Go-only)
from avionics.interop.go_bridge import GoEnvelopeBridge
from protocols.grpc_client import GrpcGoClient
```

### 15.2 Pipeline Pattern

```
start → stageA → stageB → stageC → end
                   ↑          │
                   └──[retry]─┘
         ↑                    │
         └────[loop_back]─────┘
```

### 15.3 Result Contract

```python
# Tool result
{
    "status": "success" | "error" | "not_found",
    "data": {...},
    "error": "...",
}

# Agent output (generic verdicts)
{
    "verdict": "proceed" | "loop_back" | "advance" | "escalate",
    "confidence": 0.95,
    "reasoning": "...",
}
```

---

*End of Handoff Document v2.0.0*
