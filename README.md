# Jeeves Core

Jeeves Core is a small, in-process Rust workflow runtime for deterministic work,
LLM calls, and tools. It is a library: there is no service, actor kernel, global
run registry, workflow file format, or persistence layer.

The runtime is intentionally direct:

- workflows are ordinary Rust values;
- each stage is a deterministic action, LLM action, direct tool call, or route;
- routing is synchronous, fallible, and read-only;
- each run is owned by a `RunHandle` and one Tokio task;
- events and approval responses are typed, in-process Rust values;
- every attempt is retained in ordered history;
- retries, bounds, error recovery, and optional state reduction are explicit.

Rust 1.75 or newer is required.

## Basic workflow

```rust
use jeeves_core::prelude::*;
use serde_json::json;

# async fn example() -> Result<()> {
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
# Ok(())
# }
```

The first declared stage is the entry point. A stage completes the workflow
unless `.next(...)` or `.route(...)` selects another stage. `.on_error(...)`
provides recovery after configured retries are exhausted.

## LLM stages

`LlmProvider` has one primitive: a stream of model events. Free-form text stages
forward `RunEvent::TextDelta` while collecting their final string. Structured
stages collect privately, parse the final JSON, and run the supplied validator;
invalid output is a normal stage failure and can use workflow retry.

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

Dropping the final handle cancels the task. Detached and recoverable runs are
outside this crate's scope.

## Reliability and extension

- `RetryPolicy` defaults to transient failures and can explicitly retry any
  stage failure.
- `RunLimits` bounds stage executions, LLM calls, tool calls, and an optional
  deadline; stages and LLM actions add visit and tool-round bounds.
- `CircuitBreakerTool` is an opt-in per-tool wrapper with threshold, cooldown,
  and status inspection.
- `LlmLoopHook` provides only the model/tool-loop interception points.
- Provider or tool wrappers are the extension point for caching, telemetry,
  authorization, and provider-local reliability behavior.

See [`docs/simplification-decisions.md`](docs/simplification-decisions.md) for
the accepted design and migration status. Public Rust API details are generated
from the crate documentation with `cargo doc --no-deps`.

## Development

```bash
cargo fmt --all -- --check
cargo test --no-fail-fast
cargo clippy --all-targets -- -D warnings
```

## License

Apache 2.0.
