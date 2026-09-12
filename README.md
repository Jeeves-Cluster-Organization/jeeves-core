# Jeeves Core

[![CI](https://github.com/Jeeves-Cluster-Organization/jeeves-core/actions/workflows/ci.yml/badge.svg)](https://github.com/Jeeves-Cluster-Organization/jeeves-core/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![MSRV](https://img.shields.io/badge/MSRV-1.75-orange.svg)

A small, in-process workflow runtime with Rust and C++23 implementations for deterministic work, LLM calls,
and tools. There is no service, actor kernel, global run registry, workflow
file format, or persistence layer: workflows are compiled-in values, and
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
uses its normal credential resolution. `LlmAction::with_max_tokens` bounds model
output; a provider-reported token-limit stop fails the stage rather than accepting
truncated output. `LlmAction::with_extra_body` forwards a JSON object of
provider-specific top-level request fields through Genai. Use it only with fields
supported by the selected endpoint; do not override the action's model, messages,
or output budget through this escape hatch.

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

## C++ implementation

The Rust crate and C++ library are maintained together. C++ follows the same
workflow, retry, routing, history, reducer, budget, tool, approval, and streaming
contracts. Its provider uses llama.cpp directly through the public C API. GenAI
remains in Rust; C++ has no GenAI or HTTP provider.

Build with CMake 3.21+ and a compiler/standard library supporting C++23
`std::expected` and C++20 `std::jthread`/stop tokens:

```bash
cmake -S . -B build-cpp
cmake --build build-cpp -j 4
ctest --test-dir build-cpp --output-on-failure
./build-cpp/jeeves_pipeline_example
./build-cpp/jeeves_llm_stream_example
```

CMake uses installed nlohmann/json and GoogleTest when available, otherwise
downloads them once. The C++ library always builds its pinned llama.cpp revision;
`LlamaCppProvider` is part of every C++ build.
Tests and examples default to enabled in a standalone build and disabled when
Jeeves is included by another CMake project. Override them with
`JEEVES_BUILD_TESTS` and `JEEVES_BUILD_EXAMPLES`. Link the CMake target
`jeeves::core` and include `<jeeves/jeeves.hpp>`.

```cpp
using namespace jeeves;

Result<std::shared_ptr<RunOutcome>> greet() {
    auto workflow = Workflow::builder("greet")
        .stage(Stage::llm("speak", LlmAction::text(Prompt::text("Greet warmly."))))
        .build();
    if (!workflow) return std::unexpected(workflow.error());

    auto registered = Engine::builder()
        .llm(MockLlmProvider::text("Hello, world!"))
        .workflow(std::move(*workflow));
    if (!registered) return std::unexpected(registered.error());
    auto engine = registered->build();
    if (!engine) return std::unexpected(engine.error());
    return engine->run("greet", "input");
}
```

The C++ API uses `Result<T>` (`std::expected<T, Error>`), `json`
(`nlohmann::json`), constructors in place of Rust `new`, and `ToolSpec::create`
for its fallible constructor. `Router`, `StateReducer`, and `PromptBuilder`
support derived implementations as well as closure adapters. Consumer callbacks
and shared handlers must support concurrent runs, as Rust requires `Send + Sync`.
Configure shared tools and providers before starting runs.

`RunEvent::value` is a `std::variant`; inspect it with `std::get_if` or `std::visit`.
An approval is the `ApprovalRequest` alternative. `RunOutcome::kind()` identifies
the terminal case, with `result()`, `error()`, and `limit()` providing its data.
`take_events()` returns a move-only receiver with blocking `recv()`/`next()`.
Copying a handle shares control and result but never copies the receiver.
The `Finished` event and `result()` contain the same outcome pointer.

Each run has one execution thread. Timed attempts use a temporary timer thread
to signal their stop token. `RunView::stop`, `ToolContext::cancellation`, and
`ModelRequest::stop` allow blocking implementations to cooperate with cancellation
and deadlines. The timer is joined when the attempt ends. Dropping the last
handle requests cancellation without waiting for consumer code; execution data
stays alive until that worker exits. Blocking callbacks that ignore the token
cannot be forcibly interrupted. Retry gets a fresh attempt token.

For local inference, construct `std::make_shared<LlamaCppProvider>("/path/model.gguf")`.
Models load lazily and are shared; each stream owns a separate context and sampler.
`with_model_role(role, path)` maps an action's model role to another GGUF; an
unmapped model name is treated as a path. `max_tokens` takes precedence over
`extra_body.n_predict` (default 512); exhausting either the output budget or
context reports `MaxTokens`. Supported `extra_body` settings are `top_k`, `top_p`,
`min_p`, `seed`, `n_threads`, `grammar`, `n_ctx` (default 4096), and `n_gpu_layers`
(fixed on the first load of each model). Structured schemas are included as
prompt hints; explicit GBNF can be supplied through `grammar`. `chat_template_kwargs`
are applied by llama.cpp's Jinja renderer; for example,
`{"chat_template_kwargs":{"enable_thinking":false}}` disables reasoning when the
model template supports it. The stage validator
remains authoritative. Tool parsing accepts complete `<tool_call>` blocks or
JSON containing `name` and `arguments`, including JSON-string arguments.
Tool generation is model-dependent and best-effort.

`tests/validation.cpp` and `tests/runner.cpp` cover every Rust integration-test
case and both crate documentation examples, plus C++ control/lifetime edge cases.
The Rust-only HTTP adapter test has no C++ equivalent. Parser/JSON-library
diagnostic suffixes can differ; runtime error kinds and engine messages match.
Tests use the mock provider and need no GGUF. Actual GGUF generation is optional
and is not part of the offline parity suite.

## License

Apache 2.0.
