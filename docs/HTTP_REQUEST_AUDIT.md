# HTTP Request Path Audit: POST /api/v1/requests → final_response

## Call Chain (numbered)

| # | File | Symbol | State Mutated |
|---|------|--------|---------------|
| 1 | `jeeves_mission_system/api/server.py` | `submit_request()` | None (entry) |
| 2 | `jeeves_protocols/agents.py` | `create_generic_envelope()` | **envelope created**: envelope_id, user_id, session_id, metadata |
| 3 | `jeeves_control_tower/kernel.py` | `ControlTower.submit_request()` | None (orchestrates) |
| 4 | `jeeves_control_tower/lifecycle/manager.py` | `LifecycleManager.submit()` | **PCB created**: pid, state=NEW, priority, quota, timestamps |
| 5 | `jeeves_control_tower/kernel.py` | `_events.emit_event()` | **event history**: process_created event added |
| 6 | `jeeves_control_tower/resources/tracker.py` | `ResourceTracker.allocate()` | **usage record**: pid→ResourceUsage(0,0,0,0,0,0) |
| 7 | `jeeves_control_tower/lifecycle/manager.py` | `LifecycleManager.schedule()` | **PCB.state**: NEW → READY, added to ready_queue |
| 8 | `jeeves_control_tower/kernel.py` | `_execute_process()` | None (orchestrates) |
| 9 | `jeeves_control_tower/lifecycle/manager.py` | `get_next_runnable()` | **PCB.state**: READY → RUNNING, started_at set |
| 10 | `jeeves_control_tower/ipc/coordinator.py` | `CommBusCoordinator.dispatch()` | **service.current_load**: +1 |
| 11 | `jeeves_control_tower/ipc/coordinator.py` | `handler(envelope)` | → calls registered orchestrator handler |
| 12 | *Capability Layer* | `Orchestrator.dispatch_handler()` | **envelope.outputs**: stage outputs added |
| 13 | *Capability Layer* | *Pipeline stages* | **envelope.current_stage**, counters, outputs per stage |
| 14 | `jeeves_control_tower/ipc/coordinator.py` | dispatch returns | **service.current_load**: -1 |
| 15 | `jeeves_control_tower/resources/tracker.py` | `check_quota()` | None (read-only check) |
| 16 | `jeeves_control_tower/lifecycle/manager.py` | `transition_state()` | **PCB.state**: RUNNING → TERMINATED, completed_at set |
| 17 | `jeeves_mission_system/api/server.py` | return | **Response**: envelope.outputs["integration"]["final_response"] |

## Python Runtime vs Go Runtime: PROOF

**Answer: This path uses PYTHON RUNTIME exclusively.**

### Evidence:

1. **Orchestrator is Python** (`jeeves_mission_system/api/server.py:257-263`):
   ```python
   orchestrator = orchestrator_config.factory(
       llm_provider_factory=llm_factory,
       tool_executor=tool_executor,
       ...
   )
   ```
   The factory is a Python callable registered via `CapabilityResourceRegistry`.

2. **Handler is async Python** (`jeeves_control_tower/ipc/coordinator.py:178-228`):
   ```python
   async def dispatch(...) -> GenericEnvelope:
       handler = self._handlers.get(service_name)
       result = await asyncio.wait_for(handler(envelope), timeout=...)
   ```
   The handler is an `async def` Python coroutine.

3. **GenericEnvelope is Python dataclass** (`jeeves_protocols/envelope.py:27`):
   ```python
   @dataclass
   class GenericEnvelope:
       """Python mirror of Go's GenericEnvelope."""
   ```

4. **No Go subprocess in request path**: `grep subprocess jeeves_mission_system/` returns no matches.

5. **Go bridge exists but is OPTIONAL** (`jeeves_avionics/interop/go_bridge.py`):
   - Used for envelope validation ops via `go-envelope` binary
   - NOT invoked in the HTTP request path
   - Raises `GoNotAvailableError` if binary missing (system continues)

### Go Runtime Usage (NOT in HTTP path):

The Go Runtime (`coreengine/runtime/runtime.go`) is used for:
- CLI: `cmd/envelope/main.go` (direct JSON stdin/stdout)
- Potential gRPC services (not the default HTTP flow)
- Envelope operations via `GoBridge` (optional validation)

## State Summary

| Component | Created | Mutated During Request |
|-----------|---------|------------------------|
| GenericEnvelope | Step 2 | outputs, current_stage, counters (Step 12-13) |
| PCB | Step 4 | state transitions, timestamps (Steps 7,9,16) |
| ResourceUsage | Step 6 | llm_calls, tool_calls, tokens (Step 12-13) |
| Event History | Step 5 | Events appended throughout |
| Service Load | Step 10 | +1 on dispatch, -1 on return |

## Final Response Extraction

```python
# server.py:559-560
integration = result_envelope.outputs.get("integration", {})
response_text = integration.get("final_response")
```

The `final_response` key is set by the Integration agent (last pipeline stage) in the capability layer.
