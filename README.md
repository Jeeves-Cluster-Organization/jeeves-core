# Jeeves Core

[![CI](https://github.com/Jeeves-Cluster-Organization/jeeves-core/actions/workflows/ci.yml/badge.svg)](https://github.com/Jeeves-Cluster-Organization/jeeves-core/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![MSRV](https://img.shields.io/badge/MSRV-1.75-orange.svg)

A small, in-process Rust workflow runtime for deterministic work, LLM calls,
and tools. There is no service, actor kernel, global run registry, workflow
file format, or persistence layer: workflows are compiled-in Rust values, and
changing them requires rebuilding.

The runtime is intentionally direct:

- each stage is a deterministic action, LLM action, direct tool call, or route;
- routing is synchronous, fallible, and read-only;
- every attempt is retained in ordered history;
- retries, bounds, error recovery, and optional state reduction are explicit;
- events and approval responses are typed, in-process Rust values.

## Where it fits

Jeeves Core occupies a narrow niche on purpose: **bounded, auditable stage
graphs that run inside your process**. It is not an agent framework.

| If you need... | Consider |
|---|---|
| Agents, RAG pipelines, provider integrations | [`rig`](https://rig.rs), [`swiftide`](https://swiftide.rs) |
| LangGraph-style stateful graphs with session persistence | [`graph-flow`](https://github.com/a-agmon/rs-graph-llm) |
| Hard per-run cost bounds, panic-isolated stages, full attempt audit trail, human approval pauses — embedded in an app or game | **Jeeves Core** |

Distinctive behaviors: run budgets (`RunLimits`) are enforced *before* model or
tool calls are spent; panics in actions, routing, reducers, or the run itself
become typed errors instead of process crashes; and tools can pause execution
in place to await a human decision through the run handle.

## Quick start

The same example runs as a doctest in the crate root docs:

```rust
use jeeves_core::prelude::*;
use serde_json::json;

let workflow = Workflow::builder("double")
    .stage(
        Stage::deterministic_fn("read", |run: &RunView<'_>| {
            let value = run.input["value"]
                .as_i64()
                .ok_or_else(|| Error::invalid_input("value must be an integer"))?;
            Ok(json!(value))
        })
        .next("double"),
    )
    .stage(Stage::deterministic_fn("double", |run: &RunView<'_>| {
        let value = run
            .latest_output("read")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| Error::internal("read output missing"))?;
        Ok(json!(value * 2))
    }))
    .build()?;

let engine = Engine::builder().workflow(workflow)?.build()?;
let outcome = engine.run("double", json!({"value": 21})).await?;

assert_eq!(outcome.result().latest_outputs["double"], json!(42));
```

Runnable versions live in `examples/`:

```bash
cargo run --example pipeline      # deterministic stages, retry, recovery routing
cargo run --example llm_stream    # streaming LLM output via the offline mock provider
```

The first declared stage is the entry point. A stage completes the workflow
unless `.next(...)` or `.route(...)` selects another stage. `.on_error(...)`
provides recovery after configured retries are exhausted.

## LLM stages

`LlmProvider` has one primitive: a stream of model events. Free-form text stages
forward `RunEvent::TextDelta` while collecting their final string. Structured
stages collect privately, parse the final JSON, and run the supplied validator;
invalid output is a normal stage failure and can use workflow retry. The
response schema sent to the provider is only a hint; the supplied validator is
the authoritative compliance check.

The included `GenaiProvider` supports the providers handled by the `genai`
crate. `LLM_API_BASE` selects an OpenAI-compatible endpoint; otherwise `genai`
uses its normal credential resolution.

```rust
use jeeves_core::prelude::*;
use std::sync::Arc;

let llm: Arc<dyn LlmProvider> = Arc::new(GenaiProvider::new("gpt-4o-mini"));
let stage = Stage::llm(
    "reply",
    LlmAction::text(Prompt::text("Reply clearly and concisely.")),
);
```

Prompts are strings or Rust builders attached to the stage. Tools are attached
statically to an `LlmAction`, so routing between stages also routes between
capability sets.

## Handles, events, and approval

`Engine::run` is the simple collect-to-completion path. `Engine::start` returns a
`RunHandle` for streaming, cancellation, and in-place human approval. Take and
drain its event receiver when using streaming. If events are not needed, use
`Engine::run` or call `RunHandle::discard_events` so unused deltas do not queue.
Workflows that can request approval must retain and drain the event receiver;
an approval request fails immediately if it cannot be presented to a consumer.

When a tool's `approval` method returns a prompt, execution emits
`RunEvent::ApprovalRequested` and waits for `respond_to_approval`. Denied LLM
tool calls return an error value to the model by default; denied direct tool
stages fail by default. Either behavior can be overridden per action.

Runs, events, history, and approvals are memory-only and process-local; there
is no persistence or recovery across restarts. Dropping the final handle
cancels the task. Detached and recoverable runs are outside this crate's scope.

## Reliability and extension

- `RetryPolicy` defaults to transient failures and can explicitly retry any
  stage failure.
- `RunLimits` bounds stage executions, LLM calls, tool calls, and an optional
  deadline; stages and LLM actions add visit and tool-round bounds.
- `LlmLoopHook` provides only the model/tool-loop interception points.
- Wrapping a `Tool` or an `LlmProvider` is the extension point for caching,
  telemetry, authorization, fail-fast breakers, or rate limiting. Return
  `Error::unavailable(...)` from such a wrapper and core retry semantics treat
  the failure as automatically retryable.

Public Rust API details are generated from the crate documentation with
`cargo doc --no-deps`.

## Testing

Behavior coverage lives in two integration suites: `tests/validation.rs`
(workflow construction rules) and `tests/runner.rs` (routing, retries, limits,
streaming, structured output, tools, approvals, cancellation). Documentation
examples execute as doctests under `cargo test`.

## Development

```bash
just check    # fmt + check + test + clippy + doc, matching CI
```

Rust 1.75 or newer is required.

## License

Apache 2.0.
