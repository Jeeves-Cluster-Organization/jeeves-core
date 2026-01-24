# Jeeves Mission System - Application Layer Index

**Parent:** [Capability Contract](../CONTRACT.md)
**Updated:** 2025-12-16

---

## Overview

This directory contains the **application layer** of the Jeeves runtime. It implements orchestration, API endpoints, and provides the framework for capabilities to build upon.

**Position in Architecture:**
```
Capability Layer (external)       →  Domain-specific capabilities (e.g., code analysis)
        ↓
jeeves_mission_system/            →  Application layer (THIS)
        ↓
jeeves_avionics/                  →  Infrastructure (database, LLM, gateway)
        ↓
jeeves_control_tower/             →  Kernel layer (lifecycle, resources, IPC)
        ↓
jeeves_protocols/, jeeves_shared/ →  Foundation (L0)
        ↓
commbus/, coreengine/             →  Go core
```

**Key Principle:** This layer provides application-specific orchestration and the framework for capabilities to build on.

---

## Directory Structure

### Orchestration

| Directory | Description |
|-----------|-------------|
| [orchestrator/](orchestrator/) | Flow orchestration (FlowService, VerticalService, events) |

### Verticals

| Directory | Description |
|-----------|-------------|
| [verticals/](verticals/) | Vertical registry and base classes |

**Note:** Domain-specific agent pipelines belong in capability layers, not here.

### API & Services

| Directory | Description |
|-----------|-------------|
| [api/](api/) | HTTP API endpoints (chat, health, governance) |
| [services/](services/) | ChatService, WorkerCoordinator |
| [contracts/](contracts/) | Contract definitions |
| [prompts/](prompts/) | Core prompts |

### Configuration

| Directory | Description |
|-----------|-------------|
| [bootstrap.py](bootstrap.py) | Composition root, create_app_context() |

### Operations

| Directory | Description |
|-----------|-------------|
| [scripts/](scripts/) | Operational scripts (import boundary checks, etc.) |
| [tests/](tests/) | Test suites (unit, contract, integration) |

---

## Files

| File | Purpose |
|------|---------|
| `__init__.py` | Package exports, version info |

---

## Import Boundary Rules

**Mission System may import:**
- ✅ `jeeves_protocols` - Protocol definitions, types
- ✅ `jeeves_shared` - Shared utilities (logging, serialization, UUID)
- ✅ `jeeves_avionics` - Infrastructure layer
- ✅ `jeeves_control_tower` - Kernel layer

**Mission System must NOT:**
- ❌ Be imported by `coreengine` (core is standalone Go)
- ❌ Be imported by `jeeves_avionics` (one-way dependency)
- ❌ Be imported by `jeeves_control_tower` (one-way dependency)

**Example:**
```python
# ALLOWED
from jeeves_protocols import Envelope, InterruptKind
from jeeves_shared import get_component_logger, parse_datetime
from jeeves_avionics.llm import LLMClient
from jeeves_avionics.database import DatabaseClient

# NOT ALLOWED (these layers cannot import mission system)
# coreengine importing jeeves_mission_system.*
# jeeves_avionics importing jeeves_mission_system.*
# jeeves_control_tower importing jeeves_mission_system.*
```

---

## Deployment

The mission system provides framework primitives for capabilities:

**Mission System Framework:**
- Provides: Stable API via `jeeves_mission_system.api`
- Exports: `MissionRuntime`, `create_mission_runtime()`
- Role: Framework that capabilities build on (NOT an application)

**Capabilities (Applications):**
- Capability entry points are defined in external capability repositories
- Protocol: gRPC on port 50051 (configurable)
- Capabilities import FROM mission system API (constitutional)

**API Gateway:**
- Contains: Minimal web layer (handled by jeeves_avionics/gateway/)
- Entry point: `jeeves_avionics.gateway.main`
- Protocol: HTTP/REST on port 8000

See [Dockerfile](../Dockerfile) and [docker-compose.yml](../docker-compose.yml) for build configuration.

---

## Related

- [coreengine/](../coreengine/) - Core runtime (Go)
- [jeeves_protocols/](../jeeves_protocols/) - Python protocols
- [jeeves_shared/](../jeeves_shared/) - Shared utilities (logging, serialization, UUID)
- [jeeves_avionics/](../jeeves_avionics/) - Infrastructure layer
- [jeeves_control_tower/](../jeeves_control_tower/) - Control Tower (kernel layer)
- [bootstrap.py](bootstrap.py) - Application bootstrap

---

*This directory represents the application layer in the three-component split (Amendment XXII).*
