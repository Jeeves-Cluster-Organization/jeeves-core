# Changelog

All notable changes to jeeves-core (Rust kernel + Python infrastructure) will be documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

**Kernel (Rust):**
- Connection backpressure via semaphore (`max_connections` in `IpcConfig`)
- Per-frame read/write timeouts (configurable, prevents slowloris)
- Input validation helpers (`validation.rs`) for non-negative integer parsing
- Bounded envelope collection with configurable max capacity and eviction
- User usage cleanup in `CleanupService` (removes entries with no active processes)
- Envelope cleanup in `CleanupService` (evicts oldest terminated envelopes)
- Lifecycle events include `session_id` in all payloads
- IPC Protocol Specification (`docs/IPC_PROTOCOL.md`)
- Criterion benchmarks for IPC codec throughput (`benches/ipc_throughput.rs`)
- Property-based fuzz tests for IPC codec via proptest
- 13 unit tests for envelope module (stage lifecycle, bounds, serde round-trips)

**Infrastructure (Python):**
- `BodyLimitMiddleware` (ASGI) — rejects requests exceeding configurable size (default 1 MB)
- `max_request_body_bytes` field in `GatewayConfig`
- Rate limiting middleware error handling — returns 503 on kernel failure instead of crashing
- `redact_url()` utility in `utils/strings.py`
- Coverage configuration in `pyproject.toml` (`fail_under = 40`)
- 16 gateway unit tests (health, ready, root, body limit, CORS)
- 18 LLM provider unit tests (OpenAI HTTP, LiteLLM, factory)
- 83 orchestrator event tests
- 39 bootstrap tests
- Integration test scaffolding with `@pytest.mark.integration` gate

**Both:**
- GitHub Actions CI workflow (Rust: fmt, clippy, tarpaulin, audit; Python: ruff, black, mypy, pytest-cov, pip-audit)
- Pre-commit hooks (Rust: fmt, clippy; Python: ruff, black)
- Deployment guides (`docs/DEPLOYMENT.md`)
- API Reference (`docs/API_REFERENCE.md`)
- Import allowlist on `_try_import_capability()` — blocks bare stdlib imports

**Repo:**
- Merged jeeves-airframe into jeeves-core under `python/` directory
- Dockerfile (multi-stage: Rust kernel build + Python wheel)

### Changed

**Kernel (Rust):**
- Frame size limit reduced from 50 MB to 5 MB (configurable)
- `encode_msgpack()` replaces 6 `unwrap_or_default()` calls in `server.rs` — encoding failures now surface as errors
- README updated: removed stale gRPC/protobuf references, corrected Rust version to 1.75+, fixed method counts
- Stale Go/proto comments in `envelope/enums.rs` updated to reference Rust as source of truth

**Infrastructure (Python):**
- CORS default origins changed from `"*"` to `"http://localhost:8000,http://localhost:3000"`
- CORS wildcard + credentials combination now rejected at startup
- HTTP error responses in `chat.py` sanitized — 6 sites now return `"Internal server error"` instead of raw exception strings
- All 17 dependencies pinned with `<NEXT_MAJOR` upper bounds
- Redis URL credentials redacted in all 4 log sites (`client.py`, `connection_manager.py`)
- Stale "mirrors Go" comments in `protocols/types.py` updated to reference Rust

### Removed

**Kernel (Rust):**
- `envelope/export.rs` and `envelope/import.rs` (unimplemented stubs, zero callers)
- `#[allow(dead_code)]` on `IpcServer::shutdown()`

**Infrastructure (Python):**
- `websocket_manager.py` and `websocket.py` (dead WebSocket code, ~280 lines)
- `/ws` endpoint and EventBridge wiring from `app.py`
- `TEST_WEBSOCKET_URL` from test configuration

### Fixed

**Kernel (Rust):**
- Integer overflow protection on `RecordUsage` and `CheckRateLimit` handlers (reject negative values)
- `as i32` casts in `kernel.rs` and `orchestration.rs` replaced with checked conversions

**Infrastructure (Python):**
- `event_context.py` parameter ordering for Python 3.13 compatibility
- `allow_credentials` extracted to `GatewayConfig` (was missing from CORS configuration)

## [0.1.0] - Initial release

**Rust micro-kernel** for AI agent orchestration:
- Process lifecycle state machine (New → Ready → Running → Blocked → Terminated → Zombie)
- Resource quota enforcement (LLM calls, tokens, agent hops, iterations)
- Per-user sliding-window rate limiting
- IPC server (TCP + MessagePack) with 4 service handlers (27 methods total)
- CommBus message bus (pub/sub events, point-to-point commands, request/response queries)
- Envelope execution state container with pipeline stage management
- Multi-agent orchestration (session init, instruction dispatch, result reporting)
- Background cleanup service (zombie processes, stale sessions, resolved interrupts)
- Panic recovery for fault isolation
- OpenTelemetry tracing + Prometheus metrics

**Python infrastructure library** for LLM agent orchestration:
- FastAPI gateway with health/ready endpoints and CORS
- Kernel IPC client (TCP + MessagePack transport)
- LLM provider abstraction (OpenAI HTTP, LiteLLM, mock) with factory
- Redis client and connection manager
- Event orchestration (emitter, context, bridge)
- Bootstrap system (`create_app_context`, config-from-env, quota sync)
- Rate limiting middleware (requires kernel)
- Structured logging and metrics
