# mission_system.orchestrator - Orchestration Framework

**Package:** `mission_system.orchestrator`  
**Purpose:** Flow orchestration, event emission, pipeline execution  
**Updated:** 2026-01-23

---

## Overview

The `orchestrator` package provides the core orchestration framework for the 7-agent pipeline. It handles:
- Agent lifecycle management
- Event emission (real-time + audit)
- Multi-stage workflow execution
- Vertical service registration

---

## Module Index

| Module | Description |
|--------|-------------|
| `__init__.py` | Package exports (EventOrchestrator, event types) |
| `events.py` | Unified EventOrchestrator facade |
| `agent_events.py` | EventEmitter for real-time streaming |
| `event_context.py` | EventContext for unified emission |
| `flow_service.py` | JeevesFlowServicer (gRPC interface) |
| `governance_service.py` | HealthServicer (tool health, agent status) |
| `vertical_service.py` | Vertical registration and management |
| `state/` | JeevesState TypedDict definitions |

---

## Key Classes and Functions

### `EventOrchestrator` (from `events.py`)

Unified facade for all event operations.

```python
@dataclass
class EventOrchestrator:
    """
    Unified orchestrator for all event operations.
    
    Provides single facade over:
    - EventEmitter (real-time streaming via gRPC)
    - EventContext (unified emission context)
    - EventEmitter (domain event persistence)
    
    Attributes:
        session_id: str
        request_id: str
        user_id: str
        event_repository: Optional[EventRepository]
        enable_streaming: bool
        enable_persistence: bool
    """
```

**Key Methods:**

```python
# Agent lifecycle events
await orchestrator.emit_agent_started("planner")
await orchestrator.emit_agent_completed("planner", status="success")

# Tool events
await orchestrator.emit_tool_started("grep_search", params={"query": "test"})
await orchestrator.emit_tool_completed("grep_search", status="success")

# Domain events (audit only)
await orchestrator.emit_plan_created(plan_id="p123", intent="find code", ...)
await orchestrator.emit_critic_decision(action="approved", confidence=0.95)

# Streaming access
async for event in orchestrator.events():
    yield convert_to_flow_event(event)
```

### `create_event_orchestrator()`

Factory function to create an EventOrchestrator.

```python
def create_event_orchestrator(
    session_id: str,
    request_id: str,
    user_id: str,
    event_repository: Optional[EventRepository] = None,
    enable_streaming: bool = True,
    enable_persistence: bool = True,
) -> EventOrchestrator
```

---

## Event Types (`agent_events.py`)

### `AgentEventType` Enum

```python
class AgentEventType(Enum):
    # Agent lifecycle
    AGENT_STARTED = "agent_started"
    AGENT_COMPLETED = "agent_completed"
    
    # Specific agent events
    PERCEPTION_STARTED = "perception.started"
    PERCEPTION_COMPLETED = "perception.completed"
    INTENT_STARTED = "intent.started"
    INTENT_COMPLETED = "intent.completed"
    PLANNER_STARTED = "planner.started"
    PLANNER_COMPLETED = "planner.plan_created"
    TRAVERSER_STARTED = "traverser.started"
    TRAVERSER_COMPLETED = "traverser.completed"
    TOOL_STARTED = "executor.tool_started"
    TOOL_COMPLETED = "executor.tool_completed"
    SYNTHESIZER_STARTED = "synthesizer.started"
    SYNTHESIZER_COMPLETED = "synthesizer.completed"
    CRITIC_STARTED = "critic.started"
    CRITIC_DECISION = "critic.decision"
    INTEGRATION_STARTED = "integration.started"
    INTEGRATION_COMPLETED = "integration.completed"
    STAGE_TRANSITION = "orchestrator.stage_transition"
    
    # Flow events
    FLOW_STARTED = "orchestrator.started"
    FLOW_COMPLETED = "orchestrator.completed"
    FLOW_ERROR = "orchestrator.error"
```

### `AgentEvent` Dataclass

```python
@dataclass
class AgentEvent:
    """An event emitted by an agent during processing."""
    event_type: AgentEventType
    agent_name: str
    session_id: str
    request_id: str
    timestamp_ms: int
    payload: Dict[str, Any]
    
    def to_dict(self) -> Dict[str, Any]
    def to_json(self) -> str
```

### `EventEmitter`

Async queue-based event emitter for agent activity.

```python
class EventEmitter:
    """Queue-based event emitter for frontend streaming."""
    
    async def emit(self, event: AgentEvent) -> None
    async def emit_agent_started(self, agent_name: str, session_id: str, request_id: str, **payload)
    async def emit_agent_completed(self, agent_name: str, session_id: str, request_id: str, **payload)
    async def emit_tool_started(self, tool_name: str, session_id: str, request_id: str, **payload)
    async def emit_tool_completed(self, tool_name: str, session_id: str, request_id: str, **payload)
    async def emit_stage_transition(self, session_id: str, request_id: str, from_stage: int, to_stage: int, ...)
    async def close(self) -> None
    async def events(self) -> AsyncIterator[AgentEvent]
```

---

## Event Context (`event_context.py`)

### `EventContext`

Unified context for agent event emission.

```python
@dataclass
class EventContext:
    """
    Unified context for agent event emission.
    
    Bridges real-time EventEmitter with domain EventEmitter.
    
    Attributes:
        session_id: str
        request_id: str
        user_id: str
        agent_event_emitter: Optional[EventEmitter]
        domain_event_emitter: Optional[EventEmitter]
        correlation_id: Optional[str]
    """
```

**Methods:**

| Method | Description |
|--------|-------------|
| `emit_agent_started()` | Emit agent started event (real-time + audit) |
| `emit_agent_completed()` | Emit agent completed event |
| `emit_plan_created()` | Emit plan created event (audit) |
| `emit_critic_decision()` | Emit critic decision event (audit) |
| `emit_tool_started()` | Emit tool started event with parameter preview |
| `emit_tool_completed()` | Emit tool completed event |
| `emit_retry_attempt()` | Emit retry attempt event (audit) |
| `emit_response_drafted()` | Emit response drafted event (audit) |
| `emit_stage_transition()` | Emit multi-stage transition event |
| `emit_flow_started()` | Emit flow started event |
| `emit_flow_completed()` | Emit flow completed event |
| `emit_flow_error()` | Emit flow error event |
| `emit_agent_reasoning()` | Emit agent reasoning/CoT event (config-gated) |

---

## State Management (`state/state.py`)

### `JeevesState` TypedDict

State container that mirrors Envelope structure.

```python
class JeevesState(TypedDict, total=False):
    # Identity (immutable per workflow)
    envelope_id: str
    request_id: str
    user_id: str
    session_id: str
    
    # Original Input
    raw_input: str
    received_at: str
    
    # Agent Outputs
    perception: Optional[Dict[str, Any]]
    intent: Optional[Dict[str, Any]]
    plan: Optional[Dict[str, Any]]
    arbiter: Optional[Dict[str, Any]]
    execution: Optional[Dict[str, Any]]
    synthesizer: Optional[Dict[str, Any]]
    critic: Optional[Dict[str, Any]]
    integration: Optional[Dict[str, Any]]
    
    # Flow Control
    stage: str
    iteration: int
    
    # Multi-step Support
    current_step_index: int
    step_results: List[Dict[str, Any]]
    
    # Interrupts
    confirmation_required: bool
    clarification_required: bool
    
    # Termination
    terminated: bool
    terminal_reason: Optional[str]
    
    # Loop Context
    loop_count: int
    loop_feedback_for_intent: Optional[Dict[str, Any]]
    
    # Multi-Stage
    completed_stages: List[Dict[str, Any]]
    current_stage: int
    max_stages: int
    
    # Observability
    confidence_history: List[Dict[str, Any]]
    routing_decisions: List[Dict[str, Any]]
    errors: List[Dict[str, Any]]
```

### `REDUCER_FIELDS`

Fields requiring list concatenation during merges:
- `step_results`
- `prior_plans`
- `loop_feedback`
- `completed_stages`
- `confidence_history`
- `routing_decisions`
- `errors`

### `create_initial_state()`

```python
def create_initial_state(
    user_id: str,
    session_id: str,
    raw_input: str,
    envelope_id: str,
    request_id: str,
    received_at: str,
    metadata: Optional[Dict[str, Any]] = None,
) -> JeevesState
```

---

## gRPC Services

### `JeevesFlowServicer` (`flow_service.py`)

gRPC implementation for code analysis pipeline.

```python
class JeevesFlowServicer(jeeves_pb2_grpc.JeevesFlowServiceServicer):
    """
    gRPC implementation of JeevesFlowService.
    
    Services:
    - StartFlow: Process code analysis queries
    - GetSession: Session retrieval
    - ListSessions: Session listing
    - CreateSession: Create new session
    - DeleteSession: Delete session
    - GetSessionMessages: Get messages for session
    """
```

### `HealthServicer` (`governance_service.py`)

gRPC implementation for system introspection (L7).

```python
class HealthServicer(jeeves_pb2_grpc.GovernanceServiceServicer):
    """
    gRPC implementation of GovernanceService.
    
    Services:
    - GetHealthSummary: Overall system health
    - GetToolHealth: Individual tool health
    - GetAgents: List all agents in pipeline
    - GetMemoryLayers: Status of L1-L7 memory layers
    """
```

---

## Pipeline Architecture

### REINTENT Flow

```
Perception → Intent → Planner → Traverser → Synthesizer → Critic
                ↑                                           │
                └─────────[reintent if not approved]────────┘
```

### Critic Verdicts

| Verdict | Action |
|---------|--------|
| `APPROVED` | All goals satisfied → Integration |
| `REINTENT` | Needs re-analysis → Intent (with hints) |
| `NEXT_STAGE` | Goal satisfied, more remain → Planner |

### Multi-Stage Execution

1. Intent extracts multiple goals from query
2. Each stage addresses remaining goals
3. Critic assesses progress after each stage
4. Loop until all goals satisfied OR max stages reached

---

## Interrupt Integration

The orchestrator integrates with the unified interrupt system via `protocols.interrupts`:

| Kind | Description |
|------|-------------|
| `CLARIFICATION` | User clarification needed |
| `CONFIRMATION` | Approval required |
| `AGENT_REVIEW` | Agent review decision point |
| `CHECKPOINT` | Checkpoint save point |
| `RESOURCE_EXHAUSTED` | Quota exceeded |

---

## Navigation

- [← Back to README](README.md)
- [← API Documentation](api.md)
- [→ Events Documentation](events.md)
- [→ Services Documentation](services.md)
