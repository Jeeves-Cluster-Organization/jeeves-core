# Jeeves Infrastructure Index

**Parent:** [Capability Contract](../CONTRACT.md)
**Updated:** 2025-12-16

---

## Overview

This directory contains the infrastructure layer that bridges protocols with the application layer. It handles dependency injection, service wiring, and configuration binding.

**Note:** Core utilities (logging, serialization, UUID) are in `shared/` at L0.
Avionics extends these with infrastructure-specific features (e.g., OpenTelemetry tracing).

---

## Files

| File | Purpose |
|------|---------|
| `__init__.py` | Package exports (Settings, FeatureFlags, AgentRuntime, etc.) |
| `settings.py` | Application settings (Pydantic BaseSettings) |
| `feature_flags.py` | Feature flag management |
| `thresholds.py` | Runtime threshold constants |
| `context.py` | AppContext DI container |
| `context_bounds.py` | ContextBounds adapter |
| `wiring.py` | ToolExecutor, create_llm_provider_factory (lazy import) |
| `runtime.py` | AgentRuntime, TimingContext |
| `capability_registry.py` | DomainLLMRegistry |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `database/` | PostgreSQL/Redis clients, repositories, connection management |
| `llm/` | LLM providers (openai, anthropic, azure, llamaserver, mock) |
| `gateway/` | FastAPI gateway, SSE, WebSocket, event bus |
| `distributed/` | RedisDistributedBus for horizontal scaling |
| `checkpoint/` | PostgresCheckpointAdapter for time-travel debugging |
| `interop/` | Go bridge (subprocess wrapper) |
| `logging/` | Logging adapters and context |
| `observability/` | OpenTelemetry tracing middleware |
| `middleware/` | Rate limiting middleware |
| `tools/` | Tool executor core |
| `utils/` | Error utilities |
| `webhooks/` | Webhook service |
| `tests/` | Unit and integration tests |

---

## Key Components

### Settings (`settings.py`)
Pydantic settings model for application configuration:
```python
class Settings(BaseSettings):
    # PostgreSQL (the ONLY supported backend)
    postgres_host: str = "localhost"
    postgres_port: int = 5432
    postgres_database: str = "assistant"
    postgres_user: str = "assistant"
    postgres_password: str = ""

    # LLM Provider (llamaserver, llamacpp, openai, anthropic, azure, mock)
    llm_provider: str = "llamaserver"
    default_model: str = "qwen2.5-3b-instruct-q4_k_m"
    llamaserver_host: str = "http://localhost:8080"

    # Per-agent overrides
    planner_model: Optional[str] = None
    validator_model: Optional[str] = None
    critic_model: Optional[str] = None

    # Helper methods
    def get_postgres_url() -> str
    def get_database_url() -> str
    def get_planner_model() -> str
    def get_feature_flags() -> FeatureFlags

# Singleton access
settings = Settings()
get_settings() -> Settings
reload_settings() -> Settings
```

### Feature Flags (`feature_flags.py`)
Dynamic feature enablement via environment variables:
```python
class FeatureFlags(BaseSettings):
    # Phase 1: LLM Gateway + Redis
    use_llm_gateway: bool = False
    use_redis_state: bool = False

    # Phase 2: Graph Workflow
    use_graph_engine: bool = False
    enable_checkpoints: bool = False

    # Phase 3: Distributed Mode
    enable_distributed_mode: bool = False
    enable_node_discovery: bool = False

    # Phase 4: Observability
    enable_tracing: bool = False
    enable_metrics_export: bool = False

    # V2 Memory Infrastructure
    memory_event_sourcing_mode: Literal["disabled", "log_only", "log_and_project"]
    memory_agent_tracing: bool = True
    memory_semantic_mode: Literal["disabled", "log_only", "log_and_use"]
    memory_graph_mode: Literal["disabled", "enabled"]

    # Methods
    def print_status() -> None
    def validate_dependencies() -> list[str]

# Also includes CyclicConfig and ContextBoundsConfig dataclasses
get_feature_flags() -> FeatureFlags
```

### Wiring (`wiring.py`)
Factory functions connecting core protocols to concrete implementations:
```python
class ToolExecutor:
    """Implements ToolExecutorProtocol."""
    async def execute(tool_name: str, params: Dict) -> Dict
    async def execute_resilient(tool_name: str, params: Dict) -> Dict
    def has_tool(name: str) -> bool
    def get_resilient_mapping(tool_name: str) -> Optional[str]

# Factory functions
def create_tool_executor(registry=None) -> ToolExecutorProtocol
def create_llm_provider_factory(settings=None) -> Callable[[str], LLMProviderProtocol]
def create_capability_service(...) -> FlowService  # Capability-specific
def get_database_client(settings=None) -> DatabaseClient
def get_tool_registry() -> ToolRegistry
```

### Thresholds (`thresholds.py`)
Centralized confidence and operational threshold constants:
```python
# Confirmation System
CONFIRMATION_DETECTION_CONFIDENCE = 0.7
CONFIRMATION_INTERPRETATION_CONFIDENCE = 0.7
CONFIRMATION_TIMEOUT_SECONDS = 300

# Planning & Execution
PLAN_MIN_CONFIDENCE = 0.70
PLAN_HIGH_CONFIDENCE = 0.85
MAX_RETRY_ATTEMPTS = 3

# Critic & Validation
CRITIC_APPROVAL_THRESHOLD = 0.80
CRITIC_HIGH_CONFIDENCE = 0.85
CRITIC_MEDIUM_CONFIDENCE = 0.75
META_VALIDATOR_PASS_THRESHOLD = 0.9
USER_CONFIRMED_CONFIDENCE = 0.9

# Search & Matching
FUZZY_MATCH_MIN_SCORE = 0.5
SEMANTIC_SEARCH_MIN_SIMILARITY = 0.5
HYBRID_SEARCH_FUZZY_WEIGHT = 0.4
HYBRID_SEARCH_SEMANTIC_WEIGHT = 0.6

# Tool Health (L7 Governance)
TOOL_DEGRADED_ERROR_RATE = 0.15
TOOL_QUARANTINE_ERROR_RATE = 0.35
TOOL_MIN_INVOCATIONS_FOR_STATS = 20

# Working Memory (L4)
SESSION_SUMMARIZATION_TURN_THRESHOLD = 8
SESSION_TOKEN_BUDGET_THRESHOLD = 6000

# Latency Budgets (per agent stage)
PERCEPTION_LATENCY_BUDGET_MS = 5000
INTENT_LATENCY_BUDGET_MS = 30000
PLANNER_LATENCY_BUDGET_MS = 60000
EXECUTOR_LATENCY_BUDGET_MS = 60000
CRITIC_LATENCY_BUDGET_MS = 30000
INTEGRATION_LATENCY_BUDGET_MS = 10000
MAX_REQUEST_LATENCY_MS = 300000  # 5 minutes total
AGENT_LATENCY_BUDGETS = {...}  # Dict mapping all agents
```

### Context Bounds (`context_bounds.py`)
Infrastructure adapter for context bounds (Amendment XXII):
```python
@dataclass
class ContextBounds:
    """Token and iteration limits for agent execution."""
    max_input_tokens: int = 4096
    max_output_tokens: int = 2048
    max_iterations: int = 3
    max_context_tokens: int = 8192
    max_llm_calls: int = 10
    max_agent_hops: int = 21

def get_context_bounds() -> ContextBounds
    """Get singleton instance."""

def configure_context_bounds(**overrides) -> ContextBounds
    """Configure with custom values."""

def reset_context_bounds() -> None
    """Reset for testing."""
```

---

## Usage

```python
from avionics import Settings, get_settings
from avionics.wiring import create_llm_provider_factory, get_database_client

# Access settings
settings = get_settings()

# Create LLM provider factory
llm_factory = create_llm_provider_factory(settings)
llm = llm_factory("planner")

# Get database client
db = get_database_client(settings)

# Or with custom configuration
from avionics import get_feature_flags, get_context_bounds

flags = get_feature_flags()
bounds = get_context_bounds()
```

---

## Related

- [coreengine/](../coreengine/) - Core engine (Go implementation)
- [protocols/](../protocols/) - Python protocols package
- [shared/](../shared/) - Shared utilities (logging, serialization, UUID)
- [control_tower/](../control_tower/) - Control Tower (kernel layer)
- [bootstrap.py](../mission_system/bootstrap.py) - Bootstrap using infrastructure
