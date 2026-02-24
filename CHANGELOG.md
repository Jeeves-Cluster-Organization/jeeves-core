# Changelog

All notable changes to jeeves-core will be documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
- GitHub Actions CI workflow (fmt, clippy, tarpaulin, audit)
- Pre-commit hooks (fmt, clippy)

### Changed
- Frame size limit reduced from 50 MB to 5 MB (configurable)
- `encode_msgpack()` replaces 6 `unwrap_or_default()` calls in `server.rs` — encoding failures now surface as errors
- README updated: removed stale gRPC/protobuf references, corrected Rust version to 1.75+, fixed method counts

### Removed
- `envelope/export.rs` and `envelope/import.rs` (unimplemented stubs, zero callers)
- `#[allow(dead_code)]` on `IpcServer::shutdown()`

### Fixed
- Integer overflow protection on `RecordUsage` and `CheckRateLimit` handlers (reject negative values)
- `as i32` casts in `kernel.rs` and `orchestration.rs` replaced with checked conversions

## [0.1.0] - Initial release

Rust micro-kernel for AI agent orchestration.

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
