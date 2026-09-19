# C++ implementation notes

The C++23 library mirrors Jeeves Core's Rust workflow contracts: stages, routing,
retry policy, limits, history, reducers, run handles, events, cancellation,
approval, tools, structured output validation, and usage accounting.

## Provider boundary

`LlmProvider` remains the sole model boundary and `MockLlmProvider` remains the
default test seam. `KoboldCppProvider` is a loopback HTTP adapter for KoboldCpp's
OpenAI-compatible `/v1/chat/completions` endpoint. It emits the existing
`ModelStreamEvent` alternatives and does not own a model or process.

`KoboldCppProcess` is an optional companion API for applications that bundle the
engine. It launches a separate executable without a shell or UI, forces
`127.0.0.1`, chooses a private port, supplies model/adapter/context/GPU flags,
captures diagnostics, polls readiness, and performs bounded shutdown. Metal,
CUDA, and Vulkan are explicit; CPU fallback is rejected. The application owns
the supervisor lifetime.

Cancellation closes the active chat-completion socket and makes a best-effort
keyed POST to `/api/extra/abort`. The provider consumes OpenAI SSE incrementally,
reassembles streamed tool calls, preserves usage-before-stop ordering, and admits
one active generation at a time. Workflow retry and cancellation semantics remain
in the engine. Response schemas are still hints; parsing and the action's validator
remain the authoritative structured-output checks.

## Build and verification

The CMake graph contains no embedded model runtime. It needs only nlohmann/json,
threads, and `ws2_32` on Windows. Offline workflow tests use mocks. Separate
loopback fixture tests cover OpenAI request serialization, malformed responses,
HTTP failures, cancellation/abort, token usage/stop mapping, process readiness,
and validation. The install/export smoke test builds an external consumer against
the generated `JeevesCore` CMake package.
