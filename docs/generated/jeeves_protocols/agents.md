# protocols.agents

**Layer**: L0 (Foundation)  
**Purpose**: Unified Agent Runtime - Go-backed pipeline execution

## Overview

This module provides the unified agent architecture where:
- **Go (coreengine/)**: Envelope state, bounds checking, pipeline graph
- **Python (this file)**: Agent execution, LLM calls, tool execution
- **Bridge (grpc_client.py)**: JSON-over-stdio / gRPC communication

## Protocols

### LLMProvider

Protocol for LLM providers.

```python
class LLMProvider(Protocol):
    async def complete(self, messages: List[Dict[str, str]], **kwargs) -> str:
        ...
```

---

### ToolExecutor

Protocol for tool execution.

```python
class ToolExecutor(Protocol):
    async def execute(self, tool_name: str, params: Dict[str, Any]) -> Dict[str, Any]:
        ...
```

---

### Logger

Protocol for structured logging.

```python
class Logger(Protocol):
    def info(self, event: str, **kwargs) -> None: ...
    def warn(self, event: str, **kwargs) -> None: ...
    def error(self, event: str, **kwargs) -> None: ...
    def bind(self, **kwargs) -> "Logger": ...
```

---

### Persistence

Protocol for state persistence.

```python
class Persistence(Protocol):
    async def save_state(self, thread_id: str, state: Dict[str, Any]) -> None: ...
    async def load_state(self, thread_id: str) -> Optional[Dict[str, Any]]: ...
```

---

### PromptRegistry

Protocol for prompt retrieval.

```python
class PromptRegistry(Protocol):
    def get(self, key: str, **kwargs) -> str: ...
```

---

### EventContext

Protocol for event emission.

```python
class EventContext(Protocol):
    async def emit_agent_started(self, agent_name: str) -> None: ...
    async def emit_agent_completed(self, agent_name: str, status: str, **kwargs) -> None: ...
```

---

## Type Aliases

```python
LLMProviderFactory = Callable[[str], LLMProvider]
PreProcessHook = Callable[[Envelope, Optional[Agent]], Envelope]
PostProcessHook = Callable[[Envelope, Dict[str, Any], Optional[Agent]], Envelope]
MockHandler = Callable[[Envelope], Dict[str, Any]]
```

---

## Classes

### AgentCapability

Agent capability flags.

```python
class AgentCapability:
    LLM = "llm"
    TOOLS = "tools"
    POLICIES = "policies"
```

---

### Agent

Unified agent driven by configuration. Agents are configuration-driven - no subclassing required. Behavior is determined by config flags and hooks.

```python
@dataclass
class Agent:
    config: AgentConfig                        # Agent configuration
    logger: Logger                             # Structured logger
    llm: Optional[LLMProvider] = None          # LLM provider
    tools: Optional[ToolExecutor] = None       # Tool executor
    prompt_registry: Optional[PromptRegistry] = None
    event_context: Optional[EventContext] = None
    use_mock: bool = False                     # Use mock handler
    
    # Hooks
    pre_process: Optional[PreProcessHook] = None
    post_process: Optional[PostProcessHook] = None
    mock_handler: Optional[MockHandler] = None
```

**Properties**:

| Property | Type | Description |
|----------|------|-------------|
| `name` | `str` | Agent name from config |

**Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `set_event_context` | `(ctx: EventContext) -> None` | Set event context for emissions |
| `process` | `(envelope: Envelope) -> Envelope` | Process envelope through this agent |

**Process Flow**:
1. Emit agent started event
2. Run pre-process hook (if defined)
3. Execute agent logic:
   - If mock mode: call mock_handler
   - If has_llm: call LLM with prompt from registry
   - Otherwise: passthrough existing output
4. Execute tools (if has_tools and tool_calls in output)
5. Run post-process hook (if defined)
6. Store output in envelope
7. Determine next stage via routing rules
8. Emit agent completed event
9. Return envelope

**Example**:
```python
from protocols import Agent, AgentConfig

agent = Agent(
    config=AgentConfig(
        name="planner",
        has_llm=True,
        prompt_key="code_analysis.planner",
        output_key="plan",
        default_next="executor",
    ),
    logger=my_logger,
    llm=my_llm_provider,
    prompt_registry=my_prompts,
)

result_envelope = await agent.process(envelope)
```

---

### Runtime

Pipeline runtime - orchestrates agent execution. Uses Go for envelope state/bounds when available. Uses Python for agent execution, LLM, tools.

```python
@dataclass
class Runtime:
    config: PipelineConfig                     # Pipeline configuration
    llm_factory: Optional[LLMProviderFactory] = None
    tool_executor: Optional[ToolExecutor] = None
    logger: Optional[Logger] = None
    persistence: Optional[Persistence] = None
    prompt_registry: Optional[PromptRegistry] = None
    use_mock: bool = False
    
    agents: Dict[str, Agent] = field(default_factory=dict)
    event_context: Optional[EventContext] = None
```

**Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `set_event_context` | `(ctx: EventContext) -> None` | Set event context for all agents |
| `run` | `(envelope: Envelope, thread_id: str) -> Envelope` | Execute pipeline on envelope |
| `run_streaming` | `(envelope, thread_id) -> AsyncIterator[Tuple[str, Dict]]` | Execute pipeline with streaming outputs |
| `resume` | `(envelope: Envelope, thread_id: str) -> Envelope` | Resume after interrupt |
| `get_state` | `(thread_id: str) -> Optional[Dict[str, Any]]` | Get persisted state |
| `get_agent` | `(name: str) -> Optional[Agent]` | Get agent by name |

**Example**:
```python
from protocols import Runtime, PipelineConfig, create_envelope

runtime = Runtime(
    config=pipeline_config,
    llm_factory=create_llm_provider,
    tool_executor=tool_executor,
    logger=logger,
    prompt_registry=prompts,
)

envelope = create_envelope(
    raw_input="Analyze authentication",
    user_id="user-123",
    session_id="session-456",
)

result = await runtime.run(envelope, thread_id="thread-789")

# Streaming
async for stage_name, output in runtime.run_streaming(envelope, thread_id):
    print(f"Stage {stage_name}: {output}")
```

---

### OptionalCheckpoint

Checkpoint configuration for time-travel debugging.

```python
@dataclass
class OptionalCheckpoint:
    enabled: bool = False
    checkpoint_id: Optional[str] = None
    stage_order: int = 0
    metadata: Dict[str, Any] = field(default_factory=dict)
```

---

## Factory Functions

### create_pipeline_runner

Factory to create Runtime from pipeline config.

```python
def create_pipeline_runner(
    config: PipelineConfig,
    llm_provider_factory: Optional[LLMProviderFactory] = None,
    tool_executor: Optional[ToolExecutor] = None,
    logger: Optional[Logger] = None,
    persistence: Optional[Persistence] = None,
    prompt_registry: Optional[PromptRegistry] = None,
    use_mock: bool = False,
) -> Runtime
```

---

### create_envelope

Factory to create Envelope with auto-generated IDs and timestamps.

```python
def create_envelope(
    raw_input: str,
    user_id: str = "",
    session_id: str = "",
    request_id: str = "",
    metadata: Optional[Dict[str, Any]] = None,
) -> Envelope
```

**Example**:
```python
envelope = create_envelope(
    raw_input="Find all usages of UserService",
    user_id="user-123",
    session_id="session-456",
    metadata={"capability": "code_analysis"}
)
# envelope.envelope_id is auto-generated UUID
# envelope.created_at is current UTC timestamp
```

---

## Navigation

- [Back to README](README.md)
- [Previous: Configuration](config.md)
- [Next: Protocols](protocols.md)
