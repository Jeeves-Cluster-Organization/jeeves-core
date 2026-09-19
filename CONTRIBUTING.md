# Contributing

Jeeves Core contains only reusable in-process workflow mechanics. Domain
actions, prompts, UI policy, persistence, distributed execution, and consumer
telemetry belong in consumers or wrappers.

Before committing, run `just check`, or the equivalent commands:

```bash
cmake -S . -B build -DJEEVES_BUILD_TESTS=ON -DJEEVES_BUILD_EXAMPLES=ON
cmake --build build -j 4
ctest --test-dir build --output-on-failure
```

User-facing examples live in `examples/` and should stay runnable offline.

Keep changes direct and explicit:

- prefer direct C++ construction over a second configuration language;
- attach behavior to a workflow stage instead of a global registry;
- preserve typed terminal outcomes and ordered attempt history;
- add a core abstraction only when multiple consumers need the same mechanism;
- put cross-cutting provider and tool behavior in small wrappers.

The architecture and consumer-facing constraints are described in `README.md`.

## License

Apache 2.0.
