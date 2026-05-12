# Contributing

```bash
cargo test                      # 170 lib + 23 integration + 2 schema
cargo clippy --all-features
cargo fmt
```

All three must be clean before submitting.

## Scope

`jeeves-core` is a micro-kernel. Changes belong here only if they extend orchestration primitives, not domain logic.

| Belongs here | Belongs in the consumer |
|---|---|
| Pipeline orchestration, routing, bounds | Domain-specific tool implementations |
| Agent trait + base impls | Custom agent implementations |
| Tool policy / catalog / health gates | Prompt templates, business logic |
| Streaming event types | Frontend / UI |

See `CONSTITUTION.md` for the full inclusion/exclusion list.

## License

Apache 2.0.
