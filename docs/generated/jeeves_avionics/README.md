# jeeves_avionics - Infrastructure Layer (L3)

The `jeeves_avionics` package provides infrastructure services for the Jeeves runtime. This is **Layer 3** (L3) in the system architecture - the infrastructure layer that provides LLM, database, and gateway services.

## Constitutional Reference

- **Avionics R1**: Adapter Pattern - all external services wrapped in adapters
- **Avionics R2**: Configuration Over Code - all behavior is configurable
- **Avionics R3**: No Domain Logic - infrastructure provides transport, not business logic
- **Avionics R4**: Swappable Implementations - all providers implement protocols

## Package Overview

```
jeeves_avionics/
├── __init__.py              # Package entry point
├── settings.py              # Infrastructure settings (pydantic-settings)
├── feature_flags.py         # Runtime toggles for phased rollout
├── capability_registry.py   # Per-capability LLM configuration registry
├── thresholds.py            # Centralized confidence/operational thresholds
├── wiring.py                # Tool execution and LLM provider factories
├── context.py               # AppContext for dependency injection
├── context_bounds.py        # Context window limits
│
├── gateway/                 # HTTP API layer (FastAPI)
│   ├── main.py              # FastAPI application
│   ├── websocket.py         # WebSocket client management and event subscription
│   ├── sse.py               # Server-Sent Events utilities
│   ├── event_bus.py         # Internal pub/sub for gateway
│   ├── grpc_client.py       # gRPC client to orchestrator
│   └── routers/             # API routers
│       ├── chat.py          # Chat endpoints
│       ├── health.py    # System health endpoints
│       └── interrupts.py    # Unified interrupt handling
│
├── database/                # Database infrastructure
│   ├── client.py            # Protocol re-exports
│   ├── connection_manager.py # Deployment-mode-aware backend selection
│   ├── factory.py           # Database client factory
│   ├── postgres_client.py   # PostgreSQL with pgvector
│   └── redis_client.py      # Redis for distributed state
│
├── llm/                     # LLM provider abstraction
│   ├── factory.py           # Provider factory
│   ├── gateway.py           # Unified LLM gateway with cost tracking
│   ├── cost_calculator.py   # Token cost calculation
│   └── providers/           # Provider implementations
│       ├── base.py          # Abstract base class
│       ├── openai.py        # OpenAI provider
│       ├── anthropic.py     # Anthropic provider
│       ├── azure.py         # Azure OpenAI provider
│       └── llamaserver_provider.py  # llama.cpp server
│
├── logging/                 # Logging infrastructure
│   ├── __init__.py          # Configuration entry point
│   ├── adapter.py           # StructlogAdapter for DI
│   └── context.py           # Context propagation
│
├── middleware/              # API middleware
│   └── rate_limit.py        # Rate limiting middleware
│
├── observability/           # OpenTelemetry integration
│   ├── metrics.py           # Prometheus metrics
│   ├── otel_adapter.py      # OpenTelemetry adapter
│   └── tracing_middleware.py # Tracing decorators
│
├── tools/                   # Tool infrastructure
│   ├── catalog.py           # Canonical tool catalog
│   └── executor.py     # Pure execution logic
│
├── checkpoint/              # State checkpointing
│   └── postgres_adapter.py  # PostgreSQL checkpoint storage
│
├── distributed/             # Horizontal scaling
│   └── redis_bus.py         # Redis distributed task bus
│
├── interop/                 # Cross-language bridge
│   └── go_bridge.py         # Python-Go interoperability
│
└── webhooks/                # Event delivery
    └── service.py           # Webhook subscription and delivery
```

## Module Documentation

| Module | Description | Documentation |
|--------|-------------|---------------|
| Settings & Config | Settings, feature flags, thresholds, capability registry | [settings.md](./settings.md) |
| Gateway | HTTP API, WebSocket, SSE, routers | [gateway.md](./gateway.md) |
| Database | PostgreSQL, Redis, connection management | [database.md](./database.md) |
| LLM | Provider abstraction, cost tracking, gateway | [llm.md](./llm.md) |
| Observability | Logging, metrics, OpenTelemetry | [observability.md](./observability.md) |
| Tools | Tool catalog, executor core | [tools.md](./tools.md) |
| Infrastructure | Checkpoints, distributed bus, Go bridge, webhooks | [infrastructure.md](./infrastructure.md) |

## Import Guidelines

### From Other Layers

Other layers should import from `jeeves_avionics` for:

```python
# Settings and configuration
from jeeves_avionics.settings import get_settings, Settings
from jeeves_avionics.feature_flags import get_feature_flags, FeatureFlags
from jeeves_avionics.thresholds import PLAN_MIN_CONFIDENCE

# Logging
from jeeves_avionics.logging import create_logger, configure_logging

# Database
from jeeves_avionics.database import create_database_client

# LLM
from jeeves_avionics.llm import create_llm_provider, LLMGateway

# Tools
from jeeves_avionics.tools import ToolCatalog, ToolId

# Context
from jeeves_avionics.context import AppContext
```

### Internal Imports

Within avionics, prefer direct imports:

```python
# Within jeeves_avionics/gateway/routers/chat.py
from jeeves_avionics.gateway.grpc_client import get_grpc_client
from jeeves_avionics.logging import get_current_logger
```

## Key Concepts

### 1. Protocol-Based Design

All implementations follow protocols from `jeeves_protocols`:

- `DatabaseClientProtocol` - Database operations
- `LLMProviderProtocol` - LLM generation
- `LoggerProtocol` - Structured logging
- `ToolRegistryProtocol` - Tool registration

### 2. Dependency Injection

Components receive dependencies via constructors (ADR-001):

```python
class MyService:
    def __init__(self, logger: LoggerProtocol, settings: Settings):
        self._logger = logger
        self._settings = settings
```

### 3. Lazy Initialization

Global singletons use lazy initialization:

```python
_settings: Optional[Settings] = None

def get_settings() -> Settings:
    global _settings
    if _settings is None:
        _settings = Settings()
    return _settings
```

### 4. Feature Flags

New features are gated by feature flags for phased rollout:

```python
flags = get_feature_flags()
if flags.use_llm_gateway:
    gateway = LLMGateway(settings)
```

## Constitutional Compliance

| Principle | Implementation |
|-----------|----------------|
| P1 (Accuracy First) | Single source of truth for tools, settings |
| P3 (Bounded Efficiency) | Context bounds, rate limiting |
| P5 (Deterministic Spine) | Typed enums, protocol-based interfaces |
| P6 (Observable) | Structured logging, OpenTelemetry, metrics |
