# Memory Abstraction Analysis

**Date:** 2026-01-19  
**Branch:** cursor/memory-abstraction-layering-bed9

---

## Executive Summary

This document identifies every "memory" abstraction in the codebase and analyzes each for persistence type, schema, lifecycle, and concurrency assumptions. **Verdict: Memory IS actually layered (L1-L7), not just naming.**

---

## Memory Abstractions Inventory

### 1. Protocol/Type Layer (jeeves_protocols)

| Abstraction | File | Persistence | Purpose |
|------------|------|-------------|---------|
| `WorkingMemory` | `memory.py` | In-memory (dataclass) | Session-level cognitive state |
| `FocusState` | `memory.py` | In-memory (dataclass) | Current focus context |
| `EntityRef` | `memory.py` | In-memory (dataclass) | Entity reference tracking |
| `MemoryItem` | `memory.py` | In-memory (dataclass) | Generic memory item |
| `Finding` | `memory.py` | In-memory (dataclass) | Analysis finding |
| `MemoryServiceProtocol` | `protocols.py` | Protocol (interface) | CRUD + search interface |
| `SemanticSearchProtocol` | `protocols.py` | Protocol (interface) | Embedding-based search |
| `SessionStateProtocol` | `protocols.py` | Protocol (interface) | Session state management |
| `GraphStorageProtocol` | `protocols.py` | Protocol (interface) | L5 entity graph |
| `SkillStorageProtocol` | `protocols.py` | Protocol (interface) | L6 learned patterns |

### 2. Repository Layer (jeeves_memory_module/repositories)

| Abstraction | Persistence | Schema | Lifecycle | Concurrency |
|------------|-------------|--------|-----------|-------------|
| `EventRepository` / `DomainEvent` | PostgreSQL | Append-only event log | Immutable (per M2) | DB-level transactions |
| `SessionStateRepository` / `SessionState` | PostgreSQL | `session_state` table | Session-scoped, upsert | DB-level row locking |
| `ChunkRepository` / `Chunk` | PostgreSQL | `chunks` table + embeddings | Soft-delete | DB-level |
| `PgVectorRepository` | PostgreSQL (pgvector) | Embedding columns on tables | Per-item | DB-level |
| `InMemoryGraphStorage` | In-memory (dict) | Nodes + edges in dicts | Process lifetime | asyncio.Lock |
| `InMemorySkillStorage` | In-memory (dict) | Skills in dict | Process lifetime | None (single-threaded) |
| `TraceRepository` | PostgreSQL | `agent_traces` table | Append-only | DB-level |
| `ToolMetricsRepository` | PostgreSQL | Metrics tables | Sliding window | DB-level |

### 3. Service Layer (jeeves_memory_module/services)

| Service | Function | Underlying Persistence |
|---------|----------|----------------------|
| `SessionStateService` | L4 working memory management | SessionStateRepository (PostgreSQL) |
| `ChunkService` | L3 semantic chunking + search | ChunkRepository (PostgreSQL) |
| `EmbeddingService` | Embedding generation | External model + caching |
| `EventEmitter` | L2 event emission + dedup | EventRepository (PostgreSQL) |
| `TraceRecorder` | Agent decision tracing | TraceRepository (PostgreSQL) |
| `ToolHealthService` | L7 tool metrics governance | ToolMetricsRepository (PostgreSQL) |

### 4. Adapter Layer (jeeves_memory_module/adapters)

| Adapter | Purpose | Persistence |
|---------|---------|-------------|
| `SQLAdapter` | Unified SQL interface for facts/messages | PostgreSQL |

### 5. Manager/Facade Layer

| Manager | Purpose | Persistence |
|---------|---------|-------------|
| `MemoryManager` | Unified memory management (routes writes to SQL + vector) | PostgreSQL + pgvector |

### 6. Caching Abstractions

| Cache | Persistence | Scope | Lifecycle |
|-------|-------------|-------|-----------|
| `SessionDedupCache` | In-memory (dict of sets) | Per-session | Process lifetime |
| `InMemoryStateBackend` | In-memory (dicts) | Global | Process lifetime |
| `RedisStateBackend` | Redis | Distributed | TTL-based |

### 7. Communication/Bus Abstractions

| Abstraction | Persistence | Scope |
|-------------|-------------|-------|
| `InMemoryCommBus` | In-memory | Process lifetime |

---

## Detailed Analysis by Layer (L1-L7)

### L1: Episodic Memory
- **Implementation**: `GenericEnvelope` (coreengine - Go)
- **Persistence**: Per-request in-memory, optionally checkpoint to DB
- **Schema**: Envelope struct with stages, routing, context
- **Lifecycle**: Request-scoped
- **Concurrency**: Go channels + mutex

### L2: Event Log
- **Implementation**: `EventRepository` + `EventEmitter`
- **Persistence**: PostgreSQL (`domain_events` table)
- **Schema**: 
  ```sql
  event_id UUID, occurred_at TIMESTAMP, aggregate_type TEXT,
  aggregate_id TEXT, event_type TEXT, schema_version INT,
  payload JSONB, causation_id UUID, correlation_id UUID,
  idempotency_key TEXT, user_id TEXT, actor TEXT
  ```
- **Lifecycle**: Append-only, immutable (per Memory Contract M2)
- **Concurrency**: DB transactions + idempotency key for deduplication

### L3: Semantic Memory
- **Implementation**: `ChunkService` + `ChunkRepository` + `PgVectorRepository`
- **Persistence**: PostgreSQL + pgvector extension
- **Schema**:
  ```sql
  chunk_id TEXT, user_id TEXT, source_type TEXT, source_id TEXT,
  content TEXT, embedding VECTOR(384), metadata JSONB,
  chunk_index INT, total_chunks INT, created_at TIMESTAMP,
  deleted_at TIMESTAMP
  ```
- **Lifecycle**: Soft-delete (deleted_at timestamp)
- **Concurrency**: DB-level row locking

### L4: Working Memory
- **Implementation**: `SessionStateService` + `SessionStateRepository`
- **Persistence**: PostgreSQL (`session_state` table)
- **Schema**:
  ```sql
  session_id UUID PRIMARY KEY, user_id TEXT, focus_type TEXT,
  focus_id TEXT, focus_context TEXT (JSON), 
  referenced_entities TEXT (JSON), short_term_memory TEXT,
  turn_count INT, created_at TIMESTAMP, updated_at TIMESTAMP
  ```
- **Lifecycle**: Session-scoped with upsert semantics
- **Concurrency**: DB-level row locking via upsert

### L5: Entity Graph
- **Implementation**: `InMemoryGraphStorage` (stub implementing `GraphStorageProtocol`)
- **Persistence**: **In-memory only** (stub for development)
- **Schema**: 
  ```python
  _nodes: Dict[str, GraphNode]  # node_id -> node
  _edges: Dict[Tuple[str, str, str], GraphEdge]  # (source, target, type) -> edge
  _outgoing: Dict[str, Set[Tuple[str, str]]]  # node_id -> [(target, type)]
  _incoming: Dict[str, Set[Tuple[str, str]]]  # node_id -> [(source, type)]
  ```
- **Lifecycle**: Process lifetime (lost on restart)
- **Concurrency**: No explicit locking (async methods but single-threaded)
- **Note**: "Production implementations should use a proper graph database (Neo4j, etc.) or PostgreSQL with recursive CTEs."

### L6: Skills/Patterns
- **Implementation**: `InMemorySkillStorage` (stub implementing `SkillStorageProtocol`)
- **Persistence**: **In-memory only** (stub for development)
- **Schema**:
  ```python
  _skills: Dict[str, Skill]  # skill_id -> skill
  _usage_history: List[SkillUsage]  # usage records
  ```
- **Lifecycle**: Process lifetime (lost on restart)
- **Concurrency**: No explicit locking
- **Note**: "Not yet implemented" per CONSTITUTION

### L7: Meta/Tool Metrics
- **Implementation**: `ToolHealthService` + `ToolMetricsRepository`
- **Persistence**: PostgreSQL
- **Schema**: Tool call metrics, error rates, quarantine status
- **Lifecycle**: Sliding window for metrics
- **Concurrency**: DB-level

---

## Concurrency Assumptions Matrix

| Abstraction | Thread Safety | Async Safety | Distributed Safety |
|-------------|---------------|--------------|-------------------|
| WorkingMemory | Mutable dataclass - NOT safe | N/A | None |
| EventRepository | DB transactions | Yes | Yes (DB) |
| SessionStateRepository | DB upsert | Yes | Yes (DB) |
| ChunkRepository | DB operations | Yes | Yes (DB) |
| InMemoryGraphStorage | No locks | Assumed single-threaded | **NOT distributed** |
| InMemorySkillStorage | No locks | Assumed single-threaded | **NOT distributed** |
| SessionDedupCache | Dict operations | Assumed single-threaded | **NOT distributed** |
| InMemoryCommBus | asyncio.Lock | Yes | **NOT distributed** |
| InMemoryStateBackend | Dict operations | Yes | **NOT distributed** |

---

## Is Memory Actually Layered?

### Verdict: **YES**, memory is genuinely layered.

### Evidence:

1. **Distinct Persistence per Layer**:
   - L2 Events: Append-only PostgreSQL
   - L3 Semantic: PostgreSQL + pgvector embeddings
   - L4 Working: PostgreSQL with upsert
   - L5/L6: In-memory stubs (awaiting production implementation)

2. **Different Lifecycle Semantics**:
   - L2: Immutable forever
   - L3: Soft-delete with cleanup
   - L4: Session-scoped with active management
   - L5/L6: Process lifetime (transient)

3. **Separation of Concerns**:
   - Each layer has its own repository pattern
   - Services compose repositories for higher-level operations
   - Clear protocol boundaries via `jeeves_protocols`

4. **Constitutional Documentation**:
   - `jeeves_memory_module/CONSTITUTION.md` explicitly defines L1-L7
   - Each layer has specific contracts and responsibilities

### However, Some Gaps Exist:

1. **L5/L6 are stubs**: Graph storage and Skills are in-memory only, not production-ready
2. **L1 is in Go**: Episodic memory (`GenericEnvelope`) lives in a different language/module
3. **Concurrency gaps**: In-memory components lack distributed safety
4. **No unified query across layers**: Each layer is queried separately

---

## Recommendations

1. **L5 Graph Storage**: Implement PostgreSQL adapter using recursive CTEs or integrate Neo4j
2. **L6 Skills Storage**: Move to PostgreSQL for persistence
3. **Distributed Safety**: Add Redis-backed alternatives for dedup cache and CommBus for multi-process deployments
4. **Cross-Layer Query**: Consider a unified `MemoryQuery` that can search across L3, L4, L5

---

## File Reference Map

```
jeeves_protocols/
├── memory.py                     # WorkingMemory, FocusState, EntityRef, MemoryItem, Finding
└── protocols.py                  # *Protocol definitions for memory services

jeeves_memory_module/
├── CONSTITUTION.md               # L1-L7 layer definitions
├── manager.py                    # MemoryManager (facade)
├── adapters/
│   └── sql_adapter.py            # SQLAdapter
├── repositories/
│   ├── event_repository.py       # L2 - DomainEvent
│   ├── session_state_repository.py  # L4 - SessionState
│   ├── chunk_repository.py       # L3 - Chunk
│   ├── pgvector_repository.py    # L3 - Vector search
│   ├── graph_stub.py             # L5 - InMemoryGraphStorage
│   ├── skill_stub.py             # L6 - InMemorySkillStorage
│   ├── trace_repository.py       # Agent traces
│   └── tool_metrics_repository.py  # L7 - Tool metrics
├── services/
│   ├── session_state_service.py  # L4 service
│   ├── chunk_service.py          # L3 service
│   ├── event_emitter.py          # L2 service + SessionDedupCache
│   ├── trace_recorder.py         # Trace service
│   └── tool_health_service.py    # L7 service
└── messages/
    └── events.py                 # CommBus event types

jeeves_avionics/
└── database/
    └── connection_manager.py     # StateBackend, InMemoryStateBackend, RedisStateBackend

jeeves_control_tower/
└── ipc/
    └── commbus.py                # InMemoryCommBus
```
