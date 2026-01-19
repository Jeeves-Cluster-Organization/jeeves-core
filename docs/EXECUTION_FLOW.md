# Execution Flow - Entrypoints & Call Graphs

## 1. Main Server Entrypoints

### Python Gateway (`scripts/run/server.py` → `jeeves_mission_system/api/server.py`)
```
uvicorn.run() → FastAPI(lifespan) → lifespan()
   ├── State Created:
   │   ├── AppState (db, control_tower, orchestrator, event_bridge)
   │   ├── create_app_context() → AppContext (settings, logger, control_tower)
   │   └── ControlTower (lifecycle, resources, ipc, events, interrupts)
   ├── Agent Routing:
   │   ├── capability_registry.get_orchestrator() → orchestrator factory
   │   ├── control_tower.register_service(handler=orchestrator.get_dispatch_handler())
   │   └── Dispatch: ipc.dispatch(target, envelope) → handler(envelope)
   └── Endpoints:
       ├── POST /api/v1/requests → control_tower.submit_request(envelope)
       └── POST /api/v1/chat/clarifications → control_tower.resume_request(pid)
```

### Go CLI (`cmd/envelope/main.go`)
```
main() → switch(cmd)
   ├── create   → envelope.CreateGenericEnvelope() → State Created
   ├── process  → envelope.FromStateDict() → env.CanContinue() → env.ToStateDict()
   ├── can-continue → envelope.FromStateDict().CanContinue()
   └── result   → envelope.ToResultDict()
   State: Pure transformation, no mutation - JSON in/out via stdin/stdout
   Routing: None (stateless envelope operations)
```

## 2. Control Tower Kernel (`jeeves_control_tower/kernel.py`)
```
ControlTower.submit_request(envelope, priority)
   ├── lifecycle.submit(envelope) → PCB created (State)
   ├── resources.allocate(pid, quota) → ResourceUsage created (State)
   ├── lifecycle.schedule(pid) → READY state
   └── _execute_process(pid, envelope)
         ├── lifecycle.get_next_runnable() → PCB.state = RUNNING
         ├── ipc.dispatch(target, envelope) → Service Handler (Routing)
         ├── resources.check_quota() → quota violations
         └── events.get_pending_interrupt() → handle interrupts
```

## 3. Go Runtime (`coreengine/runtime/runtime.go`)
```
Runtime.Execute(ctx, envelope, opts)
   ├── initializeEnvelope() → StageOrder, CurrentStage (State)
   ├── Mode: Sequential/Parallel
   │   ├── runSequentialCore(): stage→agent→Process()→next_stage (Routing)
   │   └── runParallelCore(): GetReadyStages()→goroutines→merge
   └── Agent Routing: env.CurrentStage → agents[stage].Process() → env.CurrentStage updated
```

## 4. Test Entrypoints

| Location | Command | What Runs |
|----------|---------|-----------|
| `make test-ci` | `pytest jeeves_core_engine/tests jeeves_avionics/tests/unit/llm` | Unit tests (no deps) |
| `make test-tier1` | Core + Avionics + Mission contracts | Fast, no Docker |
| `make test-tier2` | + Database tests | Requires Docker Postgres |
| `make test-tier3` | + Integration w/ LLM | Requires llama-server |
| `make test-tier4` | + E2E | Full stack |

**Test fixtures**: `conftest.py` creates isolated `AppState`/`AppContext` per test

## State & Routing Summary

| Component | State Created | State Mutated | Agent Routing |
|-----------|--------------|---------------|---------------|
| **API Server** | AppContext, AppState, ControlTower | envelope.outputs, PCB.state | capability_registry → ipc.dispatch() |
| **ControlTower** | PCB, ResourceUsage, Quota | PCB.state, usage counters | ipc.dispatch(service_name, envelope) |
| **Go Runtime** | Envelope counters | env.Outputs, env.CurrentStage | config.StageOrder → agents[stage].Process() |
| **Go CLI** | None (stateless) | None | None |

## Request Flow (Full Path)
```
HTTP POST /api/v1/requests
  → submit_request() → create_generic_envelope()           # State: envelope created
  → control_tower.submit_request(envelope)
      → lifecycle.submit()                                  # State: PCB created
      → resources.allocate()                                # State: quota allocated
      → ipc.dispatch(default_service, envelope)             # Routing: to orchestrator
          → orchestrator.get_dispatch_handler()(envelope)   # Routing: pipeline stages
              → Runtime.Execute() → agents[stage].Process() # Routing: stage transitions
  → return envelope.outputs["integration"]["final_response"]
```
