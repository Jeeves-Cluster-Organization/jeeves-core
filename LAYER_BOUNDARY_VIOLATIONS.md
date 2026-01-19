# Layer Boundary Violations Report

**Generated:** 2026-01-19  
**Branch:** cursor/layer-boundary-violations-717d

This report identifies the top 10 layer boundary violations in the codebase, including:
- Lower layers importing higher layers
- Hidden globals (module-level mutable state)
- Cross-module side effects

---

## Layer Architecture Reference

From `HANDOFF.md`:

```
┌─────────────────────────────────────────────────────────────────┐
│ L4: jeeves_mission_system                                       │
│     - Orchestration framework, HTTP/gRPC API                    │
├─────────────────────────────────────────────────────────────────┤
│ L3: jeeves_avionics                                             │
│     - Infrastructure (LLM providers, DB, Gateway)               │
├─────────────────────────────────────────────────────────────────┤
│ L2: jeeves_memory_module                                        │
│     - Event sourcing, semantic memory, session state            │
├─────────────────────────────────────────────────────────────────┤
│ L1: jeeves_control_tower                                        │
│     - OS-like kernel (process lifecycle, resources)             │
├─────────────────────────────────────────────────────────────────┤
│ L0: jeeves_protocols + jeeves_shared                            │
│     - Type contracts (zero dependencies)                        │
└─────────────────────────────────────────────────────────────────┘
```

**Import Rules:**
| Layer | May Import From |
|-------|-----------------|
| L4 (mission_system) | L3, L2, L1, L0 |
| L3 (avionics) | L2, L1, L0 |
| L2 (memory_module) | L1, L0 |
| L1 (control_tower) | L0 only |
| L0 (protocols/shared) | Nothing |

---

## Top 10 Violations

### 1. **CRITICAL: Lower Layer Imports Higher Layer**

**File:** `jeeves_avionics/observability/metrics.py`  
**Line:** 39  
**Violation Type:** Layer boundary violation (L3 → L4)

```python
from jeeves_mission_system.common.models import VerificationReport
```

**Why it breaks layering:**  
Avionics (L3) imports from Mission System (L4). This creates an upward dependency that violates the four-layer architecture. L3 should only depend on L2, L1, and L0.

**Fix:** Move `VerificationReport` to `jeeves_protocols` (L0) or pass it as a parameter via dependency injection.

---

### 2. **Hidden Global: CommBus Singleton**

**File:** `jeeves_control_tower/ipc/commbus.py`  
**Line:** 469  
**Violation Type:** Hidden global state

```python
_global_bus: Optional[InMemoryCommBus] = None

def get_commbus(logger: Optional[LoggerProtocol] = None) -> InMemoryCommBus:
    global _global_bus
    if _global_bus is None:
        _global_bus = InMemoryCommBus(logger=logger)
    return _global_bus
```

**Why it breaks layering:**  
Module-level mutable singleton creates implicit coupling. Any module importing `get_commbus()` shares state with all other modules. This makes testing difficult and creates hidden dependencies between otherwise unrelated components.

**Fix:** Use explicit dependency injection. Pass `InMemoryCommBus` instance through constructors.

---

### 3. **Hidden Global: Settings Singleton**

**File:** `jeeves_avionics/settings.py`  
**Line:** 298  
**Violation Type:** Hidden global state with cross-module side effects

```python
_settings: Optional[Settings] = None

def get_settings() -> Settings:
    global _settings
    if _settings is None:
        _settings = Settings()
    return _settings
```

**Why it breaks layering:**  
The `Settings` singleton is accessed across multiple layers (L4, L3, L2). Changes to settings in one module affect all other modules silently. This creates non-obvious coupling and makes the system harder to reason about.

**Fix:** Bootstrap settings at application startup and inject via `AppContext` pattern already documented in `jeeves_mission_system/bootstrap.py`.

---

### 4. **Hidden Global: Config Registry Singleton**

**File:** `jeeves_mission_system/config/registry.py`  
**Line:** 119  
**Violation Type:** Hidden global state

```python
_global_registry: Optional[ConfigRegistry] = None

def get_config_registry() -> ConfigRegistry:
    global _global_registry
    if _global_registry is None:
        _global_registry = ConfigRegistry()
    return _global_registry
```

**Why it breaks layering:**  
Global mutable state allows any module to register configurations that affect other modules. Registration order becomes implicit, creating race conditions in tests and unpredictable behavior.

**Fix:** Create registry at bootstrap and pass through dependency injection.

---

### 5. **Hidden Global: OpenTelemetry Adapter**

**File:** `jeeves_avionics/observability/otel_adapter.py`  
**Line:** 531  
**Violation Type:** Hidden global state with cross-module side effects

```python
_global_adapter: Optional[OpenTelemetryAdapter] = None

def set_global_otel_adapter(adapter: OpenTelemetryAdapter) -> None:
    global _global_adapter
    _global_adapter = adapter
```

**Why it breaks layering:**  
Any module can call `set_global_otel_adapter()` to change the global adapter, affecting tracing behavior across the entire application. This is a side effect that crosses module boundaries invisibly.

**Fix:** Initialize adapter at application startup and inject into components that need tracing.

---

### 6. **Hidden Global: Feature Flags Singleton**

**File:** `jeeves_avionics/feature_flags.py`  
**Line:** 433  
**Violation Type:** Hidden global state

```python
_feature_flags: Optional[FeatureFlags] = None

def get_feature_flags() -> FeatureFlags:
    global _feature_flags
    if _feature_flags is None:
        _feature_flags = FeatureFlags()
    return _feature_flags
```

**Why it breaks layering:**  
Feature flags are accessed from multiple layers to control behavior. Hidden global state means flag changes propagate silently, making it difficult to track which code paths are affected.

**Fix:** Inject `FeatureFlagsProtocol` via dependency injection as done in some components.

---

### 7. **Hidden Global: Event Dedup Cache**

**File:** `jeeves_memory_module/services/event_emitter.py`  
**Line:** 170  
**Violation Type:** Hidden global state with cross-module side effects

```python
_global_dedup_cache: Optional[SessionDedupCache] = None

def _get_global_dedup_cache() -> SessionDedupCache:
    global _global_dedup_cache
    if _global_dedup_cache is None:
        _global_dedup_cache = SessionDedupCache()
    return _global_dedup_cache
```

**Why it breaks layering:**  
The dedup cache accumulates state across requests. Multiple `EventEmitter` instances share the same cache, which can lead to memory leaks and cross-session contamination if not properly managed.

**Fix:** Scope cache to session/request lifecycle and inject via constructor.

---

### 8. **Hidden Global: Webhook Service**

**File:** `jeeves_avionics/webhooks/service.py`  
**Line:** 626  
**Violation Type:** Hidden global state

```python
_global_webhook_service: Optional[WebhookService] = None

def init_webhook_service(logger: Optional[LoggerProtocol] = None) -> WebhookService:
    global _global_webhook_service
    _global_webhook_service = WebhookService(logger)
    return _global_webhook_service
```

**Why it breaks layering:**  
Webhook subscriptions are stored in global state. Any module can add subscriptions that affect event delivery across the system, creating hidden coupling.

**Fix:** Create service at bootstrap and inject where needed.

---

### 9. **Hidden Global: gRPC Client**

**File:** `jeeves_avionics/gateway/grpc_client.py`  
**Line:** 155  
**Violation Type:** Hidden global state requiring initialization order

```python
_client: Optional[GrpcClientManager] = None

def get_grpc_client() -> GrpcClientManager:
    if _client is None:
        raise RuntimeError("gRPC client not initialized. Check app lifespan.")
    return _client
```

**Why it breaks layering:**  
This global requires specific initialization order during app startup. If called before initialization, it throws a runtime error. This creates implicit temporal coupling between modules.

**Fix:** Use explicit dependency injection via FastAPI's dependency system.

---

### 10. **Hidden Global in L0: Capability Resource Registry**

**File:** `jeeves_protocols/capability.py`  
**Line:** 743  
**Violation Type:** Global state in foundational layer

```python
_resource_registry: Optional[CapabilityResourceRegistry] = None

def get_capability_resource_registry() -> CapabilityResourceRegistry:
    global _resource_registry
    if _resource_registry is None:
        _resource_registry = CapabilityResourceRegistry()
    return _resource_registry
```

**Why it breaks layering:**  
L0 (`jeeves_protocols`) should be pure type definitions with zero mutable state. Having a global registry here means the foundational layer has side effects, which can affect all layers that import from it.

**Fix:** Move registry management to L3 (avionics) or L4 (mission_system) and keep L0 purely declarative.

---

## Summary

| # | File | Line | Type | Severity |
|---|------|------|------|----------|
| 1 | `jeeves_avionics/observability/metrics.py` | 39 | L3→L4 import | **Critical** |
| 2 | `jeeves_control_tower/ipc/commbus.py` | 469 | Hidden global | High |
| 3 | `jeeves_avionics/settings.py` | 298 | Hidden global | High |
| 4 | `jeeves_mission_system/config/registry.py` | 119 | Hidden global | High |
| 5 | `jeeves_avionics/observability/otel_adapter.py` | 531 | Hidden global | Medium |
| 6 | `jeeves_avionics/feature_flags.py` | 433 | Hidden global | Medium |
| 7 | `jeeves_memory_module/services/event_emitter.py` | 170 | Hidden global | Medium |
| 8 | `jeeves_avionics/webhooks/service.py` | 626 | Hidden global | Medium |
| 9 | `jeeves_avionics/gateway/grpc_client.py` | 155 | Hidden global | Medium |
| 10 | `jeeves_protocols/capability.py` | 743 | Global in L0 | Medium |

---

## Recommendations

1. **Immediate:** Fix violation #1 by moving `VerificationReport` to `jeeves_protocols`
2. **Short-term:** Replace global singletons with dependency injection at bootstrap
3. **Long-term:** Implement an application context pattern that wires all dependencies explicitly

The codebase already has partial DI patterns (e.g., `AppContext`, protocol injection) - these should be extended consistently across all layers.
