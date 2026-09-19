# Jeeves Core

[![CI](https://github.com/Jeeves-Cluster-Organization/jeeves-core/actions/workflows/ci.yml/badge.svg)](https://github.com/Jeeves-Cluster-Organization/jeeves-core/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

A C++23 library for running bounded, in-process LLM workflows against a local
KoboldCpp backend. It includes a loopback-only OpenAI chat-completions client
and an optional cross-platform supervisor for a KoboldCpp process.

There is no service, global run registry, workflow file format, or persistence
layer. Workflows are compiled-in C++ values.

## Features

- Streaming chat completions, tool calls, token usage, and stop reasons.
- Loopback-only KoboldCpp connections.
- Optional KoboldCpp process launch, readiness checks, and bounded shutdown.
- Cancellation through stop tokens and KoboldCpp's keyed abort endpoint.
- Deterministic stages, routing, retries, limits, history, and reducers.
- In-process tools and human approval pauses.
- Installable CMake package.

## Requirements

- CMake 3.21 or newer.
- A compiler and standard library with C++23 `std::expected` and C++20
  `std::jthread`/stop-token support.
- nlohmann/json 3.11.3 or newer.

CMake uses an installed nlohmann/json and GoogleTest when available, otherwise
it downloads them during configuration. The library does not fetch or link an
inference engine.

## Build and test

```bash
cmake -S . -B build -DJEEVES_BUILD_TESTS=ON -DJEEVES_BUILD_EXAMPLES=ON
cmake --build build -j 4
ctest --test-dir build --output-on-failure
```

Tests and examples default to enabled for a standalone build and disabled when
the project is included by another CMake project. Override that behavior with
`JEEVES_BUILD_TESTS` and `JEEVES_BUILD_EXAMPLES`.

## Use from CMake

Add the project as a subdirectory and link `jeeves::core`, or install it as a
CMake package:

```bash
cmake --install build --prefix /path/to/prefix
```

An installed consumer can then use:

```cmake
find_package(JeevesCore 0.1 CONFIG REQUIRED)
target_link_libraries(my_app PRIVATE jeeves::core)
```

Include the complete public API with:

```cpp
#include <jeeves/jeeves.hpp>
```

## KoboldCpp provider

`KoboldCppProvider` maps messages, tools, temperature, output limits, response
schemas, and non-reserved `extra_body` fields to `/v1/chat/completions`. It
turns the SSE response into text, tool-call, usage, and stop events. A provider
serializes generations to match KoboldCpp's single-user server behavior.

```cpp
#include <jeeves/llm/koboldcpp.hpp>

jeeves::KoboldCppEndpoint endpoint{
    .host = "127.0.0.1",
    .port = 5001,
    .model = "local",
};

jeeves::KoboldCppProvider provider(endpoint);
jeeves::ModelRequest request{
    .messages = {jeeves::Message::user("Hello")},
};

auto stream = provider.stream(request);
if (!stream) {
    // Inspect stream.error().
}
```

The endpoint must use `127.0.0.1` or `localhost` with a nonzero port.

## KoboldCpp process supervisor

`KoboldCppProcess` launches a separate KoboldCpp executable without a shell or
launcher UI, binds it to `127.0.0.1` on a private port, captures diagnostics,
checks its version/model/context size, and owns its lifetime.

```cpp
jeeves::KoboldCppProcessConfig config{
    .executable = "/path/to/koboldcpp",
    .model = "/path/to/model.gguf",
    .chat_completions_adapter = "/path/to/adapter.jinja",
    .diagnostics_file = "/path/to/koboldcpp.log",
    .backend = jeeves::KoboldCppGpuBackend::Metal,
    .context_size = 8192,
    .required_version = "1.99",
};

auto process = jeeves::KoboldCppProcess::launch(std::move(config));
if (!process) {
    // Inspect process.error().
}

auto ready = (*process)->wait_until_ready();
jeeves::KoboldCppProvider provider((*process)->endpoint());
```

Metal, CUDA, and Vulkan are explicit backends. Launch requests full GPU offload
and exposes no CPU fallback setting. The application owns the returned RAII
process and must keep it alive while the provider is in use.

## Runtime behavior

`Result<T>` is `std::expected<T, Error>`, and `json` is `nlohmann::json`.
`ModelStream::next()` returns streamed model events. `RunEvent::value` and
`ModelStreamEvent::value` are variants that can be inspected with `std::get_if`
or `std::visit`.

Each workflow run has one execution thread. Blocking providers, tools, and
callbacks should observe their supplied stop token. Dropping the last run handle
requests cancellation without waiting for consumer code.

## License

Apache 2.0.
