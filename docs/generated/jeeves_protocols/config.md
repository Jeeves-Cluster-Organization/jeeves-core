# protocols.config

**Layer**: L0 (Foundation)  
**Purpose**: Configuration types - mirrors Go coreengine/config

## Overview

Pure data types for configuration. No global state. Configuration is injected via AppContext.

## Enums

### RunMode

How the pipeline runs stages.

```python
class RunMode(str, Enum):
    SEQUENTIAL = "sequential"  # One stage at a time
    PARALLEL = "parallel"      # Independent stages run concurrently
```

---

### JoinStrategy

How to handle multiple prerequisites in parallel execution.

```python
class JoinStrategy(str, Enum):
    ALL = "all"  # Wait for ALL prerequisites (default)
    ANY = "any"  # Proceed when ANY prerequisite completes
```

---

## Dataclasses

### RoutingRule

Routing rule for conditional stage transitions.

```python
@dataclass
class RoutingRule:
    condition: str    # Field to check in output
    value: Any        # Expected value
    target: str       # Target stage if condition matches
```

**Example**:
```python
rule = RoutingRule(
    condition="needs_clarification",
    value=True,
    target="clarification"
)
```

---

### EdgeLimit

Per-edge transition limit for cyclic routing control.

```python
@dataclass
class EdgeLimit:
    from_stage: str   # Source stage
    to_stage: str     # Target stage
    max_count: int    # Maximum transitions allowed
```

**Example**:
```python
# Limit execution -> critic loops to prevent infinite retries
limit = EdgeLimit(
    from_stage="critic",
    to_stage="execution",
    max_count=3
)
```

---

### AgentConfig

Declarative agent configuration - mirrors Go `coreengine/config.AgentConfig`.

```python
@dataclass
class AgentConfig:
    name: str                                    # Agent name
    stage_order: int = 0                         # Order in pipeline
    
    # Dependencies (for parallel execution)
    requires: List[str] = field(default_factory=list)  # Hard dependencies
    after: List[str] = field(default_factory=list)     # Soft ordering
    join_strategy: JoinStrategy = JoinStrategy.ALL
    
    # Capabilities
    has_llm: bool = False                        # Uses LLM
    has_tools: bool = False                      # Uses tools
    has_policies: bool = False                   # Has policy checks
    
    # Tool access
    tool_access: ToolAccess = ToolAccess.NONE
    allowed_tools: Optional[Set[str]] = None     # Whitelist of allowed tools
    
    # LLM config
    model_role: Optional[str] = None             # Role for LLM routing
    prompt_key: Optional[str] = None             # Key for prompt lookup
    temperature: Optional[float] = None
    max_tokens: Optional[int] = None
    
    # Output
    output_key: str = ""                         # Key for storing output
    required_output_fields: List[str] = field(default_factory=list)
    
    # Routing
    routing_rules: List[RoutingRule] = field(default_factory=list)
    default_next: Optional[str] = None           # Default next stage
    error_next: Optional[str] = None             # Stage on error
    
    # Bounds
    timeout_seconds: Optional[int] = None
    max_retries: int = 0
    
    # Processing hooks (Python-only)
    pre_process: Optional[Callable] = None
    post_process: Optional[Callable] = None
    mock_handler: Optional[Callable] = None
```

**Example**:
```python
from protocols import AgentConfig, ToolAccess, RoutingRule

planner_config = AgentConfig(
    name="planner",
    stage_order=2,
    has_llm=True,
    model_role="planner",
    prompt_key="code_analysis.planner",
    output_key="plan",
    required_output_fields=["steps", "search_targets"],
    default_next="executor",
    timeout_seconds=120,
)

executor_config = AgentConfig(
    name="executor",
    stage_order=3,
    requires=["planner"],  # Must wait for planner
    has_llm=True,
    has_tools=True,
    tool_access=ToolAccess.ALL,
    output_key="execution",
    routing_rules=[
        RoutingRule(condition="complete", value=True, target="synthesizer"),
        RoutingRule(condition="needs_more", value=True, target="planner"),
    ],
    default_next="synthesizer",
)
```

---

### PipelineConfig

Pipeline configuration - mirrors Go `coreengine/config.PipelineConfig`.

```python
@dataclass
class PipelineConfig:
    name: str                                    # Pipeline name
    agents: List[AgentConfig] = field(default_factory=list)
    
    # Run mode
    default_run_mode: RunMode = RunMode.SEQUENTIAL
    
    # Bounds (Go enforces these authoritatively)
    max_iterations: int = 3
    max_llm_calls: int = 10
    max_agent_hops: int = 21
    default_timeout_seconds: int = 300
    
    # Edge limits for cyclic routing control
    edge_limits: List[EdgeLimit] = field(default_factory=list)
    
    # Interrupt resume stages (capability-defined)
    clarification_resume_stage: Optional[str] = None
    confirmation_resume_stage: Optional[str] = None
    agent_review_resume_stage: Optional[str] = None
```

**Methods**:

| Method | Signature | Description |
|--------|-----------|-------------|
| `get_stage_order` | `() -> List[str]` | Get ordered list of agent names |
| `get_edge_limit` | `(from_stage: str, to_stage: str) -> int` | Get cycle limit for an edge |
| `get_ready_stages` | `(completed: Dict[str, bool]) -> List[str]` | Get stages ready to execute given completed stages |

**Example**:
```python
from protocols import PipelineConfig, AgentConfig, RunMode

pipeline = PipelineConfig(
    name="code_analysis",
    agents=[
        AgentConfig(name="perception", stage_order=0, ...),
        AgentConfig(name="intent", stage_order=1, ...),
        AgentConfig(name="planner", stage_order=2, ...),
        AgentConfig(name="executor", stage_order=3, ...),
        AgentConfig(name="synthesizer", stage_order=4, ...),
        AgentConfig(name="critic", stage_order=5, ...),
        AgentConfig(name="integration", stage_order=6, ...),
    ],
    default_run_mode=RunMode.SEQUENTIAL,
    max_iterations=3,
    max_llm_calls=10,
    clarification_resume_stage="perception",
)

# Get stage order
stages = pipeline.get_stage_order()  # ["perception", "intent", ...]

# Check ready stages for parallel execution
completed = {"perception": True, "intent": True}
ready = pipeline.get_ready_stages(completed)  # ["planner"]
```

---

### ContextBounds

Context window bounds for LLM interactions.

```python
@dataclass
class ContextBounds:
    max_input_tokens: int = 4096
    max_output_tokens: int = 2048
    max_context_tokens: int = 16384
    reserved_tokens: int = 512
```

---

### ExecutionConfig

Core runtime configuration.

```python
@dataclass
class ExecutionConfig:
    # Bounds
    max_iterations: int = 3
    max_llm_calls: int = 10
    max_agent_hops: int = 21
    
    # Context
    context_bounds: ContextBounds = field(default_factory=ContextBounds)
    
    # Feature flags
    enable_telemetry: bool = True
    enable_checkpoints: bool = False
    debug_mode: bool = False
```

---

### OrchestrationFlags

Runtime orchestration flags.

```python
@dataclass
class OrchestrationFlags:
    enable_parallel_agents: bool = False
    enable_checkpoints: bool = False
    enable_distributed: bool = False
    enable_telemetry: bool = True
    max_concurrent_agents: int = 4
    checkpoint_interval_seconds: int = 30
```

---

## Navigation

- [Back to README](README.md)
- [Previous: Envelope](envelope.md)
- [Next: Agents](agents.md)
