# Jeeves-Core Audit Report

**Date**: 2026-02-06
**Scope**: Rust microkernel codebase only (no external Python consumers)
**Commit base**: `claude/audit-rust-microkernel-CQwHf`

---

## Executive Summary

The Jeeves-Core microkernel is a well-structured Rust implementation of a
multi-agent orchestration runtime. The codebase demonstrates sound
architectural principles: zero unsafe code, strongly-typed IDs, clear module
boundaries, and a single-actor ownership model for mutable state.

| Metric | Value |
|---|---|
| Total Rust source files | ~30 |
| Estimated LOC | ~8,700 |
| Unit tests (lib) | 96 |
| Integration tests | 11 |
| Doc tests | 1 passing, 2 ignored |
| **All tests passing** | **Yes (107/107)** |
| Clippy errors (lib) | 1 (`unwrap_used` in non-test code) |
| Clippy warnings (lib) | 22 |
| Reported coverage | 81.36% (576/708 lines) |

---

## Tracked Issues

Each issue below is scoped to this repository only, with no scope expansion.

---

### ISSUE-01: Clippy `deny(unwrap_used)` violation in `interrupts.rs`

**Severity**: Error (blocks `cargo clippy` clean pass)
**File**: `src/kernel/interrupts.rs:573`

```rust
*stats.get_mut(key).unwrap() += 1;
```

The `Cargo.toml` sets `unwrap_used = "deny"`, but `get_stats()` uses
`.unwrap()` on a `HashMap::get_mut()` call. While the keys are pre-inserted
on lines 561-564, clippy cannot prove this statically.

**Fix**: Replace with `.entry().and_modify()` or use `if let Some(v) = ...`.

---

### ISSUE-02: Stub implementations will panic at runtime

**Severity**: High
**Files**:
- `src/envelope/export.rs:8` — `unimplemented!("to_json will be implemented in checkpoint 2")`
- `src/envelope/import.rs:11` — `unimplemented!("from_json will be implemented in checkpoint 2")`
- `src/observability.rs:5-11` — `init_tracing()` and `init_metrics()` are no-ops

`export::to_json()` and `import::from_json()` will **panic** if called.
These are public functions in a `pub mod`, so any consumer (including gRPC
service code) could invoke them.

**Fix**: Either implement them, or change to returning `Err(...)` /
`todo!()` with a deprecation warning, or gate behind a feature flag.

---

### ISSUE-03: Observability is not wired up

**Severity**: Medium
**File**: `src/observability.rs`

`init_tracing()` and `init_metrics()` are empty no-ops despite the crate
declaring dependencies on `tracing-subscriber`, `tracing-opentelemetry`,
`opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, and `prometheus`.
The `main.rs` calls `init_tracing()` but gets no tracing subscriber
installed.

**Impact**: No structured logging, no distributed tracing, no metrics
collection in production. The dependencies are dead weight until wired up.

**Fix**: Implement tracing subscriber setup, or remove the unused
dependencies from `Cargo.toml` to reduce compile time.

---

### ISSUE-04: `tokio::sync::Mutex` in main.rs causes unnecessary contention

**Severity**: Medium
**File**: `src/main.rs:35`

```rust
let kernel = Arc::new(Mutex::new(Kernel::new()));
```

Uses `tokio::sync::Mutex` which holds the lock across `.await` points in
every gRPC handler. Since `Kernel` methods are all synchronous (`&mut self`),
this means every gRPC call serializes through a single async mutex. Under
concurrent load, this will become the bottleneck.

**Consideration**: The single-actor model is documented as intentional. If
concurrency is needed, consider either:
- `parking_lot::Mutex` (already a dependency) for sync-only critical sections
- A channel-based actor model (mpsc → kernel task)
- Sharded state for read-heavy paths

---

### ISSUE-05: No CI/CD pipeline

**Severity**: Medium
**Files**: None (missing)

There are no CI configuration files (`.github/workflows/`, `.gitlab-ci.yml`,
`Makefile`, `justfile`). The README documents manual commands (`cargo test`,
`cargo clippy`, `cargo tarpaulin`) but nothing automates them on push/PR.

**Impact**: No automated regression protection, no enforced clippy/fmt
checks, no coverage gating.

**Fix**: Add a GitHub Actions workflow with at minimum:
- `cargo fmt -- --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- Coverage reporting (optional)

---

### ISSUE-06: `Cargo.lock` is not committed

**Severity**: Medium
**File**: `.gitignore` excludes `Cargo.lock`

For a binary/application crate (which this is — it produces
`jeeves-kernel`), Rust convention is to commit `Cargo.lock` for reproducible
builds. The `.gitignore` currently excludes it.

**Fix**: Remove `Cargo.lock` from `.gitignore` and commit it.

---

### ISSUE-07: License inconsistency — Cargo.toml says MIT, LICENSE file is Apache 2.0

**Severity**: Medium
**Files**:
- `Cargo.toml:8` — `license = "MIT"`
- `LICENSE` — Full text of Apache License 2.0
- `README.md` — "Apache License 2.0"

The Cargo.toml metadata states MIT, but the actual LICENSE file and README
both say Apache 2.0. This is a legal inconsistency that should be resolved.

**Fix**: Align `Cargo.toml` `license` field with the actual LICENSE file.

---

### ISSUE-08: 22 clippy warnings in library code

**Severity**: Low-Medium
**Breakdown**:

| Warning | Count | Impact |
|---|---|---|
| Redundant closure | 9 | Style — `.map_err(Error::internal)` vs `.map_err(\|e\| Error::internal(e))` |
| `or_insert_with` for default values | 2 | Should be `or_default()` |
| Large `Err` variant | 2 | `Error` enum size could be reduced by boxing large variants |
| Unnecessary `.to_string()` in `format!` | 2 | Minor efficiency |
| Too many function arguments | 2 | `create_interrupt` (9 args), `create_interrupt` in InterruptService (11 args) |
| Missing `Debug` impl | 1 | `CleanupService` |
| Unused import (`UnwindSafe`) | 1 | Dead import in `recovery.rs` |
| Complex type | 1 | `query_handlers` map type in `commbus` |
| Derivable `impl` | 1 | Manual Default that could be `#[derive(Default)]` |
| Field assignment outside initializer | 1 | Style |

**Fix**: Address these to achieve a clean `cargo clippy` pass.

---

### ISSUE-09: `ServiceRegistry::dispatch` is a placeholder

**Severity**: Low-Medium
**File**: `src/kernel/services.rs:367-404`

The dispatch method checks service existence and capacity but returns a
hardcoded success result without invoking any handler. The `handlers` field
stores `HashMap<String, usize>` (placeholder) instead of actual
`ServiceHandler` closures.

**Impact**: The `dispatch` gRPC endpoint will always return success without
doing any work. The `ServiceHandler` type alias exists but is unused.

**Fix**: Either implement actual handler invocation or document this as a
known stub in the API.

---

### ISSUE-10: Envelope tests are missing

**Severity**: Medium
**Files**:
- `src/envelope/mod.rs` — 0 tests for Envelope struct methods
- `src/envelope/enums.rs` — 0 tests for enum behavior
- `src/envelope/export.rs` — stub, 0 tests
- `src/envelope/import.rs` — stub, 0 tests

The Envelope is the **core state container** flowing through the entire
pipeline, yet it has zero unit tests. Methods like `start_stage()`,
`complete_stage()`, `fail_stage()`, `at_limit()`, `terminate()`,
`set_interrupt()`, `clear_interrupt()` are all untested.

The reported coverage (53.85% for `envelope/mod.rs`) comes only from other
modules' tests that incidentally exercise Envelope construction.

**Fix**: Add targeted unit tests for all Envelope methods and enum behavior.

---

### ISSUE-11: `envelope/import.rs` has 0% coverage — `from_json` panics

**Severity**: Low (stub)
**File**: `src/envelope/import.rs`

Coverage report shows 0% for this module. The `deserialize_int_from_float`
helper has no tests.

**Fix**: When `from_json` is implemented, add tests. In the meantime, add
tests for `deserialize_int_from_float` since it contains real logic.

---

### ISSUE-12: `recovery.rs` has low coverage (35.48%)

**Severity**: Low
**File**: `src/kernel/recovery.rs`

The `with_recovery_async` function's panic branch is hard to test because
Tokio's runtime catches panics in spawned tasks. The 2 ignored doc-tests
also reduce coverage.

**Fix**: Add tests that exercise the async panic recovery path (e.g., using
`std::panic::set_hook` or testing synchronous panic wrapping). Fix or
remove ignored doc-tests.

---

### ISSUE-13: `ProcessControlBlock::start()` and `block()` don't validate state transitions

**Severity**: Low
**File**: `src/kernel/types.rs:302-331`

The PCB methods `start()`, `complete()`, `block()`, and `wait()` mutate
state directly without checking `can_transition_to()`. The validation only
happens at the `LifecycleManager` level. This means direct PCB manipulation
(e.g., in tests or future code) can create invalid state machines.

The `can_transition_to()` method exists but is never called outside of
tests.

**Fix**: Either enforce transitions in PCB methods (returning `Result`), or
mark these methods `pub(crate)` to prevent external misuse.

---

### ISSUE-14: `validation.rs` is unused and gated behind `#![allow(dead_code)]`

**Severity**: Low
**File**: `src/validation.rs`

The file-level `#![allow(dead_code)]` suppresses warnings for two
validation helpers (`validate_non_empty`, `validate_positive`) that are
never called. The gRPC layer performs its own inline validation instead.

**Fix**: Either integrate these validators into the gRPC boundary or remove
the file. The `validator` crate (already a dependency) provides derive
macros that would be more idiomatic.

---

### ISSUE-15: gRPC integration tests are shallow

**Severity**: Low
**File**: `tests/grpc_integration.rs`

The 11 integration tests only verify:
- Service struct instantiation
- Shared kernel state reference
- Proto type constructability

They don't test any actual gRPC call/response behavior. No tests verify
that `create_process`, `get_process`, `publish`, `subscribe`, etc. work
through the gRPC layer.

**Fix**: Add actual gRPC client-server integration tests using `tonic`'s
test utilities or an in-process server.

---

### ISSUE-16: `main.rs` binds to `[::1]:50051` (IPv6 loopback only)

**Severity**: Low
**File**: `src/main.rs:44`

```rust
let addr = "[::1]:50051".parse()?;
```

This binds to IPv6 loopback only. In environments without IPv6 (some
containers, CI runners), the server won't be reachable. The `Config`
struct's default (`127.0.0.1:50051`) is never actually used — main.rs
hardcodes the address.

**Fix**: Use the `Config` struct's `grpc_addr` field, or bind to
`0.0.0.0:50051` / `[::]:50051` for dual-stack.

---

### ISSUE-17: `Config` struct is never loaded from files/env

**Severity**: Low
**Files**: `src/main.rs:29`, `src/types/config.rs`

`main.rs` creates `Config::default()` and immediately discards it
(`let _config = Config::default()`). The `config` crate dependency exists
but is never used to load configuration from environment variables or
files.

**Fix**: Either wire up config loading or remove the `config` crate
dependency.

---

## Coverage Gap Analysis

| Module | Reported % | Lines | Key Gaps |
|---|---|---|---|
| `commbus/mod.rs` | 90.74% | 98/108 | Disconnected subscriber cleanup |
| `kernel/interrupts.rs` | 91.45% | 107/117 | Edge cases in expiry |
| `kernel/resources.rs` | 90.91% | 20/22 | `total_usage` edge cases |
| `kernel/cleanup.rs` | 86.36% | 38/44 | Async cleanup loop body |
| `kernel/lifecycle.rs` | 86.84% | 66/76 | `list()`, `list_by_state()` edge cases |
| `kernel/orchestrator.rs` | 87.91% | 80/91 | Routing rule evaluation |
| `kernel/services.rs` | ~80% | — | Dispatch handler invocation |
| `envelope/mod.rs` | 53.85% | 21/39 | All methods untested directly |
| `envelope/export.rs` | 0% | 0/1 | Stub |
| `envelope/import.rs` | 0% | 0/2 | Stub |
| `kernel/recovery.rs` | 35.48% | 11/31 | Async panic path |
| `grpc/*` | ~60-70% | — | No end-to-end gRPC tests |
| `types/*` | ~90%+ | — | Error conversion branches |
| `validation.rs` | 0% | 0/2 | Never called |

---

## Summary of Issues by Priority

| Priority | Count | Issues |
|---|---|---|
| Error/Blocker | 1 | ISSUE-01 |
| High | 1 | ISSUE-02 |
| Medium | 6 | ISSUE-03, 04, 05, 06, 07, 10 |
| Low-Medium | 2 | ISSUE-08, 09 |
| Low | 7 | ISSUE-11, 12, 13, 14, 15, 16, 17 |

---

## Strengths

- **Zero unsafe code** enforced at compile time
- **Strict clippy config** with `unwrap_used = "deny"` (despite the one violation)
- **Well-organized module structure** with clear responsibilities
- **Comprehensive kernel tests** (lifecycle, resources, interrupts, rate limiter, orchestrator)
- **CommBus fully implemented and well-tested** (90.74% coverage)
- **Strong type system** — newtype IDs prevent ID confusion
- **Error types map cleanly to gRPC status codes**
- **Panic recovery** provides fault isolation for production
- **Background cleanup** prevents memory leaks in long-running deployments
