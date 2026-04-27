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

#### `runner.describe_pipeline(pipeline_name=None)`

Return the full pipeline config as a Python dict (stages, routing, bounds, state_schema).

#### `runner.list_tools()`

Return all registered tools: `[{"name": str, "description": str, "parameters": dict}, ...]`

#### `PipelineRunner.get_schema()` (static method)

Return JSON Schema for PipelineConfig. Use for editor validation of pipeline.json files.

#### `runner.checkpoint(process_id)`

Capture a serializable snapshot of pipeline state at the current instruction boundary.
Returns a dict with: version, timestamp, process_id, envelope, pcb, session_state, edge_traversals, stage_visits.

#### `runner.resume_from_checkpoint(snapshot_json, pipeline_name=None)`

Restore a pipeline from a JSON checkpoint string. Returns the restored process_id.

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
| `routing_fn` | string | null | Name of registered routing function |
| `default_next` | string | null | Fallback target when no routing_fn or it terminates |
| `error_next` | string | null | Target on agent failure (checked before routing_fn) |
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
| `max_context_tokens` | int | null | Max estimated tokens in LLM context (chars/4 heuristic) |
| `context_overflow` | string | `"Fail"` | Strategy: `"Fail"` or `"TruncateOldest"` |

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

### State Schema (Accumulation)

`state_schema` controls how stage outputs merge into `envelope.state`:

| Strategy | Behavior |
|----------|----------|
| `Replace` | Overwrite (default) |
| `Append` | Append to array (creates if missing) |
| `MergeDict` | Shallow-merge object keys |

```json
"state_schema": [
  {"key": "retrieved_docs", "merge": "Append"},
  {"key": "context", "merge": "MergeDict"}
]
```

Use `Append` for multi-hop RAG (accumulate docs across loops). Use `MergeDict` for parallel Fork outputs.

---

## Routing Functions

Routing is code. Consumers register named `RoutingFn` closures on the Kernel before spawning. Pipeline stages reference them by name via `routing_fn`. Static wiring (`default_next`, `error_next`) remains declarative.

### RoutingContext

Routing functions receive a `RoutingContext` with read-only access to:
- `current_stage`, `agent_name`, `agent_failed`
- `outputs` — all agent outputs (`agent_name -> {key -> value}`)
- `metadata` — envelope metadata
- `interrupt_response` — resolved interrupt response, if any
- `state` — accumulated state across loop iterations

### RoutingResult

| Variant | Description |
|---------|-------------|
| `Next(String)` | Route to a single target stage |
| `Fan(Vec<String>)` | Fan out to multiple stages in parallel (Fork) |
| `Terminate` | End the pipeline |

### Evaluation Order

1. Agent failed AND `error_next` set → route to error_next
2. `routing_fn` registered → call it
3. No routing_fn + `default_next` set → route to default_next
4. No routing_fn + no default_next → terminate COMPLETED

---

## TerminalReason

`Completed`, `BreakRequested`, `MaxIterationsExceeded`, `MaxLlmCallsExceeded`, `MaxAgentHopsExceeded`, `UserCancelled`, `ToolFailedFatally`, `LlmFailedFatally`, `PolicyViolation`, `MaxStageVisitsExceeded`, `EdgeLimitExceeded`

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
| `ToolRegistryBuilder` | `worker::tools` | Composable tool registry builder |
| `AgentFactoryBuilder` | `worker::agent_factory` | Agent auto-creation from pipeline config |
| `AclToolExecutor` | `worker::tools` | Composable ACL wrapper for ToolExecutor |
| `CheckpointSnapshot` | `kernel::checkpoint` | Serializable pipeline state snapshot |

### Agent Auto-Creation

When using `PipelineRunner.from_json()`, agents are auto-created from stage config:

| Condition | Agent Type |
|-----------|-----------|
| `node_kind: Gate` | `DeterministicAgent` |
| `child_pipeline` set | `PipelineAgent` |
| `has_llm: true` | `LlmAgent` |
| `has_llm: false` + matching tool | `ToolDelegatingAgent` |
| `has_llm: false` + no tool | `DeterministicAgent` |

---

## Consumer Patterns

### Rust

```rust
use jeeves_core::prelude::*;

let tools = ToolRegistryBuilder::new()
    .add_executor(Arc::new(MyTools::new()))
    .build();
let agents = AgentFactoryBuilder::new(llm, prompts, tools.clone())
    .add_pipeline(config.clone())
    .build();
let result = run_pipeline_with_envelope(&handle, pid, config, envelope, &agents).await?;
```

### Python

```python
runner = PipelineRunner.from_json("pipeline.json", prompts_dir="prompts/",
    openai_api_key=os.getenv("OPENAI_API_KEY"))
runner.register_tool(my_tool)

print(runner.list_tools())           # Introspect tools
print(runner.describe_pipeline())    # Introspect pipeline

result = runner.run("hello", user_id="user1")
snapshot = runner.checkpoint(result["process_id"])
```

### pipeline.json (with new features)

```json
{
  "$schema": "../jeeves-core/schema/pipeline.schema.json",
  "name": "my_pipeline",
  "max_iterations": 10, "max_llm_calls": 20, "max_agent_hops": 15,
  "state_schema": [{"key": "docs", "merge": "Append"}],
  "stages": [{
    "name": "search", "agent": "search", "has_llm": true,
    "max_context_tokens": 16000, "context_overflow": "TruncateOldest",
    "allowed_tools": ["web_search"], "default_next": "answer"
  }]
}
```

---

## Test References

| Test File | What's Tested |
|-----------|--------------|
| `src/kernel/mod.rs` | MergeStrategy (Replace, Append, MergeDict) |
| `src/worker/tools.rs` | ToolRegistryBuilder, AclToolExecutor |
| `src/kernel/checkpoint.rs` | Checkpoint round-trip serialization |
| `src/worker/agent.rs` | Context overflow (Fail, TruncateOldest) |
| `src/kernel/orchestrator.rs` | step_limit, EdgeLimitExceeded, routing |
| `tests/worker_integration.rs` | Full pipeline integration tests |

Use the `test-harness` feature flag for test utilities in consumer integration tests.
