# System Capability Inventory

> **No speculation.** Each capability listed here links to the code path that proves its existence.

---

## Table of Contents

1. [User-Visible Capabilities](#user-visible-capabilities)
   - [HTTP REST API Endpoints](#http-rest-api-endpoints)
   - [SSE Streaming](#sse-streaming)
   - [WebSocket Real-Time Communication](#websocket-real-time-communication)
   - [gRPC Services](#grpc-services)
   - [Webhook Event Delivery](#webhook-event-delivery)
   - [Health & Readiness Probes](#health--readiness-probes)
   - [Governance Dashboard](#governance-dashboard)

2. [Internal Capabilities](#internal-capabilities)
   - [Pipeline Execution Engine](#pipeline-execution-engine)
   - [Control Tower Kernel](#control-tower-kernel)
   - [Interrupt System](#interrupt-system)
   - [LLM Provider Abstraction](#llm-provider-abstraction)
   - [Database Layer](#database-layer)
   - [Memory Module](#memory-module)
   - [Observability & Tracing](#observability--tracing)
   - [Distributed Execution](#distributed-execution)
   - [Communication Bus](#communication-bus)

---

## User-Visible Capabilities

### HTTP REST API Endpoints

| Capability | Endpoint | Code Path |
|------------|----------|-----------|
| Send chat message | `POST /api/v1/chat/messages` | `jeeves_avionics/gateway/routers/chat.py:186-346` |
| Create session | `POST /api/v1/chat/sessions` | `jeeves_avionics/gateway/routers/chat.py:461-501` |
| List sessions | `GET /api/v1/chat/sessions` | `jeeves_avionics/gateway/routers/chat.py:504-546` |
| Get session details | `GET /api/v1/chat/sessions/{session_id}` | `jeeves_avionics/gateway/routers/chat.py:549-582` |
| Get session messages | `GET /api/v1/chat/sessions/{session_id}/messages` | `jeeves_avionics/gateway/routers/chat.py:585-625` |
| Delete session | `DELETE /api/v1/chat/sessions/{session_id}` | `jeeves_avionics/gateway/routers/chat.py:628-658` |
| Get interrupt details | `GET /api/v1/interrupts/{interrupt_id}` | `jeeves_avionics/gateway/routers/interrupts.py:114-142` |
| Respond to interrupt | `POST /api/v1/interrupts/{interrupt_id}/respond` | `jeeves_avionics/gateway/routers/interrupts.py:145-235` |
| List interrupts | `GET /api/v1/interrupts/` | `jeeves_avionics/gateway/routers/interrupts.py:238-291` |
| Cancel interrupt | `DELETE /api/v1/interrupts/{interrupt_id}` | `jeeves_avionics/gateway/routers/interrupts.py:294-337` |
| Get governance dashboard | `GET /api/v1/governance/dashboard` | `jeeves_avionics/gateway/routers/governance.py:97-193` |
| Get health summary | `GET /api/v1/governance/health` | `jeeves_avionics/gateway/routers/governance.py:196-230` |
| Get tool health | `GET /api/v1/governance/tools/{tool_name}` | `jeeves_avionics/gateway/routers/governance.py:233-261` |

**Router Registration:**
```python
# jeeves_avionics/gateway/main.py:251-255
app.include_router(chat.router, prefix="/api/v1/chat", tags=["chat"])
app.include_router(governance.router, prefix="/api/v1/governance", tags=["governance"])
app.include_router(interrupts.router, prefix="/api/v1", tags=["interrupts"])
```

---

### SSE Streaming

| Capability | Endpoint | Code Path |
|------------|----------|-----------|
| Stream chat events | `GET /api/v1/chat/stream` | `jeeves_avionics/gateway/routers/chat.py:349-437` |

**Event Types Streamed:**
- `flow_started` - Flow initiated
- `token` - Streaming token from LLM
- `plan_created` - Execution plan generated
- `tool_started` / `tool_completed` - Tool lifecycle
- `response_ready` - Final response
- `clarification` / `confirmation` - Interrupt signals
- `error` - Error occurred

**Event Type Mapping:**
```python
# jeeves_avionics/gateway/routers/chat.py:665-696
mapping = {
    jeeves_pb2.FlowEvent.FLOW_STARTED: "flow_started",
    jeeves_pb2.FlowEvent.TOKEN: "token",
    jeeves_pb2.FlowEvent.PLAN_CREATED: "plan_created",
    # ... additional mappings
}
```

---

### WebSocket Real-Time Communication

| Capability | Endpoint | Code Path |
|------------|----------|-----------|
| WebSocket connection | `WS /ws` | `jeeves_avionics/gateway/main.py:263-318` |

**Supported Message Types:**
- `ping` → `pong` (heartbeat)
- `subscribe` → `subscribed` (channel subscription)
- General messages → `ack` (acknowledgment)

**Client Management:**
```python
# jeeves_avionics/gateway/websocket.py
register_client(websocket)
unregister_client(websocket)
```

---

### gRPC Services

| Service | Method | Code Path |
|---------|--------|-----------|
| JeevesCoreService | CreateEnvelope | `coreengine/grpc/server.go:54-83` |
| JeevesCoreService | UpdateEnvelope | `coreengine/grpc/server.go:86-97` |
| JeevesCoreService | CloneEnvelope | `coreengine/grpc/server.go:100-113` |
| JeevesCoreService | CheckBounds | `coreengine/grpc/server.go:119-144` |
| JeevesCoreService | ExecutePipeline (streaming) | `coreengine/grpc/server.go:151-232` |
| JeevesCoreService | ExecuteAgent | `coreengine/grpc/server.go:235-254` |
| JeevesFlowService | StartFlow (streaming) | `jeeves_mission_system/orchestrator/flow_service.py:76-101` |
| JeevesFlowService | GetSession | `jeeves_mission_system/orchestrator/flow_service.py:103-140` |
| JeevesFlowService | ListSessions | `jeeves_mission_system/orchestrator/flow_service.py:142-193` |
| JeevesFlowService | CreateSession | `jeeves_mission_system/orchestrator/flow_service.py:195-235` |
| JeevesFlowService | DeleteSession | `jeeves_mission_system/orchestrator/flow_service.py:237-273` |
| JeevesFlowService | GetSessionMessages | `jeeves_mission_system/orchestrator/flow_service.py:275-333` |

**Server Lifecycle:**
```go
// coreengine/grpc/server.go:588-619
func Start(address string, server *JeevesCoreServer) error
func StartBackground(address string, server *JeevesCoreServer) (*grpc.Server, error)
```

---

### Webhook Event Delivery

| Capability | Code Path |
|------------|-----------|
| Register webhook | `jeeves_avionics/webhooks/service.py:241-272` |
| Unregister webhook | `jeeves_avionics/webhooks/service.py:274-309` |
| Emit event to webhooks | `jeeves_avionics/webhooks/service.py:324-379` |
| HMAC signature verification | `jeeves_avionics/webhooks/service.py:579-593` |
| Exponential backoff retry | `jeeves_avionics/webhooks/service.py:440-504` |

**Webhook Configuration:**
```python
# jeeves_avionics/webhooks/service.py:62-98
@dataclass
class WebhookConfig:
    url: str
    events: List[str]  # Supports wildcards: "request.*", "*"
    secret: Optional[str] = None  # HMAC secret
    max_retries: int = 3
    retry_delay_seconds: float = 1.0
```

**Delivery Headers:**
- `X-Webhook-Event` - Event type
- `X-Webhook-ID` - Event ID
- `X-Webhook-Timestamp` - ISO timestamp
- `X-Webhook-Signature` - HMAC-SHA256 signature (if secret configured)

---

### Health & Readiness Probes

| Capability | Endpoint | Code Path |
|------------|----------|-----------|
| Liveness probe | `GET /health` | `jeeves_avionics/gateway/main.py:163-171` |
| Readiness probe | `GET /ready` | `jeeves_avionics/gateway/main.py:174-223` |
| Root API info | `GET /` | `jeeves_avionics/gateway/main.py:226-244` |

**Readiness Checks:**
- gRPC connection to orchestrator
- InterruptService availability

---

### Governance Dashboard

| Capability | Code Path |
|------------|-----------|
| Dashboard data aggregation | `jeeves_avionics/gateway/routers/governance.py:97-193` |
| Tool health summary | `jeeves_memory_module/services/tool_health_service.py:257-344` |
| System health aggregation | `jeeves_memory_module/services/tool_health_service.py:423-455` |

**Dashboard Components:**
- Tool health status (healthy/degraded/unhealthy)
- Agent information
- Memory layer status
- Configuration info

---

## Internal Capabilities

### Pipeline Execution Engine

| Capability | Code Path |
|------------|-----------|
| Runtime creation | `coreengine/runtime/runtime.go:59-83` |
| Agent building from config | `coreengine/runtime/runtime.go:85-116` |
| Sequential execution | `coreengine/runtime/runtime.go:254-306` |
| Parallel execution | `coreengine/runtime/runtime.go:309-411` |
| Streaming execution | `coreengine/runtime/runtime.go:427-455` |
| Interrupt resume | `coreengine/runtime/runtime.go:467-519` |
| State persistence | `coreengine/runtime/runtime.go:242-251` |

**Execution Modes:**
```go
// coreengine/runtime/runtime.go:16-21
const (
    RunModeSequential = config.RunModeSequential
    RunModeParallel   = config.RunModeParallel
)
```

**Bounds Checking:**
```go
// coreengine/runtime/runtime.go:221-239
func (r *Runtime) shouldContinue(env *envelope.GenericEnvelope) (bool, string)
```

---

### Control Tower Kernel

| Capability | Code Path |
|------------|-----------|
| Kernel initialization | `jeeves_control_tower/kernel.py:90-132` |
| Request submission | `jeeves_control_tower/kernel.py:167-215` |
| Process execution | `jeeves_control_tower/kernel.py:217-342` |
| Interrupt handling | `jeeves_control_tower/kernel.py:344-403` |
| Request resumption | `jeeves_control_tower/kernel.py:405-490` |
| Request cancellation | `jeeves_control_tower/kernel.py:519-544` |
| System status reporting | `jeeves_control_tower/kernel.py:618-645` |

**Subsystems Composed:**
```python
# jeeves_control_tower/kernel.py:114-125
self._lifecycle = LifecycleManager(logger, default_quota)
self._resources = ResourceTracker(logger, default_quota)
self._ipc = CommBusCoordinator(logger)
self._events = EventAggregator(logger)
self._interrupts = InterruptService(...)
```

**Resource Tracking:**
```python
# jeeves_control_tower/kernel.py:556-583
def record_llm_call(self, pid, tokens_in=0, tokens_out=0) -> Optional[str]
def record_tool_call(self, pid) -> Optional[str]
def record_agent_hop(self, pid) -> Optional[str]
```

---

### Interrupt System

| Capability | Code Path |
|------------|-----------|
| InterruptService initialization | `jeeves_control_tower/services/interrupt_service.py:155-191` |
| Create generic interrupt | `jeeves_control_tower/services/interrupt_service.py:197-294` |
| Get interrupt by ID | `jeeves_control_tower/services/interrupt_service.py:296-315` |
| Get pending for request | `jeeves_control_tower/services/interrupt_service.py:317-349` |
| Get pending for session | `jeeves_control_tower/services/interrupt_service.py:351-394` |
| Respond to interrupt | `jeeves_control_tower/services/interrupt_service.py:396-490` |
| Cancel interrupt | `jeeves_control_tower/services/interrupt_service.py:492-545` |
| Expire pending interrupts | `jeeves_control_tower/services/interrupt_service.py:547-608` |
| Create clarification | `jeeves_control_tower/services/interrupt_service.py:614-644` |
| Create confirmation | `jeeves_control_tower/services/interrupt_service.py:646-676` |
| Create resource exhausted | `jeeves_control_tower/services/interrupt_service.py:678-711` |

**Interrupt Types:**
```python
# jeeves_control_tower/services/interrupt_service.py:66-103
DEFAULT_INTERRUPT_CONFIGS = {
    InterruptKind.CLARIFICATION: InterruptConfig(default_ttl=timedelta(hours=24)),
    InterruptKind.CONFIRMATION: InterruptConfig(default_ttl=timedelta(hours=1)),
    InterruptKind.AGENT_REVIEW: InterruptConfig(default_ttl=timedelta(minutes=30)),
    InterruptKind.CHECKPOINT: InterruptConfig(auto_expire=False),
    InterruptKind.RESOURCE_EXHAUSTED: InterruptConfig(default_ttl=timedelta(minutes=5)),
    InterruptKind.TIMEOUT: InterruptConfig(default_ttl=timedelta(minutes=5)),
    InterruptKind.SYSTEM_ERROR: InterruptConfig(default_ttl=timedelta(hours=1)),
}
```

---

### LLM Provider Abstraction

| Capability | Provider | Code Path |
|------------|----------|-----------|
| Provider base class | Abstract | `jeeves_avionics/llm/providers/base.py:31-109` |
| OpenAI provider | OpenAI | `jeeves_avionics/llm/providers/openai.py` |
| Anthropic provider | Anthropic | `jeeves_avionics/llm/providers/anthropic.py` |
| Azure AI Foundry | Azure | `jeeves_avionics/llm/providers/azure.py` |
| llama.cpp (local) | LlamaCpp | `jeeves_avionics/llm/providers/llamacpp_provider.py` |
| llama-server (OpenAI API) | LlamaServer | `jeeves_avionics/llm/providers/llamaserver_provider.py` |
| Mock provider | Testing | `jeeves_avionics/llm/providers/mock.py` |

**Factory Functions:**
```python
# jeeves_avionics/llm/factory.py:46-130
def create_llm_provider(provider_type, settings, agent_name=None) -> LLMProvider
def create_agent_provider(settings, agent_name, override_provider=None) -> LLMProvider
def create_agent_provider_with_node_awareness(settings, agent_name, node_profiles) -> LLMProvider
```

**LLMFactory Class:**
```python
# jeeves_avionics/llm/factory.py:252-324
class LLMFactory:
    def get_provider_for_agent(self, agent_name, use_cache=True) -> LLMProvider
    def clear_cache(self)
    def mark_node_healthy(self, node_name, is_healthy)
```

---

### Database Layer

| Capability | Code Path |
|------------|-----------|
| PostgreSQL client | `jeeves_avionics/database/postgres_client.py:33-767` |
| Connection pooling | `jeeves_avionics/database/postgres_client.py:172-228` |
| Transaction management | `jeeves_avionics/database/postgres_client.py:269-290` |
| Schema initialization | `jeeves_avionics/database/postgres_client.py:292-436` |
| Query execution | `jeeves_avionics/database/postgres_client.py:438-515` |
| Health check | `jeeves_avionics/database/postgres_client.py:656-697` |
| Table statistics | `jeeves_avionics/database/postgres_client.py:699-743` |
| VACUUM ANALYZE | `jeeves_avionics/database/postgres_client.py:745-763` |
| Database factory | `jeeves_avionics/database/factory.py:31-116` |
| Registry pattern | `jeeves_avionics/database/registry.py` |

**pgvector Support:**
```python
# jeeves_avionics/database/postgres_client.py:213-223
result = await conn.execute(
    text("SELECT extname, extversion FROM pg_extension WHERE extname = 'vector'")
)
```

---

### Memory Module

| Capability | Code Path |
|------------|-----------|
| Session state management | `jeeves_memory_module/services/session_state_service.py:34-333` |
| Focus tracking | `jeeves_memory_module/services/session_state_service.py:116-152` |
| Entity reference recording | `jeeves_memory_module/services/session_state_service.py:154-181` |
| Turn counting | `jeeves_memory_module/services/session_state_service.py:183-200` |
| Summarization trigger | `jeeves_memory_module/services/session_state_service.py:296-314` |
| Tool health tracking | `jeeves_memory_module/services/tool_health_service.py:75-534` |
| Circuit breaker logic | `jeeves_memory_module/services/tool_health_service.py:346-379` |
| Embedding generation | `jeeves_memory_module/services/embedding_service.py:21-276` |
| Similarity calculation | `jeeves_memory_module/services/embedding_service.py:190-230` |
| Embedding cache | `jeeves_memory_module/services/embedding_service.py:67-113` |

**Repositories:**
- `session_state_repository.py` - Session state persistence
- `tool_metrics_repository.py` - Tool execution metrics
- `event_repository.py` - Event storage
- `trace_repository.py` - Trace data
- `chunk_repository.py` - Text chunk storage
- `pgvector_repository.py` - Vector search

---

### Observability & Tracing

| Capability | Code Path |
|------------|-----------|
| OpenTelemetry adapter | `jeeves_avionics/observability/otel_adapter.py:177-528` |
| Span management | `jeeves_avionics/observability/otel_adapter.py:221-273` |
| Agent span events | `jeeves_avionics/observability/otel_adapter.py:300-375` |
| Tool span events | `jeeves_avionics/observability/otel_adapter.py:376-406` |
| LLM call spans | `jeeves_avionics/observability/otel_adapter.py:408-438` |
| Trace context propagation | `jeeves_avionics/observability/otel_adapter.py:147-174` |
| Tracer creation | `jeeves_avionics/observability/otel_adapter.py:100-134` |
| Metrics collection | `jeeves_avionics/observability/metrics.py` |

**TracingContext:**
```python
# jeeves_avionics/observability/otel_adapter.py:66-97
@dataclass
class TracingContext:
    trace_id: str
    span_id: str
    trace_flags: int = 1
    trace_state: str = ""
    baggage: Dict[str, str] = field(default_factory=dict)
```

---

### Distributed Execution

| Capability | Code Path |
|------------|-----------|
| Redis client | `jeeves_avionics/database/redis_client.py:23-386` |
| Rate limiting | `jeeves_avionics/database/redis_client.py:82-149` |
| Distributed locks | `jeeves_avionics/database/redis_client.py:151-219` |
| Embedding cache | `jeeves_avionics/database/redis_client.py:221-276` |
| Distributed bus | `jeeves_avionics/distributed/redis_bus.py:24-447` |
| Task enqueueing | `jeeves_avionics/distributed/redis_bus.py:78-127` |
| Task dequeueing | `jeeves_avionics/distributed/redis_bus.py:129-196` |
| Task completion | `jeeves_avionics/distributed/redis_bus.py:198-239` |
| Task failure/retry | `jeeves_avionics/distributed/redis_bus.py:241-303` |
| Worker registration | `jeeves_avionics/distributed/redis_bus.py:305-335` |
| Queue statistics | `jeeves_avionics/distributed/redis_bus.py:372-408` |

**Distributed Primitives:**
- Priority queues (Redis sorted sets)
- Worker heartbeat
- Dead letter detection
- Exponential backoff retry

---

### Communication Bus

| Capability | Code Path |
|------------|-----------|
| In-memory bus | `commbus/bus.go:10-368` |
| Event publish (fan-out) | `commbus/bus.go:57-113` |
| Command send | `commbus/bus.go:117-150` |
| Query with timeout | `commbus/bus.go:154-200` |
| Event subscription | `commbus/bus.go:209-234` |
| Handler registration | `commbus/bus.go:238-249` |
| Middleware chain | `commbus/bus.go:253-259`, `commbus/bus.go:326-364` |
| Handler introspection | `commbus/bus.go:266-303` |

**Message Patterns:**
```go
// commbus/bus.go
Publish() - Event fan-out to multiple subscribers
Send()    - Command fire-and-forget
QuerySync() - Request-response with timeout
```

---

## Architecture Summary

```
┌─────────────────────────────────────────────────────────────────┐
│                     USER-VISIBLE LAYER                          │
├─────────────────────────────────────────────────────────────────┤
│  HTTP REST API  │  SSE Stream  │  WebSocket  │  gRPC  │ Webhooks │
│  (FastAPI)      │              │             │        │          │
└────────────────────────────────┬────────────────────────────────┘
                                 │
┌────────────────────────────────▼────────────────────────────────┐
│                     CONTROL TOWER (Kernel)                      │
├─────────────────────────────────────────────────────────────────┤
│  Lifecycle  │  Resources  │  IPC/CommBus  │  Events  │ Interrupts│
└────────────────────────────────┬────────────────────────────────┘
                                 │
┌────────────────────────────────▼────────────────────────────────┐
│                     PIPELINE ENGINE                              │
├─────────────────────────────────────────────────────────────────┤
│  Runtime (Go)  │  Agents  │  Sequential/Parallel  │  Streaming  │
└────────────────────────────────┬────────────────────────────────┘
                                 │
┌────────────────────────────────▼────────────────────────────────┐
│                     AVIONICS LAYER                               │
├─────────────────────────────────────────────────────────────────┤
│  LLM Providers  │  Database  │  Memory  │  Observability  │ Redis│
└─────────────────────────────────────────────────────────────────┘
```

---

## Verification Commands

To verify these capabilities exist, run:

```bash
# Check gateway endpoints
grep -n "router\." jeeves_avionics/gateway/routers/*.py

# Check gRPC services
grep -n "async def" coreengine/grpc/server.go

# Check interrupt types
grep -n "InterruptKind" jeeves_control_tower/services/interrupt_service.py

# Check LLM providers
ls -la jeeves_avionics/llm/providers/

# Check database capabilities
grep -n "async def" jeeves_avionics/database/postgres_client.py | head -20
```

---

*Document generated: 2026-01-19*
*Based on codebase analysis - no speculation*
