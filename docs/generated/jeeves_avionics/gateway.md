# Gateway Module

HTTP API layer using FastAPI with WebSocket, SSE, and gRPC client support.

## Navigation

- [README](./README.md) - Overview
- [Settings](./settings.md)
- **Gateway** (this file)
- [Database](./database.md)
- [LLM](./llm.md)
- [Observability](./observability.md)
- [Tools](./tools.md)
- [Infrastructure](./infrastructure.md)

---

## gateway/main.py

Main FastAPI application with lifecycle management.

### Class: GatewayConfig

```python
class GatewayConfig:
    """Gateway configuration from environment."""
    
    def __init__(self):
        self.orchestrator_host = os.getenv("ORCHESTRATOR_HOST", "localhost")
        self.orchestrator_port = int(os.getenv("ORCHESTRATOR_PORT", "50051"))
        self.api_host = os.getenv("API_HOST", "0.0.0.0")
        self.api_port = int(os.getenv("API_PORT", "8000"))
        self.cors_origins = os.getenv("CORS_ORIGINS", "*").split(",")
        self.debug = os.getenv("DEBUG", "false").lower() == "true"
```

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `ORCHESTRATOR_HOST` | `localhost` | gRPC orchestrator host |
| `ORCHESTRATOR_PORT` | `50051` | gRPC orchestrator port |
| `API_HOST` | `0.0.0.0` | API bind host |
| `API_PORT` | `8000` | API port |
| `CORS_ORIGINS` | `*` | Comma-separated CORS origins |
| `DEBUG` | `false` | Enable debug mode |

### Application Setup

```python
app = FastAPI(
    title="Jeeves Gateway",
    description="HTTP Gateway - translates REST/SSE to internal gRPC",
    version="4.0.0",
    lifespan=lifespan,
)
```

### Endpoints

| Path | Method | Description |
|------|--------|-------------|
| `/` | GET | Root with API information |
| `/health` | GET | Liveness probe |
| `/ready` | GET | Readiness probe |
| `/ws` | WebSocket | Real-time communication |
| `/chat` | GET | Chat UI (HTML) |
| `/governance` | GET | Governance UI (HTML) |

### Mounted Routers

- `/api/v1/chat` - Chat router
- `/api/v1/governance` - Governance router
- `/api/v1/interrupts` - Interrupts router

---

## gateway/websocket.py

WebSocket client management and event subscription.

### Functions

```python
def get_connected_clients() -> Set[WebSocket]:
    """Get the set of connected WebSocket clients."""

def register_client(websocket: WebSocket) -> int:
    """Register a WebSocket client. Returns client count."""

def unregister_client(websocket: WebSocket) -> int:
    """Unregister a WebSocket client. Returns client count."""

async def broadcast_to_clients(message: dict) -> int:
    """Broadcast message to all connected clients. Returns sent count."""

async def setup_websocket_subscriptions() -> None:
    """Subscribe to gateway events for WebSocket broadcasting."""
```

### Event Patterns Subscribed

- `agent.*` - Agent lifecycle events
- `perception.*`, `intent.*`, `planner.*`, `executor.*`
- `synthesizer.*`, `critic.*`, `integration.*`
- `orchestrator.*` - Flow-level events

---

## gateway/sse.py

Server-Sent Events (SSE) utilities.

### Functions

```python
def format_sse_event(
    data: Any,
    event: Optional[str] = None,
    id: Optional[str] = None,
    retry: Optional[int] = None,
) -> str:
    """Format data as an SSE event string."""

def format_sse_comment(comment: str) -> str:
    """Format a comment (for keepalive)."""

async def sse_keepalive(interval: float = 15.0, comment: str = "keepalive") -> AsyncIterator[str]:
    """Generate keepalive comments at regular intervals."""

async def merge_sse_streams(data_stream: AsyncIterator[str], keepalive_interval: float = 15.0) -> AsyncIterator[str]:
    """Merge a data stream with keepalive pings."""
```

### Class: SSEStream

```python
class SSEStream:
    """Helper class for building SSE response streams."""
    
    def event(self, data: Any, event: Optional[str] = None, include_id: bool = True) -> str:
        """Format an event with auto-incrementing ID."""
    
    def done(self, data: Optional[Any] = None) -> str:
        """Send a [DONE] marker (OpenAI-style)."""
    
    def error(self, message: str, code: Optional[str] = None) -> str:
        """Send an error event."""
    
    def keepalive(self) -> str:
        """Send a keepalive comment."""
```

---

## gateway/event_bus.py

Internal pub/sub for gateway events.

### Class: GatewayEventBus

```python
class GatewayEventBus(EventEmitterProtocol):
    """Constitutional async event bus for gateway internal communication.
    
    Features:
    - Pattern-based subscriptions (e.g., "agent.*")
    - Async handlers receiving Event
    - Fire-and-forget publishing
    """
```

#### Methods

```python
async def emit(self, event: Event) -> None:
    """Emit a unified event to all matching subscribers."""

async def subscribe(
    self,
    pattern: str,
    handler: Callable[[Event], Awaitable[None]],
) -> str:
    """Subscribe to events matching a pattern. Returns subscription ID."""

async def unsubscribe(self, subscription_id: str) -> None:
    """Unsubscribe from events."""

def clear(self) -> None:
    """Clear all subscriptions (for testing)."""
```

### Global Instance

```python
gateway_events = GatewayEventBus()
```

---

## gateway/grpc_client.py

gRPC client stubs for internal services.

### Class: GrpcClientManager

```python
class GrpcClientManager:
    """Manages gRPC channel and service stubs."""
    
    def __init__(
        self,
        orchestrator_host: str = "localhost",
        orchestrator_port: int = 50051,
        max_retries: int = 3,
        retry_delay: float = 1.0,
        logger: Optional[LoggerProtocol] = None,
    ):
        ...
```

#### Methods

```python
async def connect(self) -> None:
    """Establish gRPC channel and create stubs."""

async def disconnect(self) -> None:
    """Close gRPC channel."""

async def health_check(self) -> bool:
    """Check if orchestrator is reachable."""
```

#### Properties

- `flow` - JeevesFlowService stub
- `governance` - GovernanceService stub

### Global Accessors

```python
def get_grpc_client() -> GrpcClientManager:
    """Get the global gRPC client instance."""

def set_grpc_client(client: GrpcClientManager) -> None:
    """Set the global gRPC client instance."""
```

---

## gateway/routers/chat.py

Chat router - REST + SSE streaming endpoints.

### Request/Response Models

```python
class MessageSend(BaseModel):
    message: str
    session_id: Optional[str] = None
    mode: Optional[str] = None
    repo_path: Optional[str] = None

class MessageResponse(BaseModel):
    request_id: str
    session_id: str
    status: str  # completed, clarification, confirmation, error
    response: Optional[str] = None
    clarification_needed: bool = False
    clarification_question: Optional[str] = None
    # ... more fields
```

### Endpoints

| Path | Method | Description |
|------|--------|-------------|
| `/messages` | POST | Send message, get response |
| `/stream` | GET | SSE streaming of flow events |
| `/sessions` | GET | List sessions |
| `/sessions` | POST | Create session |
| `/sessions/{session_id}` | GET | Get session details |
| `/sessions/{session_id}/messages` | GET | Get session messages |
| `/sessions/{session_id}` | DELETE | Delete session |

---

## gateway/routers/health.py

Governance router - System health and introspection.

### Response Models

```python
class ToolHealthBrief(BaseModel):
    tool_name: str
    status: str  # healthy, degraded, unhealthy
    success_rate: float
    last_used: Optional[str] = None

class DashboardResponse(BaseModel):
    tools: List[ToolHealthBrief]
    agents: List[AgentInfo]
    memory_layers: List[MemoryLayerInfo]
    config: ConfigInfo
```

### Endpoints

| Path | Method | Description |
|------|--------|-------------|
| `/dashboard` | GET | Full governance dashboard |
| `/health` | GET | System health summary |
| `/tools/{tool_name}` | GET | Tool health details |

---

## gateway/routers/interrupts.py

Unified interrupt handling router.

### Request/Response Models

```python
class InterruptResponseRequest(BaseModel):
    text: Optional[str] = None      # For clarification
    approved: Optional[bool] = None  # For confirmation
    decision: Optional[str] = None   # For critic review
    data: Optional[Dict[str, Any]] = None
```

### Endpoints

| Path | Method | Description |
|------|--------|-------------|
| `/interrupts/{id}` | GET | Get interrupt details |
| `/interrupts/{id}/respond` | POST | Respond to interrupt |
| `/interrupts/` | GET | List interrupts |
| `/interrupts/{id}` | DELETE | Cancel interrupt |

---

## Usage Example

```python
# Starting the gateway
import uvicorn
from avionics.gateway.main import app

uvicorn.run(app, host="0.0.0.0", port=8000)

# Sending a chat message
import httpx

response = httpx.post(
    "http://localhost:8000/api/v1/chat/messages",
    json={"message": "Hello"},
    params={"user_id": "user123"}
)
print(response.json())

# SSE streaming
async with httpx.AsyncClient() as client:
    async with client.stream("GET", "http://localhost:8000/api/v1/chat/stream", params={
        "user_id": "user123",
        "message": "Analyze this code"
    }) as response:
        async for line in response.aiter_lines():
            print(line)
```
