# Jeeves Memory Module Constitution

**Updated:** 2026-01-06

---

## Overview

The Jeeves Memory Module provides memory services (L1-L7) for the Jeeves system. It implements persistence, semantic search, event sourcing, and tool health governance.

---

## Constitutional Hierarchy

```
protocols/         ← Foundation (protocols, memory types)
           ↑
shared/            ← Shared utilities (logging, serialization)
           ↑
memory_module/     ← THIS (memory services, implementations)
           ↑
avionics/          ← Infrastructure (database factory only)
           ↑
mission_system/    ← Application layer
```

---

## Core Principles

### P1: Memory Types in protocols

All memory types are defined in `protocols/memory.py`:
- `WorkingMemory` - Session-level cognitive state
- `FocusState` / `FocusType` - Focus tracking
- `EntityRef` - Entity reference tracking
- `MemoryItem` - Generic memory item
- `Finding` - Exploration finding

### P2: Memory Protocols in protocols

Memory protocols are defined in `protocols/protocols.py`:
- `MemoryServiceProtocol` - CRUD + search operations
- `SemanticSearchProtocol` - Embedding-based search
- `SessionStateProtocol` - Session state management

### P3: Single Ownership

This module **owns** all memory-specific implementations:
- Repositories (persistence layer)
- Services (business logic)
- Adapters (protocol implementations)
- Manager (facade)

### P4: CommBus Communication

Memory operations **should** publish events via CommBus for observability:
- `MemoryStored` - When items are persisted
- `MemoryRetrieved` - When searches complete
- `SessionStateChanged` - When session state mutates

---

## Module Structure

```
memory_module/
├── CONSTITUTION.md          # This file
├── __init__.py              # Public exports
├── manager.py               # MemoryManager facade
├── intent_classifier.py     # Intent classification
├── adapters/                # Protocol implementations
│   ├── __init__.py
│   └── sql_adapter.py       # SQLAdapter
├── services/                # Memory services
│   ├── __init__.py
│   ├── session_state_adapter.py  # SessionStateProtocol impl
│   ├── session_state_service.py
│   ├── chunk_service.py     # L3 semantic chunking
│   ├── embedding_service.py # Embedding generation
│   ├── event_emitter.py     # Event emission (CommBus wired)
│   ├── trace_recorder.py
│   ├── tool_health_service.py
│   ├── nli_service.py
│   ├── code_indexer.py
│   └── xref_manager.py
├── repositories/            # Persistence layer
│   ├── __init__.py
│   ├── chunk_repository.py  # L3 semantic chunks
│   ├── event_repository.py  # L2 event log
│   ├── graph_stub.py        # L5 in-memory stub (GraphStorageProtocol)
│   ├── skill_stub.py        # L6 in-memory stub (SkillStorageProtocol)
│   ├── session_state_repository.py  # L4 working memory
│   ├── trace_repository.py
│   ├── pgvector_repository.py
│   └── tool_metrics_repository.py
├── messages/                # CommBus messages
│   ├── __init__.py
│   ├── events.py            # MemoryStored, etc.
│   ├── queries.py           # GetSessionState, etc.
│   └── commands.py          # ClearSession, etc.
└── tests/                   # Memory tests
    └── unit/
```

---

## Memory Layers (L1-L7)

| Layer | Name | Scope | Implementation |
|-------|------|-------|----------------|
| **L1** | Episodic | Per-request | `Envelope` (core_engine) |
| **L2** | Events | Append-only log | `EventRepository` + `EventEmitter` (CommBus wired) |
| **L3** | Semantic | Embeddings/search | `ChunkService` + `PgVectorRepository` |
| **L4** | Working | Per-session state | `SessionStateService` |
| **L5** | Graph | Entity relationships | `GraphStorageProtocol` + `InMemoryGraphStorage` (stub) |
| **L6** | Skills | Learned patterns | `SkillStorageProtocol` + `InMemorySkillStorage` (stub) |
| **L7** | Meta | Tool metrics | `ToolHealthService` |

### L5-L6 Extension Points

L5 (Graph) and L6 (Skills) use protocol-first design for extensibility:

```python
from protocols import GraphStorageProtocol, SkillStorageProtocol
from memory_module.repositories import InMemoryGraphStorage, InMemorySkillStorage

# Development/Testing: Use in-memory stubs
graph = InMemoryGraphStorage()
skills = InMemorySkillStorage()

# Production: Use PostgresGraphAdapter from avionics (L3)
from avionics.database import PostgresGraphAdapter
graph = PostgresGraphAdapter(db_client)
await graph.ensure_tables()
```

**Architecture Note**: PostgreSQL-specific implementations live in `avionics`
(L3 infrastructure layer), not in the memory module. This follows Dependency
Inversion: core depends on abstractions (protocols), infrastructure provides
concrete implementations.

---

## Dependency Rules

### ALLOWED Dependencies

```
memory_module → protocols (DatabaseClientProtocol, LoggerProtocol, memory types)
memory_module → shared (logging, serialization, UUID utilities)
memory_module → avionics.database.factory (create_database_client ONLY)
```

Import pattern:
```python
from protocols import WorkingMemory, Finding, DatabaseClientProtocol
from shared import create_logger, to_json, uuid_str
from avionics.database.factory import create_database_client
```

### FORBIDDEN Dependencies

```
memory_module ✗→ avionics.database.postgres_client (use factory)
memory_module ✗→ avionics.llm/* (use LLMProviderProtocol)
memory_module ✗→ mission_system (circular)
memory_module ✗→ jeeves-capability-* (circular)
memory_module ✗→ control_tower (circular)
```

