# Contributing

## Setup

```bash
git clone https://github.com/jeeves-cluster-organization/jeeves-core.git
cd jeeves-core
cargo test --workspace
```

## Before submitting

```bash
cargo test --workspace      # all tests pass
cargo clippy --workspace    # no warnings
cargo fmt --all             # formatted
```

## Where does my change belong?

This workspace has a strict split, documented in [CONSTITUTION.md](CONSTITUTION.md):

| If your change is… | It goes in… |
|---|---|
| A new primitive every harness would need (events, budget counter, tool trait extension) | `agent-core` |
| A default two plausible harnesses would disagree on | a harness |
| Pi-style UX (session commands, TUI, AGENTS.md) | `harness-pi` |
| Production concerns (confirmation gate, OTel, Python bindings) | `harness-jeeves` |
| An entirely new shape (Slack bot, VS Code extension) | a new harness crate |

If you can't decide, err on the side of the harness — policy can graduate into
core later, but core leaking into policy is hard to reverse.

## Commits

Conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`.

## License

Apache-2.0.
