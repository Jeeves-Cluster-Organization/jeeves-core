# Jeeves-Core Rust Microkernel Audit

**Date**: 2026-02-06
**Scope**: Repository-internal code, tests, coverage, build tooling
**Out of scope**: External Python consumers, deployment infrastructure

---

## Executive Summary

`jeeves-core` is a ~8,400 LOC Rust microkernel for multi-agent pipeline orchestration, exposing 4 gRPC services. It was ported from Go. The codebase is well-structured, compiles cleanly (`0 clippy warnings` on lib), passes all **107 tests** (96 unit + 11 integration), and reports **81.36% line coverage**. `#![deny(unsafe_code)]` is enforced.

The audit identified **19 trackable issues** across 5 categories, none of which expand scope beyond the repository. All are improvements to correctness, test quality, or operational readiness of the existing code.

---

## 1. Build & Tooling

| Item | Status |
|------|--------|
| `cargo build` | Pass (requires `protoc` — not documented in README quick-start) |
| `cargo test` | **107 pass**, 0 fail, 2 doc-tests ignored |
| `cargo clippy --lib` | **0 warnings** |
| `cargo clippy --all-targets` | 189 warnings (all `unwrap_used` in test code) |
| CI/CD pipeline | **Missing entirely** |
| `Cargo.lock` | Not committed (correct for library crate) |
| `rustfmt.toml` | Missing (using defaults) |
| `rust-toolchain.toml` | Missing (no pinned toolchain, MSRV 1.75 documented but not enforced) |

---

## 2. Code Quality Findings

### 2.1 Correctness Issues

**AUDIT-01: Lossy integer casts in proto conversions (`conversions.rs:125-148`)**
`tokens_in` and `tokens_out` are `i64` in domain types but `i32` in proto. The conversion uses `as i32` / `as i64` which silently truncates values exceeding `i32::MAX` (~2.1B tokens). Same pattern in `AgentExecutionMetrics` (`conversions.rs:623-638`) for `duration_ms`. These should use `TryFrom` or at minimum document the assumption that values stay within i32 range.

**AUDIT-02: Hardcoded rate limit values in `kernel_service.rs:295-301`**
`check_rate_limit` returns `limit: 60`, `retry_after_seconds: 60.0`, and `remaining: 60 - current_count` — all hardcoded. These should reflect the actual `RateLimitConfig` values. If the config is changed, the gRPC response will be wrong.

**AUDIT-03: `record_usage` in `kernel_service.rs:230-261` updates per-user aggregates, not the PCB**
The gRPC handler calls `kernel.record_usage()` which delegates to `resources.record_usage()` — this updates a per-user aggregate in `ResourceTracker`. But then it reads back `pcb.usage` which is the process-level PCB usage and has NOT been updated. The returned `ResourceUsage` may not reflect the recorded usage.

**AUDIT-04: `wait_process` and `resume_process` use `pid.as_str()` to look up envelopes (`kernel/mod.rs:198,208`)**
Envelopes are keyed by `envelope_id` (a string), but these methods look up using `pid.as_str()`. This only works if `envelope_id == pid`, which is not guaranteed by the type system or documented as an invariant.

**AUDIT-05: `InterruptResponse` conversion: `approved` field loses `None` vs `false` distinction (`conversions.rs:538`)**
`if proto.approved { Some(true) } else { None }` — this maps `approved=false` to `None`, but there's a semantic difference between "not answered" and "denied". Proto `false` and absent both map to `None`.

### 2.2 Stub / Incomplete Implementations

**AUDIT-06: `observability.rs` — `init_tracing()` and `init_metrics()` are empty stubs**
The server binary calls neither function, and both are TODO placeholders. The system has `tracing`, `opentelemetry`, and `prometheus` as dependencies but none are wired up.

**AUDIT-07: `envelope/import.rs` and `envelope/export.rs` — always return `Err`**
`from_json()` and `to_json()` unconditionally return an error. These are referenced as "checkpoint 2" but never completed. Dead code that inflates dependency surface if unused.

**AUDIT-08: `CommBusService` gRPC handlers are placeholders (`grpc/commbus_service.rs`)**
All 4 RPC methods (`Publish`, `Send`, `Query`, `Subscribe`) return hardcoded success without interacting with the actual `CommBus` in-memory bus. The kernel owns a real `CommBus` with full pub/sub implementation, but the gRPC layer doesn't use it.

**AUDIT-09: `EngineService::execute_agent` is a no-op (`grpc/engine_service.rs:253-285`)**
Returns `success: true` with zero metrics and the original envelope unchanged. No agent execution occurs.

**AUDIT-10: `get_process_counts` has `queue_depth: 0` TODO (`kernel_service.rs:351`)**
Hardcoded to 0 with a TODO comment. The lifecycle manager has the data to compute this.

### 2.3 Architectural Observations

**AUDIT-11: Global `Arc<Mutex<Kernel>>` serializes all gRPC calls**
Every gRPC handler acquires the mutex for the entire operation. Under concurrent load, all 4 services contend on a single lock. This is acceptable now (single-process design) but will become the bottleneck under production load. The `CommBus` module internally uses `Arc<RwLock<...>>` for fine-grained concurrency, but this is negated by the outer `Mutex<Kernel>`.

**AUDIT-12: Kernel struct fields are `pub` (`kernel/mod.rs:48-69`)**
All subsystems (`lifecycle`, `resources`, `rate_limiter`, `interrupts`, `services`, `orchestrator`, `commbus`) are public fields. gRPC handlers reach through the kernel to access subsystems directly (e.g., `kernel.lifecycle.schedule()`, `kernel.rate_limiter.check_rate_limit()`), bypassing the Kernel's own delegation methods. This weakens encapsulation.

---

## 3. Test Quality Findings

### 3.1 Test Architecture

- **96 unit tests** in `src/` modules (inline `#[cfg(test)]` blocks)
- **11 integration tests** in `tests/grpc_integration.rs`
- **2 ignored doc-tests** (envelope example, recovery example)
- **0 property-based tests** (`proptest` is a dev-dep but unused)
- **0 benchmarks** (`criterion` is a dev-dep but unused)
- **0 gRPC-level tests** (integration tests only verify service instantiation, not actual RPC calls)

### 3.2 Coverage Gaps

**AUDIT-13: No tests for any gRPC RPC handler logic**
The 11 integration tests verify struct construction and shared-kernel compilation, but zero tests make actual gRPC calls (via `tonic::transport` or direct trait invocation). All the logic in `kernel_service.rs`, `engine_service.rs`, `orchestration_service.rs`, and `commbus_service.rs` is untested at the handler level.

**AUDIT-14: `envelope/mod.rs` at 53.85% coverage — state machine undertested**
The `Envelope` struct has methods like `terminate()`, `set_interrupt()`, `clear_interrupt()`, `complete_stage()`, `advance_stage()` — several of which are only exercised indirectly through orchestrator tests. No dedicated unit tests for envelope state transitions.

**AUDIT-15: Doc-test for `Envelope::new()` example is `#[ignore]`d (`envelope/mod.rs:231`)**
The example in the doc comment is marked ignored. If the API changes, this example will silently rot.

**AUDIT-16: Test code has 189 clippy `unwrap_used` warnings**
While `unwrap()` in tests is common, the project's own `Cargo.toml` sets `unwrap_used = "warn"`. This means the project's own lint policy is violated in every test file. Either relax the lint for tests or use `expect()` with context messages.

### 3.3 Missing Test Categories

**AUDIT-17: No negative/adversarial tests for proto conversions**
`conversions.rs` (657 lines) has no tests. Invalid timestamps, overflowing integers, missing required fields, and malformed JSON bytes are all plausible gRPC inputs that are unverified.

---

## 4. Operational Readiness

**AUDIT-18: No CI/CD configuration**
No `.github/workflows/`, `.gitlab-ci.yml`, or equivalent. The build requires `protoc` as a system dependency — this is a common source of CI failures and is not documented in the quick-start section of README.md.

**AUDIT-19: `build.rs` generates both client and server code (`build_client(true)`)**
The binary only needs the server stubs. Generating client code adds compile time and binary size for unused code. This is minor but worth noting.

---

## 5. Issue Catalog

Below is a prioritized list of all findings, suitable for direct issue tracking.

### P0 — Correctness (fix before relying on these paths)

| ID | Title | File(s) | Type |
|----|-------|---------|------|
| AUDIT-01 | Lossy `as i32`/`as i64` casts in proto conversions | `conversions.rs` | Bug |
| AUDIT-02 | Hardcoded rate limit values in gRPC response | `kernel_service.rs` | Bug |
| AUDIT-03 | `record_usage` updates user aggregates but returns stale PCB usage | `kernel_service.rs` | Bug |
| AUDIT-04 | Envelope lookup by `pid.as_str()` assumes `envelope_id == pid` | `kernel/mod.rs` | Bug |
| AUDIT-05 | `approved=false` and absent `approved` both map to `None` | `conversions.rs` | Bug |

### P1 — Test Coverage (blocks confidence in correctness)

| ID | Title | File(s) | Type |
|----|-------|---------|------|
| AUDIT-13 | No tests for gRPC RPC handler logic | `grpc/*.rs` | Test gap |
| AUDIT-14 | Envelope state machine undertested (53% coverage) | `envelope/mod.rs` | Test gap |
| AUDIT-17 | No negative/adversarial tests for proto conversions | `conversions.rs` | Test gap |
| AUDIT-15 | Ignored doc-test for Envelope example | `envelope/mod.rs` | Test gap |
| AUDIT-16 | 189 clippy `unwrap_used` warnings in test code | `tests/`, `src/**/tests` | Code quality |

### P2 — Stubs & Incomplete (functional gaps)

| ID | Title | File(s) | Type |
|----|-------|---------|------|
| AUDIT-06 | Observability stubs (`init_tracing`, `init_metrics`) never called | `observability.rs` | Incomplete |
| AUDIT-07 | `from_json`/`to_json` always return error | `envelope/import.rs`, `export.rs` | Dead code |
| AUDIT-08 | CommBusService gRPC handlers don't use actual CommBus | `grpc/commbus_service.rs` | Incomplete |
| AUDIT-09 | `execute_agent` is a no-op | `grpc/engine_service.rs` | Incomplete |
| AUDIT-10 | `queue_depth` hardcoded to 0 | `kernel_service.rs` | Incomplete |

### P3 — Operational & Hygiene

| ID | Title | File(s) | Type |
|----|-------|---------|------|
| AUDIT-18 | No CI/CD configuration | (repo root) | Infra |
| AUDIT-19 | `build.rs` generates unused gRPC client stubs | `build.rs` | Optimization |
| AUDIT-11 | Global mutex serializes all gRPC calls | `kernel/mod.rs`, `grpc/*.rs` | Architecture |
| AUDIT-12 | Public kernel fields bypass encapsulation | `kernel/mod.rs` | Architecture |

---

## 6. Recommendations (Scoped to This Repo)

1. **Fix P0 bugs first** — AUDIT-01 through AUDIT-05 are low-effort, high-value corrections.
2. **Add gRPC handler tests (AUDIT-13)** — Call the trait methods directly with constructed `Request<T>` values. This is the single highest-leverage test improvement.
3. **Add proto conversion tests (AUDIT-17)** — Especially for boundary values (`i32::MAX` tokens, `0` timestamps, empty strings).
4. **Set up CI (AUDIT-18)** — Even a minimal `cargo test && cargo clippy` workflow prevents regressions.
5. **Decide on stubs (P2)** — Either implement the placeholder code or remove the dead dependencies/code to reduce surface area.
6. **Defer P3 items** — Lock contention (AUDIT-11) and encapsulation (AUDIT-12) are design decisions best revisited when profiling data or multi-service requirements emerge.

---

*Generated by audit session 2026-02-06*
