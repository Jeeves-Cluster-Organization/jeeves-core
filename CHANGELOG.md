# Changelog

All notable changes to jeeves-core will be documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **A2A Composition** — `PipelineAgent` runs a child pipeline as a stage in a parent pipeline
  - `child_pipeline` field on `PipelineStage` (mutually exclusive with `has_llm`)
  - OnceLock-based two-pass registry rebuild for circular dependency resolution
  - Streaming event bridge with dotted pipeline name attribution (`parent.child`)
  - Nested output extraction under parent stage's output_key
- **CommBus Federation** — cross-pipeline communication
  - 5 new `KernelCommand` variants: `PublishEvent`, `Subscribe`, `Unsubscribe`, `CommBusQuery`, `ListAgentCards`
  - Async actor dispatch with fire-and-spawn pattern for queries (deadlock prevention)
  - Subscription wiring at session init from `PipelineConfig.subscriptions`
  - Event inbox drain at `get_next_instruction()` into `envelope.event_inbox`
  - Automatic cleanup on process termination
  - `register_command_handler()`, `unsubscribe()`, `get_query_handler()`, `cleanup_disconnected()` on CommBus
- **AgentCard** — federation discovery registry (`src/kernel/agent_card.rs`)
- `pipeline: String` field on all `PipelineEvent` variants (required, not optional)
- `pipeline_name` field on `AgentContext`
- `event_inbox` on `Envelope` for CommBus event accumulation
- `subscriptions` and `publishes` fields on `PipelineConfig`
- `Any` supertrait on `Agent` trait (enables downcasting for OnceLock backfill)
- 21 new tests for federation and composition

### Changed

- `dispatch()` in actor.rs is now async (required for fire-and-spawn CommBusQuery)

---

## 2026-03-13

### Added

- **Streaming pipeline events** — `PipelineEvent` variants carry stage context
  - `StageStarted`, `StageCompleted`, `ToolCallStart`, `ToolResult` events
  - Observable tool execution with per-call events
- Metadata parameter on PyO3 `PipelineRunner.run()` / `.stream()`

### Fixed

- Kernel output propagation — agent outputs correctly merged into envelope
- Event serde — PipelineEvent serialization matches Python consumer expectations
- PyO3 test coverage for buffered and streaming modes

---

## 2026-03-12

### Added

- **PyO3 binding layer** — kernel as importable Python library (`from jeeves_core import PipelineRunner, tool`)
  - `PipelineRunner.from_json()` — load pipeline config from JSON file
  - `PipelineRunner.run()` — buffered execution, returns dict
  - `PipelineRunner.stream()` — streaming via `PyEventIterator`
  - `@tool` decorator for registering Python functions as kernel tools
  - `ToolBridge` — Python↔Rust tool dispatch
  - Agent auto-creation from pipeline.json stages (LlmAgent, McpDelegatingAgent, DeterministicAgent)
- **MCP stdio server** — replaces HTTP gateway
  - `src/main.rs` — JSON-RPC on stdin/stdout, tracing to stderr
  - `JEEVES_MCP_SERVERS` env var for upstream MCP server auto-connect (proxy pattern)
  - `mcp-stdio` feature flag (requires clap)
- `McpDelegatingAgent` — agent that delegates to a named tool
- `AgentConfig` for config-driven agent registration

### Removed

- **HTTP gateway** (`src/worker/gateway.rs`) — replaced by PyO3 + MCP stdio
- `http-server` feature flag and axum runtime dependency

---

## 2026-03-10

### Added

- **DX improvements** — PipelineBuilder DSL, discovery, client helpers
  - `PipelineBuilder` with fluent API (`src/kernel/builder.rs`)
  - God-file splits for orchestrator types and routing
- MCP client (`McpToolExecutor`) with HTTP and stdio transports
- Tool health tracking with sliding-window circuit breaking
- Execution tracking on envelope (stage numbers, timing)
- Interrupt lifecycle wiring (resolve + resume flow)

---

## 2026-03-08

### Added

- **Single-process Rust-only kernel** — eliminates Python + IPC
  - `worker::actor` — kernel actor (single `&mut Kernel` behind mpsc channel)
  - `worker::handle` — `KernelHandle` typed channel wrapper (Clone, Send+Sync)
  - `worker::agent` — `Agent` trait, `LlmAgent`, `DeterministicAgent`, `AgentRegistry`
  - `worker::llm` — `LlmProvider` trait, OpenAI-compatible HTTP client, mock provider
  - `worker::tools` — `ToolExecutor` trait, `ToolRegistry`
  - `worker::prompts` — `PromptRegistry` with `{var}` template substitution
- `PipelineStage` fields: `prompt_key`, `has_llm`, `temperature`, `max_tokens`, `model_role`
- Integration tests for kernel actor + agent task round-trip

### Removed

- **Entire Python layer** (~30,800 LOC): kernel_client, pipeline_worker, gateway, LLM providers
- **IPC layer** (~3,449 LOC): TCP server, msgpack codec, router, all handler modules
- Python dependencies, maturin build system (later re-added for PyO3)

### Changed

- Architecture: multi-process (Python + TCP+msgpack IPC + Rust) → single-process (Rust only)

---

## 2026-02-28

### Added

- **Graph primitives** — `NodeKind` (Agent, Gate, Fork), state merge, break semantics
- Parallel execution via Fork topology with `JoinStrategy` (WaitAll, WaitFirst)
- `state_schema` on `PipelineConfig` for typed state merge
- `output_key` on `PipelineStage`
- `max_visits` per-stage visit limit

---

## 2026-02-15

### Added

- Envelope→AgentContext migration, typed LLM boundaries
- Pipeline validation pushed to Rust kernel (from Python)
- Terminal classification in Rust (TerminalReason enum)
- Tool ACL enforcement at kernel level

---

## [0.1.0] - Initial release

Rust micro-kernel for AI agent orchestration:
- Process lifecycle state machine (New → Ready → Running → Blocked → Terminated → Zombie)
- Resource quota enforcement (LLM calls, tokens, agent hops, iterations)
- Per-user sliding-window rate limiting
- CommBus message bus (pub/sub events, point-to-point commands, request/response queries)
- Envelope execution state container with pipeline stage management
- Multi-agent orchestration (session init, instruction dispatch, result reporting)
- Background cleanup service (zombie processes, stale sessions, resolved interrupts)
- Tracing via `tracing` crate (compact or JSON output)
