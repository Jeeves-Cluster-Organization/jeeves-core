# mission_system.api - REST API Layer

**Package:** `mission_system.api`  
**Purpose:** HTTP REST API endpoints for the Mission System  
**Updated:** 2026-01-23

---

## Overview

The `api` package provides FastAPI-based HTTP endpoints for:
- Chat operations (session and message management)
- Health checks (liveness/readiness probes)
- Governance dashboard (system monitoring)
- Request processing via Control Tower

---

## Module Index

| Module | Description |
|--------|-------------|
| `__init__.py` | `MissionRuntime`, `create_mission_runtime()` - Framework entry point |
| `chat.py` | Chat API router for conversational interface |
| `health.py` | Governance API for system introspection (L7) |
| `health.py` | Health check endpoints (Kubernetes-compatible) |
| `server.py` | FastAPI application and lifespan management |

---

## Key Classes and Functions

### `MissionRuntime` (from `__init__.py`)

Framework primitives bundle for capabilities.

```python
@dataclass
class MissionRuntime:
    """
    Mission System Runtime - Framework primitives for capabilities.
    
    Attributes:
        persistence: PersistenceProtocol - Database client
        tool_executor: ToolExecutorProtocol - Tool execution service
        settings: Settings - Application settings
        config_registry: ConfigRegistryProtocol - Config injection
        llm_provider_factory: Optional[Callable] - LLM factory (None if mock)
        mock_provider: Optional[LLMProviderProtocol] - Mock provider
    """
```

### `create_mission_runtime()`

Factory function to create a Mission Runtime.

```python
def create_mission_runtime(
    tool_registry: ToolRegistryProtocol,
    persistence: Optional[PersistenceProtocol] = None,
    settings: Optional[Settings] = None,
    use_mock: bool = False,
    config_registry: Optional[ConfigRegistryProtocol] = None,
) -> MissionRuntime:
    """Create a Mission System runtime for capabilities."""
```

**Usage:**
```python
from jeeves_infra.gateway import create_mission_runtime

runtime = create_mission_runtime(
    tool_registry=my_tool_registry,
    persistence=my_db,
)

# Register capability configs
runtime.config_registry.register("language_config", LanguageConfig())

# Build capability service
my_service = MyCapabilityService(
    tool_executor=runtime.tool_executor,
    llm_factory=runtime.llm_provider_factory,
)
```

---

## Chat API (`chat.py`)

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/chat/sessions` | List user sessions |
| `POST` | `/api/v1/chat/sessions` | Create new session |
| `GET` | `/api/v1/chat/sessions/{session_id}` | Get session details |
| `PATCH` | `/api/v1/chat/sessions/{session_id}` | Update session (rename, archive) |
| `DELETE` | `/api/v1/chat/sessions/{session_id}` | Delete session |
| `GET` | `/api/v1/chat/sessions/{session_id}/messages` | List messages |
| `POST` | `/api/v1/chat/messages` | Send message (triggers orchestrator) |
| `DELETE` | `/api/v1/chat/messages/{message_id}` | Delete message |
| `PATCH` | `/api/v1/chat/messages/{message_id}` | Edit message |
| `GET` | `/api/v1/chat/search` | Full-text search |
| `GET` | `/api/v1/chat/sessions/{session_id}/export` | Export session |

### Request/Response Models

```python
class SessionCreate(BaseModel):
    user_id: str
    title: Optional[str] = None

class SessionResponse(BaseModel):
    session_id: str
    user_id: str
    title: Optional[str]
    message_count: int
    created_at: str
    last_activity: str

class MessageSend(BaseModel):
    message: str
    session_id: Optional[str] = None

class MessageSendResponse(BaseModel):
    request_id: str
    session_id: str
    status: str  # completed, processing, failed
    response: Optional[str]
    confirmation_needed: bool
    confirmation_message: Optional[str]
    confirmation_id: Optional[str]
```

---

## Governance API (`health.py`)

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/v1/governance/health` | System health summary |
| `GET` | `/api/v1/governance/tools/{tool_name}` | Individual tool health |
| `GET` | `/api/v1/governance/dashboard` | Web UI dashboard |

### Response Models

```python
class HealthSummaryResponse(BaseModel):
    overall_status: str  # healthy, degraded, unhealthy, unknown
    summary: Dict[str, int]  # healthy, degraded, unhealthy, unknown counts
    tools_needing_attention: list
    slow_executions: list
    checked_at: str

class ToolHealthResponse(BaseModel):
    tool_name: str
    status: str
    success_rate: float
    avg_latency_ms: float
    total_calls: int
    recent_errors: int
    issues: list
    recommendations: list
```

### Dashboard Features (v0.13)

- Memory layer status (L1-L7)
- Circuit breaker status
- Tool health cards with latency metrics
- Constitutional alignment indicators

---

## Health API (`health.py`)

Kubernetes-compatible health probes.

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Liveness probe (is service running?) |
| `GET` | `/ready` | Readiness probe (can accept traffic?) |

### Classes

```python
class ComponentStatus(str, Enum):
    UP = "up"
    DOWN = "down"
    DEGRADED = "degraded"

class HealthChecker:
    """Performs health checks on system components."""
    
    async def check_liveness(self) -> HealthCheckResult
    async def check_readiness(self) -> HealthCheckResult
```

### Readiness Checks

1. **Database** - Connection and schema verification
2. **LLM Models** - Provider availability (OpenAI, Anthropic, Azure, llama.cpp)

---

## Server (`server.py`)

Main FastAPI application setup.

### Key Components

```python
class AppState:
    """Application-level state for dependency injection."""
    db: Optional[DatabaseClientProtocol]
    tool_catalog: Any
    control_tower: Optional[ControlTower]
    event_bridge: Optional[EventBridge]
    orchestrator: Optional[Any]
    health_checker: Optional[HealthChecker]
    event_manager: Optional[WebSocketEventManager]
    chat_service: Optional[ChatService]
    tool_health_service: Any
```

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Root - API information |
| `POST` | `/api/v1/requests` | Submit analysis request |
| `POST` | `/api/v1/confirmations` | Submit confirmation response |
| `POST` | `/api/v1/chat/clarifications` | Submit clarification response |
| `GET` | `/api/v1/requests/{pid}/status` | Get request status |
| `GET` | `/api/v1/metrics` | Get system metrics |
| `WebSocket` | `/ws` | Real-time event streaming |

### Lifespan Management

```python
@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """
    Lifecycle manager for FastAPI application.
    
    Initializes:
    - Database connection
    - Control Tower
    - Event Bridge
    - Orchestrator (from capability registry)
    - Health checker
    - Tool health service
    """
```

---

## Control Tower Integration

All requests flow through Control Tower for:
- Lifecycle management (ProcessControlBlock)
- Resource tracking (quotas, usage)
- Event aggregation (WebSocket streaming)

```python
# Submit request via Control Tower
result_envelope = await app_state.control_tower.submit_request(
    envelope=envelope,
    priority=SchedulingPriority.NORMAL,
)
```

---

## Navigation

- [← Back to README](README.md)
- [→ Orchestrator Documentation](orchestrator.md)
- [→ Services Documentation](services.md)
