# Changelog

All notable changes to jeeves-core will be documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `src/worker/` module — single-process agent execution replacing Python infrastructure + IPC bridge
  - `worker::actor` — kernel actor (single `&mut Kernel` behind mpsc channel)
  - `worker::handle` — `KernelHandle` typed channel wrapper (Clone, Send+Sync)
  - `worker::agent` — `Agent` trait, `LlmAgent`, `DeterministicAgent`, `AgentRegistry`
  - `worker::llm` — `LlmProvider` trait, OpenAI-compatible HTTP client (reqwest), mock provider
  - `worker::gateway` — axum HTTP gateway (chat, sessions, health, status)
  - `worker::tools` — `ToolExecutor` trait, `ToolRegistry`
  - `worker::prompts` — `PromptRegistry` with `{var}` template substitution
- `PipelineStage` fields for agent execution: `prompt_key`, `has_llm`, `temperature`, `max_tokens`, `model_role`
- Integration tests for kernel actor + agent task round-trip (5 tests)
- CLI via clap: `jeeves-kernel run --http-addr --llm-model --llm-api-key --llm-base-url`

### Removed

- **Entire Python layer** (`python/` — ~30,800 LOC): kernel_client, pipeline_worker, gateway, LLM providers, bootstrap, protocols, runtime, orchestrator, capability_wiring
- **IPC layer** (`src/ipc/` — ~3,449 LOC): TCP server, msgpack codec, router, all 6 handler modules
- `tests/ipc_integration.rs` (887 LOC), `benches/ipc_throughput.rs` (75 LOC)
- `codegen/generate_python_types.py`
- Python dependencies: msgpack, httpx, pydantic, fastapi, litellm, etc.
- maturin build system

### Changed

- Architecture: multi-process (Python + TCP+msgpack IPC + Rust) → single-process (Rust only)
- Consumer contract: Python library imports → HTTP API + Rust crate API
- Default listen address: `127.0.0.1:50051` (IPC) → `0.0.0.0:8080` (HTTP)
- Env var: `AIRFRAME_KERNEL_ADDRESS` → `JEEVES_HTTP_ADDR`
- Dockerfile: multi-stage maturin+Python → pure Rust build

---

## Previous [Unreleased]

### Added

**Kernel (Rust):**
- Connection backpressure via semaphore (`max_connections` in IPC config)
- Per-frame read/write timeouts (configurable, prevents slowloris)
- Input validation helpers (`validation.rs`) for non-negative integer parsing
- Bounded envelope collection with configurable max capacity and eviction
- User usage cleanup in `CleanupService`
- Envelope cleanup in `CleanupService`
- Lifecycle events include `session_id` in all payloads
- Criterion benchmarks for IPC codec throughput
- Property-based fuzz tests for IPC codec via proptest
- 13 unit tests for envelope module

### Changed

**Kernel (Rust):**
- Frame size limit reduced from 50 MB to 5 MB (configurable)
- `encode_msgpack()` replaces 6 `unwrap_or_default()` calls — encoding failures surface as errors

### Removed

**Kernel (Rust):**
- `envelope/export.rs` and `envelope/import.rs` (unimplemented stubs, zero callers)

### Fixed

**Kernel (Rust):**
- Integer overflow protection on `RecordUsage` and `CheckRateLimit` handlers
- `as i32` casts replaced with checked conversions

## [0.1.0] - Initial release

Rust micro-kernel for AI agent orchestration:
- Process lifecycle state machine (New → Ready → Running → Blocked → Terminated → Zombie)
- Resource quota enforcement (LLM calls, tokens, agent hops, iterations)
- Per-user sliding-window rate limiting
- CommBus message bus (pub/sub events, point-to-point commands, request/response queries)
- Envelope execution state container with pipeline stage management
- Multi-agent orchestration (session init, instruction dispatch, result reporting)
- Background cleanup service (zombie processes, stale sessions, resolved interrupts)
- Panic recovery for fault isolation
- OpenTelemetry tracing + Prometheus metrics
