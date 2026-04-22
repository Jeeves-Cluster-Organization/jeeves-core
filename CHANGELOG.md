# Changelog

## [Unreleased] — architecture reset

Complete rewrite. The repository is now a cargo workspace with a minimal,
policy-free `agent-core` crate and two harnesses (`harness-pi`, `harness-jeeves`)
consuming it.

### Removed

The entire multi-agent orchestration kernel, the CommBus IPC layer, the
envelope/pipeline machinery, the MCP binary, the tool health/catalog/ACL
subsystems, the process-lifecycle model, the rate limiter, and the old PyO3
surface. ~13 kLOC deleted.

### Added

- `agent-core` — single-agent ReAct loop, `Tool` / `Hook` / `Event` /
  `Session` / `LlmProvider` / `GenaiProvider` primitives.
- `harness-pi` — pi-style coding CLI with four default tools
  (read / write / edit / bash), AGENTS.md autoload, two-level settings.
- `harness-jeeves` — confirmation gate middleware, sliding-window compaction,
  OTel helper (gated behind `otel` feature).

### Principles

See [CONSTITUTION.md](CONSTITUTION.md).
