# Contributing

Jeeves Core contains only reusable in-process workflow mechanics. Domain
actions, prompts, UI policy, persistence, distributed execution, and consumer
telemetry belong in consumers or wrappers.

Before committing, run:

```bash
cargo fmt --all -- --check
cargo test --no-fail-fast
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

Keep changes direct and explicit:

- prefer Rust construction over a second configuration language;
- attach behavior to a workflow stage instead of a global registry;
- preserve typed terminal outcomes and ordered attempt history;
- add a core abstraction only when multiple consumers need the same mechanism;
- put cross-cutting provider and tool behavior in small wrappers.

The accepted architecture and remaining consumer work are recorded in
`docs/simplification-decisions.md`.

## License

Apache 2.0.
