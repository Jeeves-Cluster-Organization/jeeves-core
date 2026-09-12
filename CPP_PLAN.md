# C++ port plan (frozen)

1:1 C++ of `jeeves-core`. Only intentional delta: drop `genai`, replace with **prebuilt llama.cpp**. No CI. Rust crate stays until C++ tests match.

Do not re-litigate this document while implementing. Execute it.

---

## Layout

Rust keeps `src/`. C++ does not reuse that tree.

```
CMakeLists.txt
include/jeeves/types.hpp
include/jeeves/events.hpp
include/jeeves/tools.hpp
include/jeeves/workflow.hpp
include/jeeves/llm.hpp
include/jeeves/llm/llamacpp.hpp
include/jeeves/engine.hpp
include/jeeves/jeeves.hpp          # prelude
cpp/sync.hpp / cpp/sync.cpp        # internal channels, cancel, uuid
cpp/types.cpp
cpp/tools.cpp
cpp/workflow.cpp
cpp/llm.cpp
cpp/engine.cpp
cpp/llm/llamacpp.cpp
tests/validation.cpp
tests/runner.cpp
examples/pipeline.cpp
examples/llm_stream.cpp
```

Public names, builders, events, outcomes, and error strings match Rust unless noted below.

---

## Runtime (no Tokio, no Asio, no Boost)

One run = one `std::jthread`. Virtuals may block on that thread.

| Rust | C++ |
|---|---|
| `tokio::spawn` | `std::jthread` |
| `CancellationToken` + last-handle drop | shared `std::stop_source`; `RunLifetime` dtor `request_stop()` |
| unbounded mpsc | mutex + deque + cv (internal) |
| `watch` result | mutex + cv + `shared_ptr<RunOutcome>` |
| `select!` | timed cv wait + `stop_token` |
| `catch_unwind` | `try/catch (...)` → `ErrorKind::Panic` (not segfaults) |
| `async_trait` | virtual methods |
| `ModelStream` | pull `next()` → `optional<Result<ModelStreamEvent>>` |
| `Engine::start` requires Tokio | not required |
| implicit future cancel | `RunView::stop` (`std::stop_token`) so `PendingAction` can unblock |

`Engine::run` = `start`, drop event rx, wait result.

Public errors: `std::expected<T, Error>` (C++23). No exceptions across the library boundary. User-callback throws → `Panic`.

Cancel/timeout of a **blocking** action that ignores `stop` cannot be force-killed (same as blocking inside a Rust async fn). Approval, backoff, and llama abort **do** check stop. `PendingAction` in tests waits on `run.stop`.

---

## Dependencies

**Use**

- C++23, CMake ≥ 3.20
- nlohmann/json via FetchContent (header-only)
- GoogleTest via FetchContent (tests only)
- **Prebuilt llama.cpp** — Homebrew is already present:

  `/opt/homebrew` (`find_package(llama)` → target `llama`, includes `llama.h` / `llama-cpp.h`)

  CMake: `find_package(llama QUIET)` with prefix `/opt/homebrew`. If found, compile `llamacpp.cpp` and `JEEVES_HAS_LLAMA=1`. If not found, core + mock still build.

**Do not**

- FetchContent / compile llama.cpp from source
- Boost, Asio, HTTP, OpenSSL, uuid lib
- llama.cpp `common/` (unstable). Public C API only.

Hand-roll UUID v4.

---

## API mapping (keep 1:1)

- `Arc<str>` → `std::string`
- `Arc<T>` → `std::shared_ptr<T>`
- `Option` → `std::optional`
- `Result<T>` → `std::expected<T, Error>`
- `serde_json::Value` → `nlohmann::json`
- traits → abstract bases + `std::function` adapters (`deterministic_fn`, routers, reducers, prompts)
- `Duration` → `std::chrono::steady_clock::duration`
- `RunHandle::result()` blocking wait
- `take_events()` move-only receiver, once; clones have no rx
- `Finished` event and `result()` share the **same** `shared_ptr` (pointer equality test)

`RunView` extra field vs Rust: `std::stop_token stop`. Needed for cancel-safe actions.

---

## Engine semantics (copy, do not simplify)

Port `src/engine.rs` control flow as-is:

- first declared stage is entry
- visit/attempt counters; `max_visits` → `LimitKind::StageVisits`
- limits checked **before** spending (`consume_llm_call` / `consume_tool_call` / stage executions)
- on success: push history, route, pop, then reduce, then publish
- reducer is transactional; control stops (cancel/limit) skip reducer
- retry vs `on_error` after action failure; reduction failure can also `on_error`
- `ToolCallGuard`: abort event on drop if not finished
- approval fails immediately if event rx missing/dropped (`ErrorKind::Configuration`)
- LLM tool-not-found → JSON error to model, not stage fail
- denied LLM tool default `DenialBehavior::Continue`
- denied direct tool default `FailStage`
- `MaxTokens` stop → `Error::invalid_input("LLM output was truncated at its token limit")`
- structured output: buffer privately, parse JSON, validator is authoritative; no `TextDelta`
- text output: forward `TextDelta` while collecting
- schema is a provider hint only
- drop last handle cancels the run
- `merge_tool_call` by non-empty id

---

## LlamaCppProvider (replaces GenaiProvider)

Drop: `LLM_API_BASE`, `with_client`, HTTP extra_body test, genai credential resolution.

Keep `LlmProvider` + `MockLlmProvider` so runner tests never need a GGUF.

`LlamaCppProvider`:

- ctor takes GGUF path (Rust `new(default_model)` was a model id; here it is a file path)
- optional `with_model_role(role, path)` if we load more than one GGUF
- shared `llama_model`; **one context per `stream()`** (contexts are not concurrent)
- messages → `llama_chat_apply_template` + `llama_model_chat_template`
- `NativeChat` adapters match architecture + embedded template marker when libllama cannot
  apply that Jinja (Gemma 4 single-turn text is the first; no tools/history in that adapter)
- thinking-channel adapters skip the default JSON token grammar and deliver `visible_text`
  at completion; an explicit `extra_body.grammar` still applies; other models stream tokens
- token loop → `ModelStreamEvent::Text`
- `max_tokens` / `n_predict` → `ModelStopReason::MaxTokens`
- temperature / extra_body sampler keys: `top_k`, `top_p`, `min_p`, `seed`, `n_threads`, `grammar`
- `n_ctx` / `n_gpu_layers` are load/context params (ctor or extra_body at first stream)
- structured `response_schema` without tools: JSON syntax grammar unless explicitly overridden;
  semantic schema validation still belongs to the workflow
- usage from prompt vs generated token counts
- cancel → `llama_set_abort_callback` on the run `stop_token`
- RAII: `llama-cpp.h` unique_ptrs
- `llama_backend_init` once

Tool-call parsing from model text: best-effort (common `<tool_call>` / JSON name+arguments). Mock covers the runner tool-loop tests.

---

## Tests (must all exist and pass)

**validation.cpp**

- duplicate stages rejected
- missing static route rejected
- LLM workflow without provider rejected at engine build

**runner.cpp** (same cases as `tests/runner.rs`)

- linear pipeline + history
- exact stage/LLM limits allow completion
- retry-any preserves failed attempts
- exhausted failure routes to recovery
- unrecovered failure is `Failed`
- reducer derives state
- dynamic routing reads latest output
- text deltas + `Finished` same pointer as `result()`
- structured collect/validate/retry and **no** `TextDelta`
- token-limit stop rejects truncated JSON then retries
- direct tool args from history
- `max_tool_calls=0` does not invoke tool
- approval pauses and resumes same LLM loop
- denied LLM tool returns to model without execute
- unobserved approval → `Configuration`, tool not called
- explicit cancel → `Cancelled` (`PendingAction` waits on `run.stop`)

**doctest equivalents** (in runner or a small example test)

- `double` pipeline (`value: 21` → `42`)
- mock greet stream `"Hello, world!"`

**Drop**

- `sends_max_tokens_and_extra_body_to_openai_compatible_http`
- genai `parses_completed_streamed_tool_arguments` (only if we keep a llama tool-arg JSON-string helper)

Also enforce `WorkflowBuilder` / `LlmAction` / `EngineBuilder` / `ToolSpec` / `RunId` validation from Rust `build()` / constructors (messages can stay identical).

Optional gated GGUF test: not required for 1:1.

---

## Examples

- `examples/pipeline.cpp` — newsroom collect/verify retry (offline)
- `examples/llm_stream.cpp` — mock streaming deltas (offline)

---

## Build order (do not skip)

1. CMake + `types` + internal sync + nlohmann
2. `workflow` validation + validation tests
3. deterministic engine: route, retry, limits, reducer, cancel
4. tools + approval
5. LLM loop + mock + streaming events
6. `LlamaCppProvider` against Homebrew llama
7. examples
8. run tests; fix until green

No GitHub CI. Local: `cmake -B build && cmake --build build && ctest --test-dir build --output-on-failure`

---

## Out of scope

- persistence, detached runs, multi-consumer events
- compiling llama.cpp from source
- HTTP OpenAI-compatible provider
- matching Rust doctest harness / clippy / rustdoc
