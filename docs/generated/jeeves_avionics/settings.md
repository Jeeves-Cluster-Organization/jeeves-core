# Settings & Configuration

Configuration modules for the Jeeves infrastructure layer.

## Navigation

- [README](./README.md) - Overview
- **Settings** (this file)
- [Gateway](./gateway.md)
- [Database](./database.md)
- [LLM](./llm.md)
- [Observability](./observability.md)
- [Tools](./tools.md)
- [Infrastructure](./infrastructure.md)

---

## settings.py

Infrastructure settings using pydantic-settings. Environment variables are loaded automatically.

### Class: Settings

```python
class Settings(BaseSettings):
    """Infrastructure settings.
    
    Constitutional Reference:
        - Avionics R3: No Domain Logic
        - Capability Constitution R6: Domain Config Ownership
    """
```

#### Deployment Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `deployment_mode` | str | `"single_node"` | `single_node` or `distributed` |

#### LLM Provider Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `llm_provider` | str | `"llamaserver"` | Default provider |
| `llamaserver_host` | str | `"http://localhost:8080"` | llama-server URL |
| `llamaserver_api_type` | str | `"native"` | `"native"` or `"openai"` |
| `default_model` | str | `"qwen2.5-3b-instruct-q4_k_m"` | Default model |
| `default_temperature` | float | `0.3` | Default temperature |
| `disable_temperature` | bool | `False` | Disable temperature |
| `openai_api_key` | str | `None` | OpenAI API key |
| `anthropic_api_key` | str | `None` | Anthropic API key |
| `azure_endpoint` | str | `None` | Azure endpoint URL |
| `azure_api_key` | str | `None` | Azure API key |
| `azure_deployment_name` | str | `None` | Azure deployment name |
| `azure_api_version` | str | `"2024-02-01"` | Azure API version |

#### Timeouts and Retries

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `llm_timeout` | int | `300` | LLM request timeout (seconds) |
| `llm_max_retries` | int | `3` | Max retry attempts |
| `executor_timeout` | int | `60` | Tool execution timeout |

#### llama.cpp In-Process Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `llamacpp_model_path` | str | `"./models/llama-3.1-8b-q4_0.gguf"` | Path to GGUF model file |
| `llamacpp_n_ctx` | int | `4096` | Context window size |
| `llamacpp_n_gpu_layers` | int | `0` | Number of GPU layers |
| `llamacpp_n_threads` | int | `None` | Thread count (auto if None) |

#### Memory Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `memory_enabled` | bool | `True` | Enable memory features |
| `memory_intent_classification` | bool | `True` | Enable intent classification |
| `memory_auto_crossref` | bool | `True` | Enable automatic cross-referencing |

#### Vector DB Configuration (pgvector)

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `vector_db_enabled` | bool | `True` | Enable vector database |
| `embedding_model` | str | `"all-MiniLM-L6-v2"` | Sentence-transformers model |
| `embedding_batch_size` | int | `32` | Embedding batch size |
| `embedding_cache_size` | int | `1000` | Embedding cache size |

#### Search Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `search_default_limit` | int | `10` | Default search result limit |
| `search_semantic_weight` | float | `0.6` | Semantic search weight |
| `search_min_similarity` | float | `0.5` | Minimum similarity threshold |

#### Cross-Reference Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `crossref_auto_extract` | bool | `True` | Auto-extract cross-references |
| `crossref_min_confidence` | float | `0.7` | Minimum confidence threshold |

#### API Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `api_host` | str | `"0.0.0.0"` | API bind host |
| `api_port` | int | `8000` | API port |
| `api_reload` | bool | `False` | Enable auto-reload |

#### Rate Limiting

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `requests_per_minute` | int | `60` | Requests per minute limit |
| `rate_limit_interval_seconds` | float | `60.0` | Rate limit window |

#### Logging

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `log_level` | str | `"INFO"` | Log level |

#### Confirmation Feature

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `enable_confirmations` | bool | `False` | Enable confirmation prompts |
| `confirmation_timeout_seconds` | int | `300` | Confirmation timeout |
| `confirmation_required_operations` | List[str] | `None` | Operations requiring confirmation |
| `skip_confirmation_for_single_item` | bool | `False` | Skip for single items |

#### Chat UI Integration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `chat_enabled` | bool | `True` | Enable chat UI |

#### PostgreSQL Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `postgres_host` | str | `"localhost"` | PostgreSQL host |
| `postgres_port` | int | `5432` | PostgreSQL port |
| `postgres_database` | str | `"assistant"` | Database name |
| `postgres_user` | str | `"assistant"` | Username |
| `postgres_password` | str | `""` | Password |
| `postgres_pool_size` | int | `20` | Connection pool size |
| `postgres_max_overflow` | int | `10` | Max overflow connections |
| `postgres_pool_timeout` | int | `30` | Pool connection timeout |
| `postgres_pool_recycle` | int | `3600` | Connection recycle interval |
| `pgvector_dimension` | int | `384` | Vector dimension |

#### Redis Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `redis_url` | str | `"redis://localhost:6379"` | Redis URL |
| `redis_pool_size` | int | `10` | Connection pool size |

#### Checkpoint Configuration (Amendment XXIII)

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `checkpoint_enabled` | bool | `True` | Enable checkpointing |
| `checkpoint_retention_days` | int | `7` | Days to retain checkpoints |
| `checkpoint_max_per_envelope` | int | `100` | Max checkpoints per envelope |

#### Distributed Configuration (Amendment XXIV)

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `distributed_enabled` | bool | `False` | Enable distributed mode |
| `worker_queues` | List[str] | `None` | Worker queue patterns |
| `worker_max_concurrent` | int | `5` | Max concurrent tasks |
| `worker_heartbeat_seconds` | int | `30` | Worker heartbeat interval |
| `worker_task_timeout` | int | `300` | Task execution timeout |

#### MCP/A2A Interoperability (Amendment XXV)

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `mcp_enabled` | bool | `False` | Enable MCP server |
| `mcp_port` | int | `8081` | MCP server port |
| `a2a_enabled` | bool | `False` | Enable Agent-to-Agent protocol |
| `a2a_registry_url` | str | `None` | A2A registry URL |
| `a2a_agent_endpoint` | str | `None` | A2A agent endpoint URL |

#### WebSocket Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `websocket_auth_token` | str | `"local-dev-token"` | Auth token |
| `websocket_auth_required` | bool | `False` | Require auth |
| `websocket_heartbeat_interval` | float | `30.0` | Heartbeat interval |
| `websocket_idle_timeout` | float | `120.0` | Idle timeout |

#### Helper Methods

```python
def get_postgres_url(self) -> str:
    """Build PostgreSQL connection URL for asyncpg."""

def get_feature_flags(self) -> FeatureFlags:
    """Get feature flags instance."""

def log_llm_config(self, logger: LoggerProtocol) -> None:
    """Log current LLM configuration."""
```

#### Global Accessors

```python
def get_settings() -> Settings:
    """Get global settings instance (lazy initialization)."""

def set_settings(settings_instance: Settings) -> None:
    """Set the global settings instance (for testing)."""

def reset_settings() -> None:
    """Reset the global settings instance."""

def reload_settings() -> Settings:
    """Reload settings from environment."""
```

---

## feature_flags.py

Runtime toggles for phased feature rollout.

### Class: FeatureFlags

```python
class FeatureFlags(BaseSettings):
    """Runtime toggles for new features.
    
    All flags default to False for safety.
    Enable via environment: FEATURE_USE_LLM_GATEWAY=true
    """
    model_config = SettingsConfigDict(env_prefix="FEATURE_")
```

#### Phase 1: LLM Gateway + Redis

| Flag | Default | Description |
|------|---------|-------------|
| `use_llm_gateway` | `False` | Route LLM calls through unified gateway |
| `use_redis_state` | `False` | Use Redis for distributed state |

#### Phase 2: Graph Workflow

| Flag | Default | Description |
|------|---------|-------------|
| `use_graph_engine` | `False` | Graph-based workflow execution |
| `enable_checkpoints` | `False` | Workflow state persistence |

#### Phase 3: Distributed Mode

| Flag | Default | Description |
|------|---------|-------------|
| `enable_distributed_mode` | `False` | Multi-node deployment |
| `enable_node_discovery` | `False` | Automatic GPU node discovery |

#### Phase 4: Observability

| Flag | Default | Description |
|------|---------|-------------|
| `enable_tracing` | `False` | OpenTelemetry tracing |
| `enable_metrics_export` | `False` | Prometheus metrics |
| `enable_debug_logging` | `False` | Verbose debug logging |

#### Phase 2.3: Agent Reasoning Observability

| Flag | Default | Description |
|------|---------|-------------|
| `emit_agent_reasoning` | `False` | Emit agent reasoning/CoT events |

#### V2 Memory Infrastructure

| Flag | Default | Description |
|------|---------|-------------|
| `memory_event_sourcing_mode` | `"log_and_project"` | Event sourcing mode |
| `memory_agent_tracing` | `True` | Agent decision tracing |
| `memory_trace_retention_days` | `30` | Days to retain agent traces |
| `memory_semantic_mode` | `"log_and_use"` | Semantic search mode |
| `memory_embedding_model` | `"all-MiniLM-L6-v2"` | Sentence-transformers model |
| `memory_working_memory` | `True` | Session summarization |
| `memory_summarization_threshold_turns` | `8` | Turns before summarization |
| `memory_summarization_threshold_tokens` | `6000` | Tokens before summarization |
| `memory_graph_mode` | `"enabled"` | Graph relationship tracking |
| `memory_auto_edge_extraction` | `True` | Auto-extract relationships |
| `memory_governance_mode` | `"log_only"` | Tool governance mode |
| `memory_tool_quarantine` | `False` | Auto-quarantine on high errors |
| `memory_prompt_versioning` | `True` | Track prompt versions |

#### Methods

```python
def log_status(self, logger: LoggerProtocol) -> None:
    """Log current feature flag status."""

def validate_dependencies(self) -> list[str]:
    """Validate that dependent features are enabled."""
```

#### Global Accessors

```python
def get_feature_flags() -> FeatureFlags:
    """Get global feature flags instance (lazy initialization)."""

def set_feature_flags(flags: FeatureFlags) -> None:
    """Set the global feature flags instance (for testing)."""

def reset_feature_flags() -> None:
    """Reset the global feature flags instance."""
```

### Class: CyclicConfig

```python
@dataclass
class CyclicConfig:
    """Configuration for the cyclic orchestrator."""
    max_llm_calls: int = 10
    max_iterations: int = 3
    critic_confidence_threshold: float = 0.6
    max_validator_retries: int = 1
    max_planner_retries: int = 1
```

### Class: ContextBoundsConfig

```python
@dataclass
class ContextBoundsConfig:
    """Configuration for context window bounds."""
    max_task_context_chars: int = 2000
    max_semantic_snippets: int = 5
    max_conversation_turns: int = 10
    max_total_context_chars: int = 10000
```

---

## capability_registry.py

Registry for capability-owned agent LLM configurations.

### Class: DomainLLMRegistry

```python
class DomainLLMRegistry:
    """Registry for capability-owned agent LLM configurations.
    
    Capabilities register their agent configs at startup.
    Infrastructure queries the registry instead of hardcoded agent names.
    """
```

#### Methods

```python
def register(
    self,
    capability_id: str,
    agent_name: str,
    config: AgentLLMConfig
) -> None:
    """Register an agent's LLM configuration."""

def get_agent_config(self, agent_name: str) -> Optional[AgentLLMConfig]:
    """Get configuration for an agent."""

def list_agents(self) -> List[str]:
    """List all registered agent names."""

def get_capability_agents(self, capability_id: str) -> List[AgentLLMConfig]:
    """Get all agent configurations for a capability."""

def clear(self) -> None:
    """Clear all registered configurations (testing)."""
```

#### Environment Variable Overrides

Environment variables override registered config values. The agent name is uppercased for the prefix:

```
{AGENT_NAME}_MODEL            -> config.model
{AGENT_NAME}_TEMPERATURE      -> config.temperature
LLAMASERVER_{AGENT_NAME}_URL  -> config.server_url
{AGENT_NAME}_LLM_PROVIDER     -> config.provider
```

Example: For agent `planner`, use `PLANNER_MODEL`, `PLANNER_TEMPERATURE`, etc.

#### Usage Example

```python
from avionics.capability_registry import get_capability_registry
from protocols import AgentLLMConfig

registry = get_capability_registry()
registry.register("my_capability", "my_agent", AgentLLMConfig(
    agent_name="my_agent",
    model="qwen2.5-7b-instruct-q4_k_m",
    temperature=0.3,
))

# In infrastructure (factory.py)
config = registry.get_agent_config("my_agent")
```

---

## wiring.py

Tool execution and LLM provider factories for dependency injection.

### Class: AgentContext

```python
@dataclass
class AgentContext:
    """Context for tool execution identity."""
    agent_name: str
    request_id: str
    envelope_id: str
```

### Class: ToolExecutor

```python
class ToolExecutor(ToolExecutorProtocol):
    """Tool executor with parameter validation and access control.
    
    - Validates parameters against tool schema
    - Filters None values to allow function defaults
    - Tracks execution time
    - Enforces access control via AgentToolAccessProtocol
    """
    
    def __init__(
        self,
        tool_registry: ToolRegistryProtocol,
        access_policy: Optional[AgentToolAccessProtocol] = None,
        logger: Optional[LoggerProtocol] = None,
    ):
        ...
    
    async def execute(
        self,
        tool_name: str,
        context: AgentContext,
        **params: Any,
    ) -> Dict[str, Any]:
        """Execute a tool with validation and access control."""
```

### Resilient Operations Mapping

```python
RESILIENT_OPS_MAP: Dict[str, str]  # Maps tool_id to resilient operation name
RESILIENT_PARAM_MAP: Dict[str, Dict[str, str]]  # Maps tool_id to parameter transformations
```

### Factory Functions

```python
def create_tool_executor(
    tool_registry: ToolRegistryProtocol,
    access_policy: Optional[AgentToolAccessProtocol] = None,
    logger: Optional[LoggerProtocol] = None,
) -> ToolExecutor:
    """Create a configured ToolExecutor instance."""

def create_llm_provider_factory(
    settings: Settings,
    logger: Optional[LoggerProtocol] = None,
) -> Callable[[str], LLMProviderProtocol]:
    """Create an LLM provider factory function.
    
    Returns a callable that creates providers by agent name.
    """
```

---

## thresholds.py

Centralized confidence and operational thresholds.

### Confirmation System

| Constant | Value | Description |
|----------|-------|-------------|
| `CONFIRMATION_DETECTION_CONFIDENCE` | `0.7` | Minimum for confirmation routing |
| `CONFIRMATION_INTERPRETATION_CONFIDENCE` | `0.7` | Minimum for interpretation |
| `CONFIRMATION_TIMEOUT_SECONDS` | `300` | Default timeout |

### Planning & Execution

| Constant | Value | Description |
|----------|-------|-------------|
| `PLAN_MIN_CONFIDENCE` | `0.70` | Below this, request clarification |
| `PLAN_HIGH_CONFIDENCE` | `0.85` | Skip optional validation |
| `MAX_RETRY_ATTEMPTS` | `3` | Transient failure retries |

### Critic & Validation

| Constant | Value | Description |
|----------|-------|-------------|
| `CRITIC_APPROVAL_THRESHOLD` | `0.80` | Approve without changes |
| `CRITIC_HIGH_CONFIDENCE` | `0.85` | High confidence retry |
| `CRITIC_MEDIUM_CONFIDENCE` | `0.75` | Medium confidence retry |
| `CRITIC_LOW_CONFIDENCE` | `0.6` | Clarification decisions |
| `CRITIC_DEFAULT_CONFIDENCE` | `0.5` | Error fallback |
| `META_VALIDATOR_PASS_THRESHOLD` | `0.9` | Pass threshold |
| `USER_CONFIRMED_CONFIDENCE` | `0.9` | User-confirmed action |

### Search & Matching

| Constant | Value | Description |
|----------|-------|-------------|
| `FUZZY_MATCH_MIN_SCORE` | `0.5` | Minimum fuzzy score |
| `SEMANTIC_SEARCH_MIN_SIMILARITY` | `0.5` | Minimum semantic score |
| `HYBRID_SEARCH_FUZZY_WEIGHT` | `0.4` | Fuzzy weight in hybrid |
| `HYBRID_SEARCH_SEMANTIC_WEIGHT` | `0.6` | Semantic weight |

### Tool Health (L7 Governance)

| Constant | Value | Description |
|----------|-------|-------------|
| `TOOL_DEGRADED_ERROR_RATE` | `0.15` | 15% triggers degraded |
| `TOOL_QUARANTINE_ERROR_RATE` | `0.35` | 35% triggers quarantine |
| `TOOL_MIN_INVOCATIONS_FOR_STATS` | `20` | Minimum sample size |
| `TOOL_QUARANTINE_DURATION_HOURS` | `24` | Quarantine duration |

### Working Memory (L4)

| Constant | Value | Description |
|----------|-------|-------------|
| `SESSION_SUMMARIZATION_TURN_THRESHOLD` | `8` | Turns before summarization |
| `SESSION_TOKEN_BUDGET_THRESHOLD` | `6000` | Token budget trigger |
| `SESSION_IDLE_TIMEOUT_MINUTES` | `30` | Idle timeout |
| `OPEN_LOOP_STALE_DAYS` | `7` | Open loop staleness |

### Latency Budget

| Constant | Value | Description |
|----------|-------|-------------|
| `MAX_REQUEST_LATENCY_MS` | `300000` | 5 minute total budget |
