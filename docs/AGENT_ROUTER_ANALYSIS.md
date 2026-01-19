# Agent Router/Dispatcher Analysis

**Date:** January 19, 2026  
**Branch:** cursor/agent-router-analysis-b65f

## Executive Summary

After comprehensive analysis of the codebase, the agent routing system is **a real router with policy + selection**, not just a function call chain. It implements a configuration-driven, multi-layered routing system with conditional transitions, cyclic graph support, parallel execution, and resource-bounded termination.

---

## Architecture Overview

The routing system operates across three layers:

1. **Control Tower Kernel** (`jeeves_control_tower/kernel.py`) - OS-like process scheduler
2. **Pipeline Runtime** (`jeeves_protocols/agents.py`, `coreengine/runtime/runtime.go`) - Stage orchestration
3. **Agent-Level Routing** (`coreengine/agents/unified.go`) - Per-agent conditional transitions

---

## Layer 1: Control Tower (Kernel Dispatch)

### Location
- `jeeves_control_tower/kernel.py`
- `jeeves_control_tower/ipc/coordinator.py`

### Routing Inputs

| Input | Type | Description |
|-------|------|-------------|
| `envelope` | `GenericEnvelope` | Request payload with user input, session state |
| `priority` | `SchedulingPriority` | REALTIME, HIGH, NORMAL, LOW, IDLE |
| `quota` | `ResourceQuota` | Resource limits (LLM calls, tool calls, timeouts) |
| `service_name` | `str` | Target service (default: "flow_service") |

### Decision Logic

```python
# kernel.py:217-265
async def _execute_process(self, pid: str, envelope: GenericEnvelope):
    # 1. Check process is runnable (PCB state machine)
    pcb = self._lifecycle.get_next_runnable()
    
    # 2. Build dispatch target
    target = DispatchTarget(
        service_name=self._default_service,  # "flow_service"
        method="run",
        priority=pcb.priority,
        timeout_seconds=pcb.quota.timeout_seconds,
    )
    
    # 3. Dispatch via IPC coordinator
    result = await self._ipc.dispatch(target, envelope)
    
    # 4. Check for interrupts (CLARIFICATION, CONFIRMATION, etc.)
    interrupt = self._events.get_pending_interrupt(pid)
```

### Outputs

| Output | Description |
|--------|-------------|
| Routed envelope | Dispatched to registered service handler |
| Process state changes | NEW → READY → RUNNING → WAITING/TERMINATED |
| Kernel events | `process.created`, `process.state_changed`, `interrupt.raised` |

### Verdict: Real Router

The Control Tower is a **genuine service router** with:
- Priority-based scheduling (`SchedulingPriority`)
- Service discovery (`CommBusCoordinator.register_service()`)
- Load balancing awareness (`current_load` tracking)
- Health checking (`service.healthy`)

---

## Layer 2: Pipeline Runtime (Stage Orchestration)

### Location
- Python: `jeeves_protocols/agents.py` (class `Runtime`)
- Go: `coreengine/runtime/runtime.go`

### Routing Inputs

| Input | Type | Description |
|-------|------|-------------|
| `envelope` | `GenericEnvelope` | Pipeline state envelope |
| `PipelineConfig` | config | Agent definitions, bounds, edge limits |
| `RunOptions.Mode` | `sequential` \| `parallel` | Execution mode |

### Decision Logic

#### Sequential Mode

```python
# agents.py:406-441
async def run(self, envelope: GenericEnvelope, thread_id: str = "") -> GenericEnvelope:
    while envelope.current_stage != "end" and not envelope.terminated:
        # 1. Check bounds (max_iterations, max_llm_calls, max_agent_hops)
        if not self._can_continue(envelope):
            break
        
        # 2. Check interrupt states (clarification, confirmation)
        if envelope.current_stage in ("clarification", "confirmation"):
            break
        
        # 3. Get agent for current stage
        agent = self.agents.get(envelope.current_stage)
        
        # 4. Execute agent (agent determines next_stage)
        envelope = await agent.process(envelope)
```

#### Parallel Mode (Go)

```go
// runtime.go:309-410
func (r *Runtime) runParallelCore(...) (*envelope.GenericEnvelope, error) {
    completed := make(map[string]bool)
    
    for !env.Terminated {
        // 1. Find stages with satisfied dependencies
        readyStages := r.Config.GetReadyStages(completed)
        
        // 2. Execute ready stages concurrently
        for _, stageName := range readyStages {
            go func(name string, a *agents.UnifiedAgent) {
                stageEnv := env.Clone()
                resultEnv, err := a.Process(ctx, stageEnv)
            }(stageName, agent)
        }
        
        // 3. Collect results, merge outputs
    }
}
```

### Stage Dependency System

The runtime supports a dependency graph for parallel execution:

```python
# config.py:47-51
class AgentConfig:
    requires: List[str]  # Hard dependencies - MUST complete
    after: List[str]     # Soft ordering - IF present
    join_strategy: JoinStrategy  # ALL (default) or ANY
```

```go
// pipeline.go:332-374
func (p *PipelineConfig) GetReadyStages(completed map[string]bool) []string {
    for _, agent := range p.Agents {
        // Check if all Requires are satisfied
        requiresSatisfied := all(completed[req] for req in agent.Requires)
        
        // For JoinAny, only need ONE satisfied
        if agent.JoinStrategy == JoinAny && len(agent.Requires) > 0 {
            requiresSatisfied = any(completed[req])
        }
    }
}
```

### Outputs

| Output | Description |
|--------|-------------|
| `envelope.current_stage` | Next stage to execute |
| `envelope.outputs` | Accumulated agent outputs keyed by `output_key` |
| Stage stream | `StageOutput` channel for streaming results |

### Verdict: Real Router

The pipeline runtime is a **real orchestration router** with:
- DAG-based parallel execution
- Dependency resolution (`requires`, `after`)
- Multiple join strategies (ALL, ANY)
- Configurable execution modes

---

## Layer 3: Agent-Level Routing (Conditional Transitions)

### Location
- Python: `jeeves_protocols/agents.py` (method `_determine_next_stage`)
- Go: `coreengine/agents/unified.go` (method `evaluateRouting`)

### Routing Inputs

| Input | Type | Description |
|-------|------|-------------|
| `output` | `Dict[str, Any]` | Agent's output after processing |
| `routing_rules` | `List[RoutingRule]` | Ordered conditional rules |
| `error_next` | `str` | Stage for error cases |
| `default_next` | `str` | Fallback if no rules match |

### Decision Logic

```go
// unified.go:361-380
func (a *UnifiedAgent) evaluateRouting(output map[string]any) string {
    // Evaluate rules IN ORDER (first match wins)
    for _, rule := range a.Config.RoutingRules {
        value, exists := output[rule.Condition]
        if exists && value == rule.Value {
            return rule.Target  // Route to matched target
        }
    }
    
    // Default fallback
    if a.Config.DefaultNext != "" {
        return a.Config.DefaultNext
    }
    return "end"
}
```

### Routing Rule Structure

```python
@dataclass
class RoutingRule:
    condition: str  # Key in output dict (e.g., "verdict")
    value: Any      # Expected value (e.g., "proceed", "loop_back")
    target: str     # Next stage name (e.g., "stageA", "end")
```

### Example: Cyclic Routing

```python
# From HANDOFF.md - capability-defined routing
STAGE_C = AgentConfig(
    name="stage_c",
    routing_rules=[
        RoutingRule(condition="verdict", value="proceed", target="end"),
        RoutingRule(condition="verdict", value="loop_back", target="stage_a"),
    ],
    default_next="end",
)
```

This creates a **cyclic graph** where `stage_c` can route back to `stage_a`.

### Cycle Control

Cycles are explicitly supported and bounded:

```go
// pipeline.go:4-18 (docstring)
// - RoutingRules allow agents to route to ANY stage, including earlier ones
// - EdgeLimits control how many times a specific edge can be traversed
// - MaxIterations provides a global bound on pipeline loop iterations

type EdgeLimit struct {
    From     string  // Source stage
    To       string  // Target stage  
    MaxCount int     // Max transitions (e.g., 3 loops max)
}
```

### Outputs

| Output | Description |
|--------|-------------|
| `envelope.current_stage` | Updated to next stage |
| Route taken | Logged for observability |

### Verdict: Real Router

Agent-level routing is a **conditional policy router** with:
- Rule-based decision logic (first-match semantics)
- Cyclic graph support (loops allowed)
- Edge-level bounds (`EdgeLimit`)
- Error routing (`error_next`)

---

## Complete Routing Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Control Tower Kernel                         │
│  ┌──────────────┐    ┌─────────────────┐    ┌──────────────────┐   │
│  │SubmitRequest │───▶│   Lifecycle     │───▶│  IPC Coordinator │   │
│  │ (priority,   │    │   Manager       │    │  (dispatch to    │   │
│  │  quota)      │    │ (PCB, states)   │    │   service)       │   │
│  └──────────────┘    └─────────────────┘    └────────┬─────────┘   │
└─────────────────────────────────────────────────────────┼───────────┘
                                                          │
                                                          ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       Pipeline Runtime                              │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │  while current_stage != "end":                              │    │
│  │      if not can_continue(bounds): break                     │    │
│  │      if interrupt_pending: break                            │    │
│  │                                                             │    │
│  │      agent = agents[current_stage]                          │    │
│  │      envelope = agent.process(envelope)                     │    │
│  │      # Agent sets envelope.current_stage via routing_rules  │    │
│  └────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Agent Routing (per-agent)                        │
│  ┌────────────────────────────────────────────────────────────┐    │
│  │  for rule in routing_rules:                                 │    │
│  │      if output[rule.condition] == rule.value:               │    │
│  │          return rule.target  # e.g., "stage_b"              │    │
│  │                                                             │    │
│  │  if output.get("error") and error_next:                     │    │
│  │      return error_next                                      │    │
│  │                                                             │    │
│  │  return default_next or "end"                               │    │
│  └────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Key Routing Policies

### 1. Resource Exhaustion Policy

```python
# kernel.py:269-298
quota_exceeded = self._resources.check_quota(pid)
if quota_exceeded:
    # Creates RESOURCE_EXHAUSTED interrupt
    await self._interrupts.create_resource_exhausted(...)
    result.terminated = True
    result.terminal_reason = TerminalReason(quota_exceeded)
```

### 2. Interrupt Handling Policy

```python
# kernel.py:344-403
if interrupt_kind == InterruptKind.TIMEOUT:
    envelope.terminated = True
    envelope.terminal_reason = TerminalReason.MAX_ITERATIONS_EXCEEDED
    
if interrupt_kind == InterruptKind.CLARIFICATION:
    envelope.interrupt_pending = True
    # Process enters WAITING state
```

### 3. Resume Policy

```go
// runtime.go:467-519
switch kind {
case InterruptKindClarification:
    env.CurrentStage = r.Config.ClarificationResumeStage  // e.g., "intent"
case InterruptKindConfirmation:
    if response.Approved {
        env.CurrentStage = r.Config.ConfirmationResumeStage  // e.g., "execution"
    } else {
        env.Terminate("User denied confirmation")
    }
}
```

---

## Final Verdict: Real Router (Policy + Selection)

### Evidence for Real Router

| Criterion | Present? | Evidence |
|-----------|----------|----------|
| **Dynamic target selection** | ✅ | `evaluateRouting()` chooses target based on output |
| **Policy rules** | ✅ | `RoutingRule` with condition/value/target |
| **Multiple routing strategies** | ✅ | Sequential, parallel, cyclic |
| **Service discovery** | ✅ | `CommBusCoordinator.register_service()` |
| **Load awareness** | ✅ | `service.current_load`, `service.healthy` |
| **Priority scheduling** | ✅ | `SchedulingPriority` enum |
| **Resource bounds** | ✅ | `ResourceQuota`, `EdgeLimit`, `MaxIterations` |
| **Interrupt handling** | ✅ | `InterruptKind`, WAITING state, resume logic |
| **Cycle support** | ✅ | Loops via routing rules, bounded by limits |

### What it is NOT

- **Not just a function call chain**: Routes are determined dynamically based on agent output
- **Not hardcoded**: All routing is configuration-driven (`AgentConfig`, `PipelineConfig`)
- **Not linear-only**: Supports cyclic graphs with bounded iterations

### Architecture Pattern

This is a **configuration-driven pipeline orchestrator** with:

1. **Declarative routing** - Routing rules defined in config, not code
2. **Multi-layer dispatch** - Kernel → Service → Pipeline → Agent
3. **Policy enforcement** - Resource quotas, interrupts, timeouts at each layer
4. **Flexible topology** - Sequential, parallel, and cyclic execution modes

---

## Files Analyzed

| File | Role |
|------|------|
| `jeeves_control_tower/kernel.py` | OS-like kernel, process lifecycle |
| `jeeves_control_tower/ipc/coordinator.py` | Service dispatch, IPC |
| `jeeves_control_tower/types.py` | PCB, ResourceQuota, DispatchTarget |
| `jeeves_protocols/agents.py` | Python Runtime, UnifiedAgent |
| `jeeves_protocols/config.py` | AgentConfig, PipelineConfig, RoutingRule |
| `coreengine/runtime/runtime.go` | Go Runtime, parallel execution |
| `coreengine/config/pipeline.go` | Go config, cyclic routing support |
| `coreengine/agents/unified.go` | Go UnifiedAgent, evaluateRouting() |
