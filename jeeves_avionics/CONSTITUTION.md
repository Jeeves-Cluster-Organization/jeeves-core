# Jeeves Avionics Constitution

**Parent:** [Architectural Contracts](../docs/CONTRACTS.md)
**Updated:** 2026-01-06

---

## Overview

This constitution extends the **Jeeves Architectural Contracts** and defines the rules for **jeeves_avionics**—the infrastructure layer that provides implementations of core protocols and manages external system integrations.

**Architecture:**
- Provides `Settings` and `FeatureFlags` for infrastructure configuration
- Provides `thresholds.py` for operational thresholds (confirmations, retries, L7 governance)
- Per-agent LLM config centralized in `mission_system.config.agent_profiles`
- Domain configs (language, tool access) owned by capability layer

**Dependency:** This component implements protocols from `jeeves_protocols`. All architectural contracts apply.

---

## Purpose

**Avionics responsibilities:**
- LLM provider adapters (OpenAI, Anthropic, llamaserver, Azure)
- Database clients (PostgreSQL, pgvector)
- Memory services (L1-L4 layers)
- FastAPI gateway (HTTP/SSE endpoints)
- Settings and feature flag management

**Avionics wraps core:** Depends on core protocols, provides implementations.

---

## Core Principles (From Architectural Contracts)

From [Architectural Contracts](../docs/CONTRACTS.md):
- **Adapter Pattern** — Implement protocols, never modify them
- **Configuration Over Code** — Infrastructure behavior is configurable
- **Swappable Implementations** — Components can be replaced without affecting higher layers
- **Defensive Error Handling** — Infrastructure failures must not crash the system

**All architectural contracts apply to this component.**

---

## Component Rules

### R1: Adapter Pattern

Avionics **implements** core protocols, never modifies them:

```python
# Protocols package defines the protocol
from jeeves_protocols.protocols import LLMProtocol

# Avionics implements it
class OpenAIAdapter(LLMProtocol):
    async def generate(self, prompt: str) -> str:
        response = await self.client.chat.completions.create(...)
        return response.choices[0].message.content
```

**Multiple adapters per protocol** are allowed (e.g., multiple LLM providers).

### R2: Configuration Over Code

All infrastructure behavior is **configurable**, not hardcoded:

```python
@dataclass
class Settings:
    """Infrastructure settings (Pydantic BaseSettings)."""

    # Database
    postgres_host: str = "localhost"
    postgres_port: int = 5432
    postgres_database: str = "assistant"

    # LLM Provider
    llm_provider: str = "llamaserver"  # openai, anthropic, azure, llamaserver
    llamaserver_host: str = "http://localhost:8080"

    # Feature Flags
    use_graph_engine: bool = False
    enable_tracing: bool = False
```

**Environment variables** override defaults. **No magic values.**

### R3: No Domain Logic

Avionics provides **transport and storage**, not **business logic**:

**Avionics does:**
- Store session data in PostgreSQL
- Call LLM APIs
- Serve HTTP endpoints
- Manage database connections

**Avionics does NOT:**
- Decide which agent runs next (Mission System)
- Validate code citations (Mission System)
- Execute tools (Mission System)
- Implement 7-agent pipeline (Mission System)

### R4: Swappable Implementations

Any infrastructure component can be replaced **without** changing core or mission system:

**Database Layer:**
- `DatabaseClientProtocol` (from commbus) defines the interface
- `PostgreSQLClient` implements the protocol
- Registry pattern (`jeeves_avionics/database/registry.py`) enables backend selection
- Factory function `create_database_client()` creates appropriate client

**Capability Resource Registry (NEW):**
- `CapabilityResourceRegistryProtocol` (from jeeves_protocols) defines the interface
- Capabilities register their resources (schemas, gateway modes) at startup
- Infrastructure queries registry instead of hardcoding capability knowledge
- Enables non-capability layers to be extracted as a separate package

```python
# Capability registration (at startup)
from jeeves_protocols import get_capability_resource_registry, CapabilityModeConfig

registry = get_capability_resource_registry()
registry.register_schema("code_analysis", "/path/to/schema.sql")
registry.register_mode("code_analysis", CapabilityModeConfig(
    mode_id="code_analysis",
    response_fields=["files_examined", "citations", "thread_id"],
))

# Infrastructure usage (avionics/gateway)
from jeeves_protocols import get_capability_resource_registry

registry = get_capability_resource_registry()
schemas = registry.get_schemas()  # Returns all registered schemas
mode_config = registry.get_mode_config(mode_id)  # Returns mode config
```

**Examples:**
- Swap OpenAI for Anthropic → Change `llm_provider` setting
- Swap PostgreSQL for another backend → Implement `DatabaseClientProtocol`, register in registry
- Swap FastAPI for gRPC → Replace gateway module
- Add new capability → Register resources via CapabilityResourceRegistry

**Core and Mission System remain unchanged.**

### R5: Capability-Owned Tool Identifiers

**ToolId enums are CAPABILITY-OWNED, not avionics-owned:**

```python
# ❌ REMOVED - Avionics no longer defines ToolId
# from jeeves_avionics.tools.catalog import ToolId

# ✅ Avionics provides generic tool infrastructure
from jeeves_avionics.tools.catalog import ToolCategory, ToolDefinition
from jeeves_avionics.wiring import ToolExecutor  # Uses string tool names

# ✅ Capabilities define their own ToolId
# In my_capability/tools/catalog.py:
class ToolId(str, Enum):
    LOCATE = "locate"
    READ_CODE = "read_code"
```

**Avionics provides:**
- `ToolCategory` — Generic categories (COMPOSITE, RESILIENT, STANDALONE)
- `ToolDefinition` — Generic tool metadata structure
- `ToolExecutor` — Executes tools by string name
- `ToolExecutorCore` — Base execution with access control

**Capabilities provide:**
- `ToolId` enum — Capability-specific tool identifiers
- `ToolCatalog` — Capability-specific tool registry
- Tool implementations

### R6: Defensive Error Handling

Infrastructure failures **must not** crash the system:

```python
async def generate_text(prompt: str) -> str:
    try:
        return await llm_client.generate(prompt)
    except RateLimitError:
        await asyncio.sleep(exponential_backoff())
        return await llm_client.generate(prompt)  # Retry
    except ConnectionError as e:
        logger.error("llm_connection_failed", error=str(e))
        raise TransientError("LLM unavailable") from e
```

**Categories:**
- `TransientError` — Retry-able (network, rate limits)
- `PermanentError` — Not retry-able (invalid API key)
- `ConfigurationError` — Misconfigured infrastructure

---

## Infrastructure Components

### Database Layer

**Location:** `jeeves_avionics/database/`

**Responsibilities:**
- Database connection management via `DatabaseClientProtocol`
- Backend registry for swappable implementations
- pgvector extension for embeddings (via `VectorStorageProtocol`)
- Graph storage via `GraphStorageProtocol`
- Session state persistence
- Event log storage
- Transaction management

**Contract:** Implements `DatabaseClientProtocol`, `VectorStorageProtocol`, and `GraphStorageProtocol` from protocols.

**Key Components:**
- `registry.py` — Backend registration and factory
- `postgres_client.py` — PostgreSQL implementation
- `postgres_graph.py` — PostgresGraphAdapter implementing GraphStorageProtocol
- `client.py` — Protocol re-exports and utilities

**Graph Storage (L5):**
```python
from jeeves_avionics.database import PostgresGraphAdapter

# Create adapter with database client
adapter = PostgresGraphAdapter(db_client, logger)
await adapter.ensure_tables()  # Create tables if needed

# Use graph operations
await adapter.add_node("file:main.py", "file", {"path": "main.py"})
await adapter.add_edge("file:main.py", "file:utils.py", "imports")
path = await adapter.find_path("file:main.py", "file:base.py")
```

**Rules:**
- Connection pooling required
- Transactions for multi-step operations
- Schema migrations via Alembic
- No raw SQL outside repositories

### LLM Layer

**Location:** `jeeves_avionics/llm/`

**Responsibilities:**
- Provider abstraction (OpenAI, Anthropic, llamaserver, Azure)
- Token counting and cost tracking
- Rate limiting and retry logic
- Model routing (per-agent overrides)

**Contract:** Implements `LLMProtocol` from core.

**Providers:**
```python
# LLM Provider Factory
def create_llm_provider(provider: str) -> LLMProtocol:
    if provider == "openai":
        return OpenAIAdapter(...)
    elif provider == "anthropic":
        return AnthropicAdapter(...)
    elif provider == "llamaserver":
        return LlamaServerAdapter(...)
    elif provider == "azure":
        return AzureOpenAIAdapter(...)
    else:
        raise ValueError(f"Unknown provider: {provider}")
```

### Memory Layer

**Location:** `jeeves_avionics/memory/`

**Responsibilities:**
- L1: Episodic (per-request transient state)
- L2: Event log (append-only request history)
- L3: Working memory (session-scoped TraversalState)
- L4: Persistent cache (code index, embeddings)

**Contract:** Implements `MemoryServiceProtocol` from core.

**Storage:**
- L1 — In-memory (GenericEnvelope)
- L2 — PostgreSQL (events table)
- L3 — PostgreSQL (sessions table)
- L4 — PostgreSQL + pgvector (embeddings)

### Gateway Layer

**Location:** `jeeves_avionics/gateway/`

**Responsibilities:**
- FastAPI HTTP server
- REST endpoints for chat/governance
- SSE (Server-Sent Events) for streaming
- Static file serving
- Health checks

**Protocol:** HTTP/REST (not a core protocol).

**Endpoints:**
- `POST /api/v1/chat/messages` — Send message
- `GET /api/v1/chat/sessions` — List sessions
- `GET /health` — Liveness probe
- `GET /ready` — Readiness probe

---

## Settings Management

### Settings Hierarchy

```python
# 1. Defaults (code)
settings = Settings()

# 2. .env file
settings = Settings(_env_file=".env")

# 3. Environment variables (highest priority)
# DATABASE_HOST=prod-db python app.py
```

**Precedence:** Environment > .env > Defaults

### Feature Flags

**Location:** `jeeves_avionics/feature_flags.py`

```python
class FeatureFlags(BaseSettings):
    # Infrastructure flags
    use_graph_engine: bool = False
    enable_checkpoints: bool = False
    enable_tracing: bool = False
    enable_distributed_mode: bool = False

    # Memory flags
    memory_event_sourcing_mode: Literal["disabled", "log_only", "log_and_project"]
    memory_semantic_mode: Literal["disabled", "log_only", "log_and_use"]
```

**Use for:**
- Gradual rollouts
- A/B testing
- Environment-specific behavior (prod vs dev)

**Do NOT use for:**
- Permanent code branches (delete flag after rollout)
- Business logic (belongs in Mission System)

---

## Dependency Management

### Avionics Dependencies

**May import:**
- `jeeves_protocols` — Python protocols and contracts
- `jeeves_shared` — Shared utilities (logging, serialization, UUID)
- `jeeves_control_tower` — For ControlTower class instantiation only
- `coreengine` — Core engine (Go implementation, via gRPC)
- External libraries (psycopg2, openai, fastapi, etc.)

**Must NOT import:**
- `jeeves_mission_system` — One-way dependency
- Domain logic from verticals
- `jeeves_control_tower.types` or `jeeves_control_tower.services` for types — Use `jeeves_protocols` instead

**Amendment (2025-12):** Interrupt and rate limit types (InterruptKind, InterruptStatus,
RateLimitConfig, etc.) are defined in `jeeves_protocols.interrupts`. Gateway and middleware
must import these from protocols, not from control_tower, to maintain clean layer separation.

**Example:**

```python
# ALLOWED
from jeeves_protocols import InterruptKind, RateLimitConfig, InterruptServiceProtocol
from jeeves_shared import get_component_logger
from psycopg2.pool import ConnectionPool

# NOT ALLOWED
from jeeves_control_tower.types import InterruptType  # ❌ Use jeeves_protocols
from jeeves_mission_system.verticals.code_analysis import CodeAnalysisAgent  # ❌
```

### External Libraries

**Required dependencies:**
- `fastapi` — HTTP gateway
- `psycopg2-binary` — PostgreSQL client
- `pgvector` — Vector extension
- `openai` — OpenAI API (optional provider)
- `anthropic` — Anthropic API (optional provider)
- `pydantic` — Settings validation
- `structlog` — Structured logging

**Optional dependencies:**
- `redis` — (removed, PostgreSQL only)
- `prometheus-client` — Metrics export
- `opentelemetry` — Distributed tracing

---

## Testing Requirements

### Unit Tests

Mock external systems, test adapter logic:

```python
async def test_openai_adapter():
    """Test LLM adapter without calling actual API."""
    mock_client = MockOpenAI()
    adapter = OpenAIAdapter(client=mock_client)

    result = await adapter.generate("test prompt")
    assert "response" in result
```

### Integration Tests

Use **testcontainers** for real infrastructure:

```python
async def test_database_client():
    """Test database client with real PostgreSQL."""
    async with PostgresContainer() as container:
        client = DatabaseClient(dsn=container.get_connection_url())
        await client.save_session_state(...)
        loaded = await client.load_session_state(...)
        assert loaded == saved
```

**Do NOT:** Call production APIs in tests.

---

## Health and Observability

### Health Checks

**Liveness** (`/health`): Is the service running?
- Returns 200 if process is alive
- No dependency checks

**Readiness** (`/ready`): Can the service accept traffic?
- Database connection OK
- LLM provider reachable (optional)
- Returns 200 if ready, 503 if not

### Structured Logging

Use `structlog` with JSON output:

```python
logger.info(
    "llm_request_complete",
    provider="openai",
    model="gpt-4",
    tokens=1234,
    latency_ms=567,
)
```

**Output:**
```json
{"event": "llm_request_complete", "provider": "openai", "model": "gpt-4", "tokens": 1234, "latency_ms": 567, "timestamp": "2025-12-05T10:30:00Z"}
```

### Metrics

**Key metrics:**
- LLM requests (count, latency, tokens, cost)
- Database queries (count, latency, errors)
- HTTP requests (count, status codes, latency)
- Memory usage (L1-L4 sizes)

**Export:** Prometheus `/metrics` endpoint (optional).

---

## Security Rules

### S1: Never Log Secrets

```python
# BAD
logger.info("api_key", key=settings.openai_api_key)  # ❌

# GOOD
logger.info("api_key_configured", has_key=bool(settings.openai_api_key))  # ✅
```

### S2: Environment Variables Only

API keys **must** come from environment variables:

```python
class Settings(BaseSettings):
    openai_api_key: str = ""  # Default empty, set via OPENAI_API_KEY env var

    class Config:
        env_file = ".env"
        env_file_encoding = "utf-8"
```

**Never** hardcode keys in source code.

### S3: Database Credentials

Use connection strings **without** embedded passwords:

```python
# BAD
dsn = "postgresql://user:password@host/db"  # ❌ Password in logs

# GOOD
dsn = f"postgresql://{user}@{host}/{db}"  # ✅ Password via environment
```

---

## Forbidden Patterns

**Do NOT:**
- Implement agent logic in avionics
- Import from `jeeves_mission_system`
- Modify core protocols (only implement them)
- Store business logic in settings
- Use feature flags for permanent code branches
- Hardcode infrastructure URLs/credentials
- Log sensitive data (API keys, passwords, PII)

---

*This constitution extends the [Architectural Contracts](../docs/CONTRACTS.md) and must not contradict its principles.*
