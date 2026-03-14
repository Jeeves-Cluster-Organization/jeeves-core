# Contributing to Jeeves Core

## Development Setup

```bash
git clone https://github.com/Jeeves-Cluster-Organization/jeeves-core.git
cd jeeves-core
cargo test                      # 302 tests
cargo clippy --all-features     # lint
```

For PyO3 development:
```bash
pip install -e .                # builds via maturin
```

## Before Submitting

1. `cargo test` — all tests pass
2. `cargo clippy --all-features` — no warnings
3. `cargo fmt` — code formatted

## What Belongs Here

Jeeves Core is the **micro-kernel**. Changes should provide orchestration primitives, not domain logic.

| Belongs here | Belongs in capability layer |
|-------------|---------------------------|
| Process lifecycle | Domain-specific tools |
| Pipeline orchestration | Prompt templates |
| Bounds enforcement | Business logic |
| CommBus (IPC) | Frontend/UI |
| Agent trait + base impls | Custom agent implementations |

## Commit Messages

Follow conventional commits: `feat:`, `fix:`, `docs:`, `test:`, `refactor:`

## License

Contributions are licensed under Apache License 2.0.
