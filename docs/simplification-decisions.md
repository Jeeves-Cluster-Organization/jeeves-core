# Jeeves Core simplification

Status: implemented in Jeeves Core on 2026-07-10. Consumer migration remains a
separate change.

## Accepted design

### Runtime ownership

- `Engine` owns immutable workflows and a shared LLM provider.
- Each run owns its mutable state in one task; `RunHandle` owns its events,
  result, cancellation, and approval responses.
- There is no kernel actor or central run registry. `RunId` is attribution, not
  lookup. Dropping the final handle cancels the run; detached runs do not exist.

### Workflows and data

- Workflows, prompts, actions, and routes are constructed in Rust. Core has no
  JSON/YAML/RON loader, workflow schema, prompt registry, or hot reload.
- `serde_json::Value` is retained only at dynamic payload boundaries: input,
  metadata, state, stage output, model structure, and tool arguments/results.
- A stage explicitly owns a deterministic, LLM, direct-tool, or route-only
  action. There is no agent factory, global agent registry, name-based tool
  inference, or no-op fallback.
- Ordered attempt history is authoritative. Latest successful output is a
  convenience view. Optional shared state is consumer-defined by a reducer over
  attempt records; core has no merge DSL.

### Routing and capabilities

- Routing is synchronous, fallible, and read-only. Async or mutating work is an
  action before routing.
- LLM tools are statically attached to an LLM stage. Routing chooses capability
  profiles, and each tool handler remains its authorization boundary.

### Reliability

- Stage retry is explicit. The default class covers transient failures;
  `retry_on_any_failure` covers every action error.
- Bounds are stage executions, LLM calls, tool calls, optional deadline,
  optional per-stage visits, and LLM-local tool rounds.
- Error recovery runs after retries. Successful recovery may complete the run,
  while the original failures remain in history. An unrecovered failure is
  never reported as completed.
- Global health aggregation was removed. Circuit breaking is an opt-in tool
  wrapper with failure threshold, cooldown, and inspectable status.

### LLMs, tools, and events

- `LlmProvider` exposes one streaming operation. The `genai` adapter remains in
  core; it does not hide retries from workflow accounting.
- Text output forwards prose deltas. Structured output is buffered privately,
  parsed, and passed through a consumer validator before success.
- Human tool approval pauses the live task and resumes it through the handle.
  LLM denial continues with a structured tool error by default; direct-tool
  denial fails the stage by default. Continue, fail, and cancel overrides exist.
- The broad agent hook was replaced by one LLM-loop hook around model and tool
  calls. Wrappers own caching, telemetry, authorization, and provider/tool
  reliability behavior.
- Events are typed in-process values. Attempt failures are history/events, and
  each retained run produces one terminal `Finished` outcome.

## Implementation result

- Replaced the actor/kernel/orchestrator stack with `Engine`, `Execution`, and
  `RunHandle`.
- Replaced serialized workflow configuration with validated Rust builders.
- Consolidated LLM text, structured output, ReAct tool calls, hooks, approval,
  retry, routing, limits, and terminal outcomes into the direct execution path.
- Removed the old agent, kernel, run, registry, ACL/catalog, health aggregation,
  observability, schema-generation, and test-harness modules.
- Reduced direct dependencies to Tokio, Tokio Util, futures, async-trait,
  serde_json, UUID, and genai.
- Replaced legacy tests with behavior-focused runtime, boundary, provider, and
  workflow-validation coverage.

## Remaining integration work

- `Game_MVP` still targets the deleted kernel/agent/tool-registry API. Migrate
  it to Rust-built workflows, direct deterministic actions, static LLM tool
  sets, and `RunHandle` events in a separate consumer commit.
- Other consumers must make the same source-level migration. This is accepted
  during bootstrap; no compatibility shim or migration layer will be added.
- A live-provider smoke test remains environment-dependent. Core tests use the
  deterministic mock provider and verify the genai tool-argument conversion.

## Deliberate constraints

- Runs and approvals are memory-only and process-local.
- Workflow or prompt changes require recompilation.
- The event receiver is single-consumer and unbounded while retained. Callers
  must drain it, discard it, or use `Engine::run`; completion is published on a
  separate result channel and never waits on event consumption.
- Structured schemas are provider hints; the consumer-supplied validator is the
  authoritative compliance check.
