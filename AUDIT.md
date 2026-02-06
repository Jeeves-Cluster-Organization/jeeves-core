# Jeeves-Core Audit Report

**Date**: 2026-02-06
**Scope**: Rust microkernel codebase only (no external Python consumers)
**Branch**: `claude/audit-rust-microkernel-CQwHf`

---

## Executive Summary

The Jeeves-Core microkernel is a well-structured Rust implementation of a
multi-agent orchestration runtime. The codebase demonstrates sound
architectural principles: zero unsafe code, strongly-typed IDs, clear module
boundaries, and a single-actor ownership model for mutable state.

### Current State (post-fix)

| Metric | Before | After |
|---|---|---|
| Clippy errors (lib) | 1 | **0** |
| Clippy warnings (lib) | 22 | **0** |
| All tests passing | 107/107 | **107/107** |
| `unimplemented!()` panics | 2 | **0** (return `Err`)  |
| License consistency | Mismatch | **Aligned** (Apache-2.0) |
| Config wired into main | No | **Yes** |
| `Cargo.lock` committed | No | **Yes** |
| Dead code (`validation.rs`) | Present | **Removed** |
| PCB state methods visibility | `pub` | **`pub(crate)`** |

---

## Fixes Applied

### ISSUE-01: FIXED — Clippy `unwrap_used` violation in `interrupts.rs`

Replaced `*stats.get_mut(key).unwrap() += 1` with safe `if let Some(count)` access.

### ISSUE-02: PARTIALLY FIXED — Stub implementations no longer panic

`envelope/export.rs::to_json()` and `envelope/import.rs::from_json()` now
return `Err(Error::internal("not yet implemented"))` instead of
`unimplemented!()`. Callers already handle `Result`, so no API change.

`observability.rs` remains a no-op (needs design decisions — see ISSUE-03).

### ISSUE-06: FIXED — `Cargo.lock` committed

Removed `Cargo.lock` from `.gitignore`. Lockfile is now tracked for
reproducible builds (Rust convention for binary crates).

### ISSUE-07: FIXED — License aligned to Apache-2.0

Changed `Cargo.toml` `license` field from `"MIT"` to `"Apache-2.0"` to match
the LICENSE file and README.

### ISSUE-08: FIXED — All 22 clippy warnings resolved

| Warning | Fix |
|---|---|
| 9 redundant closures | Replaced with function references (e.g., `.map(i32::from)`) |
| 2 `or_insert_with(Vec::new)` | Changed to `or_default()` |
| 2 large `Err` variant (`Status`) | Changed `parse_pid` to return `Error` (smaller), not `Status` |
| 2 unnecessary `.to_string()` in `format!` | Removed redundant conversion |
| 2 too many function arguments | Introduced `CreateInterruptParams` struct |
| 1 missing `Debug` | Added `#[derive(Debug)]` to `CleanupService` |
| 1 unused import (`UnwindSafe`) | Removed |
| 1 complex type | Added `QueryHandlerSender` type alias |
| 1 derivable Default | Changed to `#[derive(Default)]` with `#[default]` attribute |
| 1 field assignment outside initializer | Restructured to use struct literal |

Lint strategy: `unwrap_used = "warn"` in `Cargo.toml` (applies globally
including tests, where unwrap is idiomatic). Library code has zero unwrap
calls. CI should enforce via `cargo clippy -- -D warnings` on lib target.

### ISSUE-13: FIXED — PCB mutation methods restricted to `pub(crate)`

`start()`, `complete()`, `block()`, `wait()`, and `resume()` are now
`pub(crate)`. External consumers must go through `LifecycleManager` which
validates state transitions.

### ISSUE-14: FIXED — Unused `validation.rs` removed

Deleted the file and removed `mod validation;` from `lib.rs`. The gRPC
layer handles its own boundary validation. The `validator` crate remains
available as a dependency for future use.

### ISSUE-16 + ISSUE-17: FIXED — Config wired into `main.rs`

`main.rs` now uses `config.server.grpc_addr` instead of hardcoded
`[::1]:50051`. Default bind address is `127.0.0.1:50051` from
`ServerConfig::default()`. Config is still constructed via `Config::default()`
(file/env loading is a separate feature — see remaining issues).

---

## Remaining Issues (not addressed in this pass)

### ISSUE-03: Observability is not wired up

**Severity**: Medium
**Status**: Open — requires design decisions (OTLP vs stdout, filter levels,
metrics endpoint)

### ISSUE-04: `tokio::sync::Mutex` contention

**Severity**: Medium
**Status**: Open — documented as intentional single-actor model. Revisit if
profiling shows contention under real load.

### ISSUE-05: No CI/CD pipeline

**Severity**: Medium
**Status**: Open — should be a separate PR. Recommended minimum:
```yaml
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test
```

### ISSUE-09: `ServiceRegistry::dispatch` is a placeholder

**Severity**: Low-Medium
**Status**: Open — returns hardcoded success without invoking handlers.
Needs handler registration design.

### ISSUE-10: Envelope tests are missing

**Severity**: Medium
**Status**: Open — the Envelope is the core pipeline state container with
zero direct unit tests. Should be addressed in a dedicated test PR.

### ISSUE-11: `envelope/import.rs` has 0% coverage

**Severity**: Low
**Status**: Open — `from_json` is now a safe stub (returns `Err`).
`deserialize_int_from_float` still untested.

### ISSUE-12: `recovery.rs` has low coverage (35.48%)

**Severity**: Low
**Status**: Open — async panic path hard to test. 2 ignored doc-tests.

### ISSUE-15: gRPC integration tests are shallow

**Severity**: Low
**Status**: Open — tests verify instantiation only, not actual RPC behavior.

---

## Coverage Gap Analysis

| Module | Reported % | Key Gaps |
|---|---|---|
| `commbus/mod.rs` | 90.74% | Disconnected subscriber cleanup |
| `kernel/interrupts.rs` | 91.45% | Edge cases in expiry |
| `kernel/resources.rs` | 90.91% | `total_usage` edge cases |
| `kernel/cleanup.rs` | 86.36% | Async cleanup loop body |
| `kernel/lifecycle.rs` | 86.84% | `list()`, `list_by_state()` edge cases |
| `kernel/orchestrator.rs` | 87.91% | Routing rule evaluation |
| `kernel/services.rs` | ~80% | Dispatch handler invocation |
| `envelope/mod.rs` | 53.85% | All methods untested directly |
| `envelope/export.rs` | 0% | Stub (now safe) |
| `envelope/import.rs` | 0% | Stub (now safe) |
| `kernel/recovery.rs` | 35.48% | Async panic path |
| `grpc/*` | ~60-70% | No end-to-end gRPC tests |
| `types/*` | ~90%+ | Error conversion branches |

---

## Strengths

- **Zero unsafe code** enforced at compile time
- **Zero clippy warnings** on library code (post-fix)
- **Well-organized module structure** with clear responsibilities
- **Comprehensive kernel tests** (lifecycle, resources, interrupts, rate limiter, orchestrator)
- **CommBus fully implemented and well-tested** (90.74% coverage)
- **Strong type system** — newtype IDs prevent ID confusion
- **Error types map cleanly to gRPC status codes**
- **Panic recovery** provides fault isolation for production
- **Background cleanup** prevents memory leaks in long-running deployments
