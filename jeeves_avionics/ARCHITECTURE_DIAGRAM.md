# Jeeves Avionics Architecture Diagram

**Companion to:** [CONSTITUTION.md](CONSTITUTION.md)
**Version:** 2.0
**Last Updated:** 2025-12-06

---

## Overview

This document provides architectural views for **Jeeves Avionics**—the infrastructure layer that implements core protocols and manages external system integrations (LLM, database, memory, gateway).

**Key Principle:** Avionics provides **transport and storage**, not **business logic**.

**Control Tower Integration:** Avionics infrastructure is consumed by Control Tower (kernel layer) and Mission System. Control Tower uses Avionics for database access, LLM calls, and event streaming, while enforcing resource quotas and lifecycle management.

---

## Intra-Level: Avionics Internal Architecture

### Module Organization

```
jeeves_avionics/
│
├─ llm/                      # LLM Provider Layer
│  ├─ providers/
│  │  ├─ openai_adapter.py   # OpenAI implementation
│  │  ├─ anthropic_adapter.py # Anthropic implementation
│  │  ├─ azure_adapter.py    # Azure OpenAI implementation
│  │  └─ llamaserver_adapter.py # Local llama.cpp server
│  ├─ factory.py             # Provider factory
│  ├─ token_counter.py       # Token counting utilities
│  └─ rate_limiter.py        # Rate limiting logic
│
├─ database/                 # Database Layer
│  ├─ client.py              # PostgreSQL connection management
│  ├─ repositories/
│  │  ├─ session_repository.py # Session CRUD
│  │  ├─ event_repository.py   # Event log
│  │  └─ cache_repository.py   # L4 persistent cache
│  ├─ schemas/
│  │  ├─ sessions.py         # SQLAlchemy models
│  │  ├─ events.py
│  │  └─ embeddings.py       # pgvector tables
│  └─ migrations/            # Alembic migrations
│
├─ memory/                   # Memory Layer
│  ├─ services/
│  │  ├─ memory_service.py   # MemoryServiceProtocol implementation
│  │  ├─ l1_episodic.py      # Per-request transient state
│  │  ├─ l2_event_log.py     # Append-only event stream
│  │  ├─ l3_working_memory.py # Session-scoped state
│  │  └─ l4_persistent_cache.py # Domain index, embeddings
│  ├─ adapters/
│  │  └─ state_adapter.py    # StateProtocol implementation
│  └─ repositories/
│     ├─ session_state_repo.py
│     └─ event_log_repo.py
│
├─ gateway/                  # HTTP Gateway Layer
│  ├─ main.py                # FastAPI app initialization
│  ├─ routers/
│  │  ├─ chat.py             # Chat endpoints
│  │  ├─ governance.py       # Governance dashboard
│  │  └─ health.py           # Health checks
│  ├─ middleware.py          # Request logging, CORS
│  └─ static/                # Static file serving
│
├─ settings.py               # Pydantic settings
├─ feature_flags.py          # Feature flag management
├─ wiring.py                 # Dependency injection
└─ thresholds.py             # Runtime thresholds
```

---

## Intra-Level: LLM Layer Architecture

### Provider Abstraction

```
┌───────────────────────────────────────────────────────────────┐
│  LLMProtocol (from jeeves_core_engine)                        │
│  • generate(prompt: str) -> str                               │
│  • generate_structured(prompt: str, schema: dict) -> dict     │
│  • health_check() -> bool                                     │
└───────────────────────────┬───────────────────────────────────┘
                            │ implemented by
                ┌───────────┴───────────┐
                │                       │
                ↓                       ↓
┌──────────────────────────┐  ┌──────────────────────────┐
│  OpenAIAdapter           │  │  AnthropicAdapter        │
│                          │  │                          │
│  • client: OpenAI        │  │  • client: Anthropic     │
│  • model: str            │  │  • model: str            │
│  • api_key: str          │  │  • api_key: str          │
│                          │  │                          │
│  generate():             │  │  generate():             │
│    → chat.completions    │  │    → messages.create     │
│                          │  │                          │
│  Retry: 3 attempts       │  │  Retry: 3 attempts       │
│  Rate limit: 60 req/min  │  │  Rate limit: 50 req/min  │
└──────────────────────────┘  └──────────────────────────┘

                ┌───────────┴───────────┐
                │                       │
                ↓                       ↓
┌──────────────────────────┐  ┌──────────────────────────┐
│  AzureOpenAIAdapter      │  │  LlamaServerAdapter      │
│                          │  │                          │
│  • client: AzureOpenAI   │  │  • base_url: str         │
│  • deployment: str       │  │  • model: str            │
│  • api_key: str          │  │                          │
│                          │  │  generate():             │
│  generate():             │  │    → POST /completion    │
│    → chat.completions    │  │                          │
│                          │  │  No rate limit           │
│  Retry: 3 attempts       │  │  Retry: 2 attempts       │
└──────────────────────────┘  └──────────────────────────┘
```

### Provider Factory

```
┌───────────────────────────────────────────────────────────────┐
│  LLM Provider Factory                                         │
│                                                               │
│  def create_llm_provider(                                     │
│      provider: str,                                           │
│      settings: Settings                                       │
│  ) -> LLMProtocol:                                            │
│                                                               │
│      if provider == "openai":                                 │
│          return OpenAIAdapter(                                │
│              api_key=settings.openai_api_key,                 │
│              model=settings.default_model,                    │
│          )                                                    │
│                                                               │
│      elif provider == "anthropic":                            │
│          return AnthropicAdapter(                             │
│              api_key=settings.anthropic_api_key,              │
│              model=settings.default_model,                    │
│          )                                                    │
│                                                               │
│      elif provider == "azure":                                │
│          return AzureOpenAIAdapter(                           │
│              api_key=settings.azure_api_key,                  │
│              endpoint=settings.azure_endpoint,                │
│              deployment=settings.azure_deployment,            │
│          )                                                    │
│                                                               │
│      elif provider == "llamaserver":                          │
│          return LlamaServerAdapter(                           │
│              base_url=settings.llamaserver_host,              │
│              model=settings.default_model,                    │
│          )                                                    │
│                                                               │
│      else:                                                    │
│          raise ValueError(f"Unknown provider: {provider}")    │
└───────────────────────────────────────────────────────────────┘
```

**Configuration-driven:** Provider selection via `LLM_PROVIDER` env var.

---

## Intra-Level: Database Layer Architecture

### PostgreSQL Connection Management

```
┌───────────────────────────────────────────────────────────────┐
│  DatabaseClient                                               │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Connection Pool                                         │ │
│  │  • min_size: 5                                           │ │
│  │  • max_size: 20                                          │ │
│  │  • idle_timeout: 300s                                    │ │
│  │  • max_lifetime: 3600s                                   │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Transaction Management                                  │ │
│  │  • Auto-commit: False                                    │ │
│  │  • Isolation level: READ COMMITTED                       │ │
│  │  • Retry on deadlock: 3 attempts                         │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Health Monitoring                                       │ │
│  │  • Ping interval: 30s                                    │ │
│  │  • Reconnect on failure                                  │ │
│  │  • Circuit breaker: 5 consecutive failures               │ │
│  └─────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────┘
```

### Repository Pattern

```
┌───────────────────────────────────────────────────────────────┐
│  Session Repository                                           │
│                                                               │
│  class SessionRepository:                                     │
│      def __init__(self, db_client: DatabaseClient):           │
│          self.db = db_client                                  │
│                                                               │
│      async def save(self, session: Session) -> None:          │
│          """Insert or update session."""                      │
│          async with self.db.transaction():                    │
│              await self.db.execute(                           │
│                  "INSERT INTO sessions ... ON CONFLICT ..."   │
│              )                                                │
│                                                               │
│      async def load(self, session_id: str) -> Session:        │
│          """Retrieve session by ID."""                        │
│          row = await self.db.fetch_one(                       │
│              "SELECT * FROM sessions WHERE id = $1",          │
│              session_id                                       │
│          )                                                    │
│          return Session.from_row(row)                         │
│                                                               │
│      async def delete(self, session_id: str) -> None:         │
│          """Delete session and related data."""               │
│          async with self.db.transaction():                    │
│              await self.db.execute(...)                       │
└───────────────────────────────────────────────────────────────┘
```

**Similar repositories:** `EventRepository`, `CacheRepository`

---

## Intra-Level: Memory Layer Architecture

### Four-Layer Memory System

```
┌───────────────────────────────────────────────────────────────┐
│  L1: Episodic (Per-Request Transient State)                  │
│                                                               │
│  • Storage: In-memory (CoreEnvelope)                          │
│  • Lifetime: Single request                                   │
│  • Contents: Agent outputs, working state                     │
│  • Persistence: None (discarded after response)               │
│                                                               │
│  class EpisodicMemory:                                        │
│      def __init__(self):                                      │
│          self.envelope = CoreEnvelope(...)                    │
│                                                               │
│      def get_output(self, agent: str) -> AgentOutput:         │
│          return self.envelope.agent_outputs.get(agent)        │
└───────────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────────┐
│  L2: Event Log (Append-Only Request History)                 │
│                                                               │
│  • Storage: PostgreSQL (events table)                         │
│  • Lifetime: Permanent (audit trail)                          │
│  • Contents: Agent execution records, tool calls              │
│  • Persistence: Written after each agent                      │
│                                                               │
│  class EventLogService:                                       │
│      async def append(self, event: Event) -> None:            │
│          await self.event_repo.insert(event)                  │
│                                                               │
│      async def query(                                         │
│          self,                                                │
│          session_id: str,                                     │
│          filters: dict                                        │
│      ) -> List[Event]:                                        │
│          return await self.event_repo.query(...)              │
└───────────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────────┐
│  L3: Working Memory (Session-Scoped State)                    │
│                                                               │
│  • Storage: PostgreSQL (sessions table)                       │
│  • Lifetime: Session (across multiple queries)                │
│  • Contents: TraversalState, discovered items, findings       │
│  • Persistence: Updated after Integration                     │
│                                                               │
│  class WorkingMemoryService:                                  │
│      async def load_context(                                  │
│          self,                                                │
│          session_id: str                                      │
│      ) -> SessionContext:                                     │
│          session = await self.session_repo.load(session_id)   │
│          return SessionContext(                               │
│              discovered_items=session.discovered_items,       │
│              findings=session.findings,                       │
│          )                                                    │
│                                                               │
│      async def save_state(                                    │
│          self,                                                │
│          session_id: str,                                     │
│          state: TraversalState                                │
│      ) -> None:                                               │
│          await self.session_repo.update(...)                  │
└───────────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────────┐
│  L4: Persistent Cache (Long-Term Knowledge)                   │
│                                                               │
│  • Storage: PostgreSQL + pgvector                             │
│  • Lifetime: Permanent (until invalidated)                    │
│  • Contents: Domain index, embeddings, entity graph           │
│  • Persistence: Built incrementally                           │
│                                                               │
│  class PersistentCacheService:                                │
│      async def get_entity(                                    │
│          self,                                                │
│          name: str,                                           │
│          scope: str                                           │
│      ) -> Optional[Entity]:                                   │
│          return await self.cache_repo.query_entity(...)       │
│                                                               │
│      async def search_embeddings(                             │
│          self,                                                │
│          query_vector: List[float],                           │
│          limit: int                                           │
│      ) -> List[Chunk]:                                        │
│          return await self.cache_repo.vector_search(...)      │
└───────────────────────────────────────────────────────────────┘
```

---

## Intra-Level: Gateway Layer Architecture

### FastAPI Application Structure

```
┌───────────────────────────────────────────────────────────────┐
│  FastAPI Gateway                                              │
│                                                               │
│  app = FastAPI(title="Jeeves Runtime API")             │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Middleware Stack                                        │ │
│  │  1. CORS (allow origins)                                 │ │
│  │  2. Request logging (structured logs)                    │ │
│  │  3. Error handling (HTTP exceptions)                     │ │
│  │  4. Metrics (Prometheus)                                 │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Routers                                                 │ │
│  │  • /api/v1/chat/* (chat_router)                          │ │
│  │  • /api/v1/governance/* (governance_router)              │ │
│  │  • /health, /ready (health_router)                       │ │
│  │  • /docs (OpenAPI/Swagger UI)                            │ │
│  │  • /static/* (static files)                              │ │
│  └─────────────────────────────────────────────────────────┘ │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  Dependency Injection                                    │ │
│  │  • get_llm_provider()                                    │ │
│  │  • get_database_client()                                 │ │
│  │  • get_memory_service()                                  │ │
│  │  • get_orchestrator_client()                             │ │
│  └─────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────┘
```

### Chat API Flow

```
POST /api/v1/chat/messages
│
├─> Router (chat.py)
│   │
│   ├─> Validate request (Pydantic model)
│   │
│   ├─> Get dependencies (DI)
│   │   ├─ llm_provider
│   │   ├─ memory_service
│   │   └─ orchestrator_client
│   │
│   ├─> Call orchestrator service (gRPC or local)
│   │   └─> Orchestrator.process_query(...)
│   │
│   ├─> Stream response (SSE)
│   │   ├─ Agent events
│   │   ├─ Intermediate results
│   │   └─ Final response
│   │
│   └─> Return HTTP 200 with response
│
└─> Client receives JSON response
```

---

## Inter-Level: Protocol Implementation

### How Avionics Implements Core Protocols

```
┌───────────────────────────────────────────────────────────────┐
│  Core Protocol: StateProtocol                                 │
│                                                               │
│  class StateProtocol(Protocol):                               │
│      async def save(self, key: str, value: Any) -> None       │
│      async def load(self, key: str) -> Optional[Any]          │
│      async def delete(self, key: str) -> None                 │
│      async def list_keys(self, prefix: str) -> List[str]      │
└───────────────────────────┬───────────────────────────────────┘
                            │ implemented by
                            ↓
┌───────────────────────────────────────────────────────────────┐
│  Avionics Implementation: StateAdapter                        │
│                                                               │
│  class StateAdapter(StateProtocol):                           │
│      def __init__(self, db_client: DatabaseClient):           │
│          self.db = db_client                                  │
│          self.session_repo = SessionRepository(db_client)     │
│                                                               │
│      async def save(self, key: str, value: Any) -> None:      │
│          """Save to PostgreSQL sessions table."""             │
│          session_id = key.split(":")[1]                       │
│          await self.session_repo.save(                        │
│              Session(id=session_id, state=value)              │
│          )                                                    │
│                                                               │
│      async def load(self, key: str) -> Optional[Any]:         │
│          """Load from PostgreSQL."""                          │
│          session_id = key.split(":")[1]                       │
│          session = await self.session_repo.load(session_id)   │
│          return session.state if session else None            │
└───────────────────────────────────────────────────────────────┘
```

**Similar implementations:**
- `LLMProtocol` → `OpenAIAdapter`, `AnthropicAdapter`, etc.
- `EventBusProtocol` → `PostgreSQLEventBus`
- `MemoryServiceProtocol` → `MemoryService`

---

## Inter-Level: Service Wiring

### Dependency Injection Setup

```
┌───────────────────────────────────────────────────────────────┐
│  Wiring Module (jeeves_avionics/wiring.py)                    │
│                                                               │
│  def create_adapters(settings: Settings) -> RuntimeAdapters:  │
│      """Wire up all infrastructure adapters."""               │
│                                                               │
│      # Database                                               │
│      db_client = DatabaseClient(                              │
│          dsn=settings.get_postgres_url()                      │
│      )                                                        │
│                                                               │
│      # LLM provider                                           │
│      llm = create_llm_provider(                               │
│          provider=settings.llm_provider,                      │
│          settings=settings                                    │
│      )                                                        │
│                                                               │
│      # Memory service                                         │
│      memory = MemoryService(                                  │
│          db_client=db_client,                                 │
│          event_repo=EventRepository(db_client),               │
│          session_repo=SessionRepository(db_client),           │
│      )                                                        │
│                                                               │
│      # State adapter                                          │
│      state = StateAdapter(db_client=db_client)                │
│                                                               │
│      # Event bus                                              │
│      events = PostgreSQLEventBus(db_client=db_client)         │
│                                                               │
│      return RuntimeAdapters(                                  │
│          llm=llm,                                             │
│          tools=None,  # Provided by Mission System            │
│          state=state,                                         │
│          events=events,                                       │
│          memory=memory,                                       │
│      )                                                        │
└───────────────────────────────────────────────────────────────┘
```

**Flow:**
1. Load `Settings` from environment
2. Create infrastructure clients (DB, LLM)
3. Wrap clients in protocol adapters
4. Return `RuntimeAdapters` to Mission System

---

## Inter-Level: External System Interactions

### LLM API Calls

```
┌───────────────────────────────────────────────────────────────┐
│  Mission System Agent                                         │
│  └─> context.llm.generate(prompt)                             │
└───────────────────────────┬───────────────────────────────────┘
                            │ protocol call
                            ↓
┌───────────────────────────────────────────────────────────────┐
│  Avionics: OpenAIAdapter                                      │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  1. Rate limiting check                                  │ │
│  │  2. Token counting (estimate cost)                       │ │
│  │  3. Retry policy setup                                   │ │
│  │  4. Call OpenAI API                                      │ │
│  │  5. Parse response                                       │ │
│  │  6. Log metrics (tokens, latency, cost)                  │ │
│  │  7. Return text to agent                                 │ │
│  └─────────────────────────────────────────────────────────┘ │
└───────────────────────────┬───────────────────────────────────┘
                            │ HTTP/JSON
                            ↓
┌───────────────────────────────────────────────────────────────┐
│  External: OpenAI API                                         │
│  POST https://api.openai.com/v1/chat/completions              │
└───────────────────────────────────────────────────────────────┘
```

### Database Operations

```
┌───────────────────────────────────────────────────────────────┐
│  Core Runtime                                                 │
│  └─> context.state.save(key, value)                           │
└───────────────────────────┬───────────────────────────────────┘
                            │ protocol call
                            ↓
┌───────────────────────────────────────────────────────────────┐
│  Avionics: StateAdapter                                       │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │  1. Serialize value (JSON)                               │ │
│  │  2. Start transaction                                    │ │
│  │  3. Execute INSERT/UPDATE                                │ │
│  │  4. Commit transaction                                   │ │
│  │  5. Handle errors (retry on deadlock)                    │ │
│  └─────────────────────────────────────────────────────────┘ │
└───────────────────────────┬───────────────────────────────────┘
                            │ SQL
                            ↓
┌───────────────────────────────────────────────────────────────┐
│  External: PostgreSQL                                         │
│  INSERT INTO sessions (id, state, updated_at) VALUES (...)    │
│  ON CONFLICT (id) DO UPDATE SET state = ..., updated_at = ... │
└───────────────────────────────────────────────────────────────┘
```

---

## Intra-Level: Settings Management

### Configuration Hierarchy

```
┌───────────────────────────────────────────────────────────────┐
│  Settings (Pydantic BaseSettings)                             │
│                                                               │
│  class Settings(BaseSettings):                                │
│      # PostgreSQL                                             │
│      postgres_host: str = "localhost"                         │
│      postgres_port: int = 5432                                │
│      postgres_database: str = "assistant"                     │
│      postgres_user: str = "assistant"                         │
│      postgres_password: str = ""                              │
│                                                               │
│      # LLM Provider                                           │
│      llm_provider: str = "llamaserver"                        │
│      default_model: str = "qwen2.5-3b-instruct"               │
│      llamaserver_host: str = "http://localhost:8080"          │
│      openai_api_key: str = ""                                 │
│      anthropic_api_key: str = ""                              │
│                                                               │
│      # Per-agent model overrides                              │
│      planner_model: Optional[str] = None                      │
│      critic_model: Optional[str] = None                       │
│                                                               │
│      # Gateway                                                │
│      gateway_host: str = "0.0.0.0"                            │
│      gateway_port: int = 8001                                 │
│                                                               │
│      class Config:                                            │
│          env_file = ".env"                                    │
│          env_file_encoding = "utf-8"                          │
│                                                               │
│      def get_postgres_url(self) -> str:                       │
│          """Build PostgreSQL DSN."""                          │
│          return f"postgresql://{self.postgres_user}:..."      │
└───────────────────────────────────────────────────────────────┘
```

**Load order:** Code defaults → `.env` file → Environment variables

---

## Inter-Level: Health and Observability

### Health Checks

```
GET /health (Liveness)
│
├─> Return 200 OK (process is alive)
│
└─> No dependency checks

GET /ready (Readiness)
│
├─> Check PostgreSQL connection
│   ├─ Execute: SELECT 1
│   └─ ✅ Connected / ❌ Connection failed
│
├─> Check LLM provider (optional)
│   ├─ Call: health_check()
│   └─ ✅ Reachable / ⚠️ Not critical
│
└─> Return 200 OK (ready) or 503 Service Unavailable (not ready)
```

### Structured Logging

```
┌───────────────────────────────────────────────────────────────┐
│  Structured Logging (structlog)                               │
│                                                               │
│  logger.info(                                                 │
│      "llm_request_complete",                                  │
│      provider="openai",                                       │
│      model="gpt-4",                                           │
│      tokens=1234,                                             │
│      latency_ms=567,                                          │
│      cost_usd=0.0123,                                         │
│  )                                                            │
│                                                               │
│  Output (JSON):                                               │
│  {                                                            │
│    "event": "llm_request_complete",                           │
│    "provider": "openai",                                      │
│    "model": "gpt-4",                                          │
│    "tokens": 1234,                                            │
│    "latency_ms": 567,                                         │
│    "cost_usd": 0.0123,                                        │
│    "timestamp": "2025-12-06T10:30:00Z",                       │
│    "level": "info"                                            │
│  }                                                            │
└───────────────────────────────────────────────────────────────┘
```

---

## Cross-References

- **Constitution:** [CONSTITUTION.md](CONSTITUTION.md)
- **Control Tower:** [../jeeves_control_tower/](../jeeves_control_tower/) - Kernel layer (primary consumer)
- **Index:** [INDEX.md](INDEX.md)
- **Related Diagrams:**
  - [../jeeves_control_tower/CONSTITUTION.md](../jeeves_control_tower/CONSTITUTION.md)
  - [../jeeves_mission_system/ARCHITECTURE_DIAGRAM.md](../jeeves_mission_system/ARCHITECTURE_DIAGRAM.md)

---

*This architecture diagram shows the internal structure of the Avionics layer (intra-level) and how it implements Core Engine protocols while serving Mission System (inter-level), maintaining the adapter pattern and configuration-driven design.*
