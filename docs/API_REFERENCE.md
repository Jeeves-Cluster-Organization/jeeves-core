# API Reference

Jeeves Core is consumed as a library (PyO3 or Rust crate), not as an HTTP service.

## PyO3 API

### PipelineRunner

```python
from jeeves_core import PipelineRunner, tool
```

#### `PipelineRunner.from_json(path, prompts_dir=None)`

Create a runner from a pipeline JSON config file.

- `path` — path to `pipeline.json`
- `prompts_dir` — optional directory containing prompt templates (`{key}.txt` files)

#### `runner.register_tool(tool_fn)`

Register a Python tool function decorated with `@tool`.

```python
@tool(name="search", description="Search the web")
def search(params):
    return {"results": params.get("query")}

runner.register_tool(search)
```

#### `runner.run(input, user_id=None, session_id=None, metadata=None)`

Run a pipeline synchronously. Returns a dict:

```python
result = runner.run("hello", user_id="user1")
# {
#     "terminated": True,
#     "terminal_reason": "COMPLETED",
#     "outputs": {
#         "stage_name": {"key": "value"}
#     }
# }
```

#### `runner.stream(input, user_id=None, session_id=None, metadata=None)`

Run a pipeline with streaming events. Returns an iterator:

```python
for event in runner.stream("hello", user_id="user1"):
    match event["type"]:
        case "stage_started":   # {"stage": "understand", "pipeline": "my_pipeline"}
        case "delta":           # {"content": "Hello", "stage": "respond", "pipeline": "my_pipeline"}
        case "tool_call_start": # {"id": "...", "name": "search", "stage": "...", "pipeline": "..."}
        case "tool_result":     # {"id": "...", "content": "...", "stage": "...", "pipeline": "..."}
        case "stage_completed": # {"stage": "understand", "pipeline": "my_pipeline"}
        case "done":            # {"process_id": "...", "terminated": true, "terminal_reason": "COMPLETED", "outputs": {...}, "pipeline": "..."}
        case "error":           # {"message": "...", "stage": "...", "pipeline": "..."}
        case "interrupt":       # {"process_id": "...", "interrupt_id": "...", "kind": "...", "pipeline": "..."}
```

---

## Pipeline Config

Pipeline configuration is a JSON document or Rust `PipelineConfig` struct.

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Pipeline name |
| `stages` | array | yes | Ordered list of stages |
| `max_iterations` | int | yes | Max pipeline loop iterations |
| `max_llm_calls` | int | yes | Max total LLM API calls |
| `max_agent_hops` | int | yes | Max stage transitions |
| `edge_limits` | array | no | Per-edge traversal limits |
| `step_limit` | int | no | Global step limit |
| `state_schema` | array | no | Typed state merge fields |
| `subscriptions` | array | no | CommBus event types to subscribe to |
| `publishes` | array | no | CommBus event types this pipeline publishes |

### Stage Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | — | Stage identifier |
| `agent` | string | — | Agent to execute |
| `routing` | array | `[]` | Expression-based routing rules |
| `default_next` | string | null | Fallback target when no rule matches |
| `error_next` | string | null | Target on agent failure |
| `max_visits` | int | null | Per-stage visit limit |
| `node_kind` | string | `"Agent"` | `Agent`, `Gate`, or `Fork` |
| `output_key` | string | null | State field key (defaults to stage name) |
| `join_strategy` | string | `"WaitAll"` | `WaitAll` or `WaitFirst` (for Fork) |
| `has_llm` | bool | `true` | Whether agent calls LLM |
| `prompt_key` | string | null | Prompt template key |
| `temperature` | float | null | LLM temperature |
| `max_tokens` | int | null | LLM max tokens |
| `model_role` | string | null | LLM model role override |
| `allowed_tools` | array | null | Tool whitelist for this stage |
| `output_schema` | object | null | JSON Schema for output validation |
| `child_pipeline` | string | null | Run named pipeline as child (composition) |

### Example

```json
{
  "name": "assistant",
  "stages": [
    {
      "name": "understand",
      "agent": "understand",
      "has_llm": true,
      "prompt_key": "understand",
      "default_next": "respond"
    },
    {
      "name": "respond",
      "agent": "respond",
      "has_llm": true,
      "prompt_key": "respond"
    }
  ],
  "max_iterations": 10,
  "max_llm_calls": 5,
  "max_agent_hops": 5,
  "edge_limits": []
}
```

---

## Routing Expressions

Routing rules are expression trees evaluated in order (first match wins).

### Operators

`Eq`, `Neq`, `Gt`, `Lt`, `Gte`, `Lte`, `Contains`, `Exists`, `NotExists`, `And`, `Or`, `Not`, `Always`

### Field Scopes

| Scope | Syntax | Description |
|-------|--------|-------------|
| `Current` | `{"scope": "Current", "key": "intent"}` | Current agent output |
| `Agent` | `{"scope": "Agent", "agent": "X", "key": "Y"}` | Specific agent output |
| `Meta` | `{"scope": "Meta", "path": "user.tier"}` | Envelope metadata (dot-notation) |
| `Interrupt` | `{"scope": "Interrupt", "key": "response"}` | Interrupt response |
| `State` | `{"scope": "State", "key": "topic"}` | Pipeline state |

### Example

```json
{
  "routing": [
    {
      "expr": {"op": "Eq", "field": {"scope": "Current", "key": "intent"}, "value": "search"},
      "target": "search_stage"
    },
    {
      "expr": {"op": "Always"},
      "target": "default_stage"
    }
  ]
}
```

### Evaluation Order

1. Agent failed AND `error_next` set → route to error_next
2. Routing rules in order (first match wins)
3. No match + `default_next` set → route to default_next
4. No match + no default_next → terminate COMPLETED

---

## TerminalReason

`Completed`, `BreakRequested`, `MaxIterationsExceeded`, `MaxLlmCallsExceeded`, `MaxAgentHopsExceeded`, `UserCancelled`, `ToolFailedFatally`, `LlmFailedFatally`, `PolicyViolation`, `MaxStageVisitsExceeded`

---

## Rust Crate API

### Key Types

| Type | Module | Description |
|------|--------|-------------|
| `Kernel` | `kernel` | Process manager + orchestrator |
| `KernelHandle` | `worker::handle` | Typed channel to kernel actor (Clone, Send+Sync) |
| `PipelineConfig` | `kernel::orchestrator_types` | Pipeline definition |
| `PipelineStage` | `kernel::orchestrator_types` | Stage definition |
| `Envelope` | `envelope` | Request state container |
| `Instruction` | `kernel::orchestrator_types` | Kernel→Worker command |
| `Agent` | `worker::agent` | Agent trait |
| `AgentContext` | `worker::agent` | Execution context passed to agents |
| `PipelineEvent` | `worker::llm` | Streaming event variants |
| `CommBus` | `commbus` | Message bus |
| `AgentCard` | `kernel::agent_card` | Federation discovery |

### Agent Auto-Creation

When using `PipelineRunner.from_json()`, agents are auto-created from stage config:

| Condition | Agent Type |
|-----------|-----------|
| `node_kind: Gate` | `DeterministicAgent` |
| `child_pipeline` set | `PipelineAgent` |
| `has_llm: true` | `LlmAgent` |
| `has_llm: false` + matching tool | `McpDelegatingAgent` |
| `has_llm: false` + no tool | `DeterministicAgent` |
