# System Determinism Assessment

**Purpose**: Identify all sources of non-determinism (randomness, time, network) and assess whether they are properly injected/controlled for reproducible runs.

**Status**: Assessment completed January 2026

---

## Executive Summary

The codebase has **solid foundational infrastructure** for determinism:
- `ClockProtocol` and `SystemClock` for time abstraction
- `IdGeneratorProtocol` with `DeterministicIdGenerator` for reproducible IDs
- `AppContext` dependency injection container
- Mock LLM provider for testing

However, **many call sites bypass this infrastructure** by calling `datetime.now()`, `uuid4()`, and `time.Now()` directly. This assessment catalogs all non-deterministic sources and recommends minimal changes.

---

## Non-Deterministic Sources Inventory

### 1. UUID/ID Generation

#### Status: PARTIALLY CONTROLLED

**Infrastructure Available:**
- `IdGeneratorProtocol` in `jeeves_protocols/protocols.py` (lines 414-419)
- `UUIDGenerator` (production) in `jeeves_shared/id_generator.py`
- `DeterministicIdGenerator` (testing) in `jeeves_shared/id_generator.py`
- `SequentialIdGenerator` (debugging) in `jeeves_shared/id_generator.py`

**Problematic Direct Calls (bypass injection):**

| Location | Call Pattern | Risk Level |
|----------|--------------|------------|
| `jeeves_protocols/events.py:64,182` | `uuid.uuid4()` | HIGH - Core event system |
| `jeeves_protocols/agents.py:576-577` | `uuid.uuid4()` | HIGH - Agent creation |
| `jeeves_mission_system/services/chat_service.py:107` | `uuid4()` | MEDIUM |
| `jeeves_mission_system/services/worker_coordinator.py:146,183,436` | `uuid.uuid4()` | MEDIUM |
| `jeeves_mission_system/services/debug_api.py:239` | `uuid.uuid4()` | LOW - Debug only |
| `jeeves_memory_module/services/event_emitter.py:848` | `uuid4()` | MEDIUM |
| `jeeves_memory_module/services/xref_manager.py:56` | `uuid4()` | MEDIUM |
| `jeeves_memory_module/repositories/*.py` | `uuid4()` defaults | MEDIUM |
| `jeeves_avionics/gateway/event_bus.py:137` | `uuid.uuid4()` | MEDIUM |
| `jeeves_avionics/checkpoint/postgres_adapter.py:308` | `uuid.uuid4()` | MEDIUM |
| `jeeves_control_tower/services/interrupt_service.py:243` | `uuid.uuid4()` | MEDIUM |
| `coreengine/envelope/generic.go:190-193` | `uuid.New()` | HIGH - Core Go engine |

**Test Files (acceptable - ~40 files):** Tests using `uuid4()` directly is acceptable as test isolation typically requires unique IDs.

### 2. Time/Clock Operations

#### Status: PARTIALLY CONTROLLED

**Infrastructure Available:**
- `ClockProtocol` in `jeeves_protocols/protocols.py` (lines 248-252)
- `SystemClock` in `jeeves_avionics/context.py` (lines 45-58)
- `utc_now()` helper in `jeeves_protocols/utils.py` (lines 152-161)

**Problematic Direct Calls:**

| Location | Call Pattern | Risk Level |
|----------|--------------|------------|
| `jeeves_protocols/events.py:67-68,180-186` | `datetime.now(timezone.utc)` | HIGH - Event timestamps |
| `jeeves_protocols/interrupts.py:58,105,185` | `datetime.now(timezone.utc)` | HIGH - Interrupt tracking |
| `jeeves_shared/id_generator.py:169,181` | `datetime.utcnow()` | MEDIUM - Timestamp IDs |
| `jeeves_shared/serialization.py:141` | `datetime.now(timezone.utc)` | MEDIUM |
| `jeeves_mission_system/orchestrator/flow_service.py:202,206,256,348` | `datetime.now()` | HIGH |
| `jeeves_mission_system/orchestrator/agent_events.py:68` | `time.time()` | HIGH |
| `jeeves_mission_system/services/chat_service.py:108,338,363,etc.` | `datetime.now()` | HIGH |
| `jeeves_mission_system/api/health.py:78,100,148,etc.` | `datetime.now()` | LOW - Health checks |
| `jeeves_memory_module/services/tool_health_service.py` | `datetime.now()` (~10 calls) | MEDIUM |
| `jeeves_memory_module/repositories/*.py` | `datetime.now()` defaults | MEDIUM |
| `jeeves_avionics/distributed/redis_bus.py` | `datetime.now()`, `time.time()` (~15 calls) | HIGH |
| `jeeves_avionics/llm/gateway.py` | `time.time()` (latency tracking) | LOW |
| `jeeves_control_tower/services/interrupt_service.py:231,442,553` | `datetime.now()` | HIGH |
| `jeeves_control_tower/resources/rate_limiter.py:261,344,404` | `time.time()` | HIGH |
| `jeeves_control_tower/ipc/commbus.py:87` | `datetime.now()` default | MEDIUM |
| `coreengine/envelope/generic.go:80,188,345,etc.` | `time.Now()` | HIGH |
| `coreengine/agents/unified.go:104,258,388` | `time.Now()` | HIGH |
| `coreengine/runtime/runtime.go:149` | `time.Now()` | HIGH |
| `coreengine/grpc/server.go:185,199,219` | `time.Now()` | HIGH |
| `commbus/middleware.go:94,222,251` | `time.Now()` | MEDIUM |

### 3. Network/External I/O

#### Status: ABSTRACTED VIA PROTOCOLS

**Well-Controlled:**
- `LLMProviderProtocol` with `MockProvider` for testing
- `PersistenceProtocol` for database abstraction
- `DistributedBusProtocol` for message bus abstraction
- `CheckpointProtocol` for checkpoint storage

**Direct Network Calls (in production code):**

| Location | Technology | Controlled? |
|----------|------------|-------------|
| `jeeves_avionics/llm/providers/llamaserver_provider.py` | `httpx.AsyncClient` | YES - via protocol |
| `jeeves_avionics/webhooks/service.py:554` | `aiohttp.ClientSession` | YES - abstracted |
| `jeeves_mission_system/api/health.py:219` | `aiohttp.ClientSession` | NO - direct call |
| `jeeves_avionics/database/redis_client.py` | Redis | YES - via protocol |
| `jeeves_avionics/database/postgres_*.py` | PostgreSQL | YES - via protocol |

### 4. Environment Variables

#### Status: CONTROLLED VIA COMPOSITION ROOT

**Well-Controlled:**
- `bootstrap.py` centralizes environment variable reading
- `Settings` and `FeatureFlags` provide abstraction
- `AppContext` propagates configuration

**Direct `os.getenv`/`os.environ` calls (~60 locations):**
- Most are in `bootstrap.py`, test fixtures, or configuration modules (acceptable)
- Some scattered in runtime code (should be injected)

### 5. Hashing (Deterministic)

#### Status: DETERMINISTIC - NO CHANGES NEEDED

All hash operations use deterministic algorithms:
- `hashlib.sha256` for content hashing
- `hashlib.md5` for rate limit keys
- No random salts or seeds observed

### 6. Async Sleep/Timing

#### Status: OBSERVABLE BUT NOT CONTROLLABLE

**Locations using sleep:**
- `jeeves_mission_system/services/worker_coordinator.py:334,499` - Backoff/heartbeat
- `jeeves_avionics/llm/providers/llamaserver_provider.py:211,223,237` - Retry backoff
- `jeeves_avionics/gateway/sse.py:79` - Heartbeat interval
- `jeeves_avionics/webhooks/service.py:487` - Retry delay

These are timing-related but typically acceptable as they affect only performance, not correctness.

---

## Recommendations for Reproducible Runs

### Priority 1: High-Impact Changes (Minimal Effort)

#### 1.1 Add Clock to UnifiedEvent.create_now()

**File:** `jeeves_protocols/events.py`

```python
# Current (non-deterministic):
@classmethod
def create_now(cls, ...) -> "UnifiedEvent":
    now = datetime.now(timezone.utc)
    return cls(
        event_id=str(uuid.uuid4()),
        ...
    )

# Recommended (deterministic):
@classmethod
def create_now(
    cls,
    ...,
    clock: Optional[ClockProtocol] = None,
    id_generator: Optional[IdGeneratorProtocol] = None,
) -> "UnifiedEvent":
    _clock = clock or _default_clock()
    _id_gen = id_generator or _default_id_generator()
    now = _clock.utcnow()
    return cls(
        event_id=_id_gen.generate(),
        ...
    )
```

#### 1.2 Add Clock/IdGenerator to FlowInterrupt creation (Go)

**File:** `coreengine/envelope/generic.go`

Create a `Clock` interface:
```go
type Clock interface {
    Now() time.Time
}

type SystemClock struct{}
func (SystemClock) Now() time.Time { return time.Now().UTC() }
```

Pass clock to envelope creation functions.

#### 1.3 Propagate Clock through AppContext

The `AppContext` already has a `clock` field. Ensure all services that need time access it from context rather than calling `datetime.now()` directly.

### Priority 2: Medium-Impact Changes

#### 2.1 Create TestClock and TestIdGenerator

**File:** `jeeves_shared/testing.py` (new additions)

```python
from datetime import datetime, timezone, timedelta
from jeeves_protocols import ClockProtocol, IdGeneratorProtocol

class FrozenClock:
    """Clock frozen at a specific time for testing."""
    
    def __init__(self, frozen_at: Optional[datetime] = None):
        self._time = frozen_at or datetime(2025, 1, 1, 12, 0, 0, tzinfo=timezone.utc)
    
    def now(self) -> datetime:
        return self._time.replace(tzinfo=None)
    
    def utcnow(self) -> datetime:
        return self._time
    
    def advance(self, delta: timedelta) -> None:
        self._time += delta

class SteppingClock:
    """Clock that advances by fixed amount on each call."""
    
    def __init__(self, start: datetime, step: timedelta = timedelta(seconds=1)):
        self._time = start
        self._step = step
    
    def now(self) -> datetime:
        result = self._time
        self._time += self._step
        return result.replace(tzinfo=None)
    
    def utcnow(self) -> datetime:
        result = self._time
        self._time += self._step
        return result
```

#### 2.2 Update Repository Default Factories

**Files:** `jeeves_memory_module/repositories/*.py`

Change from:
```python
self.created_at = created_at or datetime.now(timezone.utc)
```

To:
```python
# Accept clock as constructor parameter
self.created_at = created_at  # Caller must provide
```

Or use a context-aware factory pattern.

### Priority 3: Comprehensive Changes (If Full Reproducibility Needed)

#### 3.1 Go Core Engine Clock Abstraction

Create interfaces in `coreengine/`:
```go
// clock.go
package coreengine

import "time"

type Clock interface {
    Now() time.Time
    Since(t time.Time) time.Duration
}

type IDGenerator interface {
    Generate() string
    GeneratePrefixed(prefix string) string
}
```

#### 3.2 Request-Scoped Determinism Context

For full reproducibility, create a `DeterminismContext`:

```python
@dataclass
class DeterminismContext:
    """Context for reproducible execution."""
    clock: ClockProtocol
    id_generator: IdGeneratorProtocol
    random_seed: Optional[int] = None
    
    @classmethod
    def for_testing(cls, seed: str = "test") -> "DeterminismContext":
        return cls(
            clock=FrozenClock(),
            id_generator=DeterministicIdGenerator(seed),
            random_seed=hash(seed),
        )
    
    @classmethod
    def for_production(cls) -> "DeterminismContext":
        return cls(
            clock=SystemClock(),
            id_generator=UUIDGenerator(),
            random_seed=None,
        )
```

---

## Migration Path

### Phase 1: Non-Breaking Changes (Week 1)
1. Add optional `clock` and `id_generator` parameters to key factory methods
2. Default to `SystemClock()` and `UUIDGenerator()` for backward compatibility
3. Update test fixtures to use `FrozenClock` and `DeterministicIdGenerator`

### Phase 2: Service Updates (Week 2)
1. Update services to accept clock/id_generator via `AppContext`
2. Replace direct `datetime.now()` calls in high-traffic code paths
3. Add lint rule to flag direct time/UUID calls

### Phase 3: Go Core Engine (Week 3)
1. Define Clock and IDGenerator interfaces
2. Update envelope creation functions
3. Pass clock through runtime context

### Phase 4: Verification
1. Create reproducibility test suite
2. Run same request with same seed, verify identical outputs
3. Document remaining acceptable non-determinism (e.g., network latency)

---

## Files Requiring Updates (Prioritized)

### Critical (Core Event/Envelope System)
- [ ] `jeeves_protocols/events.py` - UnifiedEvent.create_now()
- [ ] `jeeves_protocols/interrupts.py` - Interrupt creation
- [ ] `coreengine/envelope/generic.go` - NewGenericEnvelope()

### High Priority (Orchestration)
- [ ] `jeeves_mission_system/orchestrator/flow_service.py`
- [ ] `jeeves_mission_system/orchestrator/agent_events.py`
- [ ] `jeeves_mission_system/services/chat_service.py`

### Medium Priority (Infrastructure)
- [ ] `jeeves_control_tower/services/interrupt_service.py`
- [ ] `jeeves_control_tower/resources/rate_limiter.py`
- [ ] `jeeves_avionics/distributed/redis_bus.py`
- [ ] `jeeves_memory_module/repositories/*.py`

### Low Priority (Already Well-Abstracted)
- [ ] Health check endpoints (timing is informational)
- [ ] Debug/replay APIs (inherently non-deterministic use case)
- [ ] Test fixtures (isolation more important than reproducibility)

---

## Existing Good Patterns to Follow

### 1. ClockProtocol Pattern (Already Implemented)
```python
# jeeves_avionics/context.py
class SystemClock:
    def now(self) -> datetime:
        return datetime.now()
    def utcnow(self) -> datetime:
        return datetime.now(timezone.utc)
```

### 2. DeterministicIdGenerator Pattern (Already Implemented)
```python
# jeeves_shared/id_generator.py
class DeterministicIdGenerator:
    def __init__(self, seed: str = "test"):
        self._seed = seed
        self._counter = 0
    
    def generate(self) -> str:
        self._counter += 1
        return str(uuid5(_TEST_NAMESPACE, f"{self._seed}:{self._counter}"))
```

### 3. AppContext Injection (Already Implemented)
```python
# jeeves_avionics/context.py
@dataclass
class AppContext:
    settings: SettingsProtocol
    feature_flags: FeatureFlagsProtocol
    logger: LoggerProtocol
    clock: ClockProtocol = field(default_factory=SystemClock)
```

---

## Conclusion

The codebase has **excellent foundational infrastructure** for determinism but **inconsistent adoption**. The recommended changes are:

1. **Minimal:** Add optional clock/id_generator parameters to factory methods
2. **Medium:** Propagate through AppContext to all services
3. **Comprehensive:** Full Go clock abstraction and reproducibility test suite

The most impactful single change is updating `UnifiedEvent.create_now()` to accept optional clock and id_generator, as events flow through the entire system.
