# Jeeves Core — Comprehensive Repository Audit

**Date**: 2026-02-23
**Branch**: `claude/repo-audit-assessment-Lt4MD`
**Commit**: `0491bf1`

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Project Identity](#2-project-identity)
3. [Repository Evolution](#3-repository-evolution)
4. [Architecture & Design](#4-architecture--design)
5. [Technology Stack](#5-technology-stack)
6. [Capability Assessment](#6-capability-assessment)
7. [Code Quality Audit](#7-code-quality-audit)
8. [Security Assessment](#8-security-assessment)
9. [Test & Coverage Assessment](#9-test--coverage-assessment)
10. [DevOps & Operational Readiness](#10-devops--operational-readiness)
11. [Documentation Assessment](#11-documentation-assessment)
12. [Risk Register](#12-risk-register)
13. [Maturity Assessment](#13-maturity-assessment)
14. [Recommendations](#14-recommendations)

---

## 1. Executive Summary

**Jeeves Core** is a Rust micro-kernel for AI agent orchestration. It provides
Unix-like process lifecycle management, resource quota enforcement, human-in-the-loop
interrupt handling, pipeline orchestration, and inter-process communication via
TCP+msgpack.

| Metric | Value |
|--------|-------|
| Language | Rust (Edition 2021) |
| Lines of Code | ~7,700 (31 source files) |
| Tests | 96 passing |
| Coverage | 81.36% (576/708 lines) |
| Unsafe Code | Zero (`#![deny(unsafe_code)]`) |
| Dependencies | 31 direct, 317 total |
| Age | 19 days (Jan 25 – Feb 12, 2026) |
| Contributors | 3 (Emper0r: 43 commits, Claude AI: 41 commits, Shahbaz Shaik: 10 commits) |
| Total Commits | 94 |
| License | Apache 2.0 |

### Overall Assessment

| Dimension | Rating | Notes |
|-----------|--------|-------|
| Architecture | **Strong** | Clean micro-kernel design, Unix-like process model |
| Code Quality | **Good** | Zero unsafe, no unwrap/panic, proper error types |
| Security | **Needs Work** | Input validation gaps, unbounded connections |
| Testing | **Good** | 81% coverage, property-based + snapshot testing |
| DevOps | **Not Started** | No CI/CD, no Docker, no deployment automation |
| Documentation | **Good** | Constitution, coverage report, analysis docs |
| Production Readiness | **Not Yet** | Missing CI, observability stubs, security hardening |

---

## 2. Project Identity

### What It Is

A **micro-kernel** that provides minimal, essential primitives for AI agent
orchestration. It sits at the bottom of a three-layer architecture:

```
Capabilities  ─────────────────────────  (LLM tools, prompts, domain logic)
     ↑
Infrastructure ────────────────────────  (LLM integrations, databases, APIs)
     ↑  IPC
Kernel ────────────────────────────────  ← jeeves-core (THIS REPO)
```

### What It Provides

- **Process Lifecycle** — Unix-like state machine (New → Ready → Running → Blocked → Terminated → Zombie)
- **Resource Quotas** — Defense-in-depth bounds on LLM calls, tokens, hops, iterations
- **Interrupt Handling** — Human-in-the-loop patterns (Clarification, Confirmation, AgentReview, etc.)
- **IPC Services** — TCP+msgpack transport with 26 RPC methods across 4 services
- **CommBus** — Kernel-mediated message bus (pub/sub events, commands, queries)
- **Pipeline Orchestration** — Routing rules, stage transitions, bounds enforcement

### What It Does NOT Provide

- LLM integrations (infrastructure layer)
- Tool implementations (capability layer)
- Prompt templates (capability layer)
- Database access (infrastructure layer)

### Organization

- **GitHub Org**: Jeeves-Cluster-Organization
- **Repo**: jeeves-core
- **Remote**: `origin/main`

---

## 3. Repository Evolution

### Timeline (19 Days, 3 Language Eras)

```
Jan 25-27    Jan 28-29      Feb 1-2        Feb 3-7         Feb 8-12
─────────────────────────────────────────────────────────────────────
 Python       Python→Go      Go→Rust        Rust            Rust IPC
 Gateway      Migration      Rewrite        Hardening       Pivot
 Refactor                                   + Analysis      gRPC→TCP
```

#### Era 1: Python Origins (Jan 25–27)
- Gateway refactoring with Strategy Pattern
- Module renaming, unit tests, LiteLLM integration
- 7 disciplined commits on gateway extraction

#### Era 2: Python-to-Go Migration (Jan 28–29)
- Rapid 2-day migration of enums, envelope, config types
- Python packages removed to `jeeves-infra`
- Go kernel with CommBusService gRPC

#### Era 3: Go-to-Rust Rewrite (Feb 1–12)
- 16 commits in ~3 hours for the initial rewrite (Claude-driven)
- Phased approach: scaffold → envelope → kernel → gRPC → tests
- Post-rewrite hardening: macros, newtypes, envelope decomposition
- Architecture pivot: gRPC → IPC TCP+msgpack (deleted `engine.proto`)
- Rate limiter, lifecycle events, process cleanup added

### Branching Strategy

**Trunk-based development** with short-lived feature branches:
- Feature branches: `claude/<task-description>-<id>`
- All merges via GitHub pull requests (PRs #38–#45 merged)
- Clean merge commits, no rebasing

### Contributor Profile

| Contributor | Commits | Role |
|-------------|---------|------|
| Emper0r | 43 (46%) | Architecture decisions, integration, Python/Go phases |
| Claude (AI) | 41 (44%) | Rust rewrite, analysis, audits, test expansion |
| Shahbaz Shaik | 10 (10%) | Contributions during Python/Go phases |

### Key Observations

- **Two complete language rewrites in 19 days** — project is in early/experimental phase
- **AI-assisted development is a first-class workflow** — 44% of commits by Claude
- **Velocity prioritized over stability** — rapid iteration to find right stack
- **Rust appears to be the settled choice** given substantial follow-up hardening

---

## 4. Architecture & Design

### Kernel Model: Single Actor

```rust
Arc<Mutex<Kernel>>
```

All state mutations go through `&mut self` methods on the `Kernel` struct. No
internal actors or message-passing. The Mutex serializes all access.

**Trade-offs:**
- (+) Simple reasoning about state
- (+) No deadlock risk (single lock)
- (+) No data races by construction
- (−) Serialized access = bottleneck under high concurrency
- (−) Single point of contention

### Module Map

```
src/
├── lib.rs                     # Public API, deny(unsafe_code)
├── main.rs                    # Binary entry point
├── observability.rs           # Tracing init (STUB)
├── kernel/
│   ├── mod.rs                 # Kernel struct, delegates to subsystems
│   ├── lifecycle.rs           # Process state machine + priority scheduler
│   ├── resources.rs           # Quota tracking + enforcement
│   ├── interrupts.rs          # Human-in-the-loop interrupt service
│   ├── rate_limiter.rs        # Sliding-window per-user rate limiting
│   ├── orchestrator.rs        # Pipeline orchestration + routing rules
│   ├── cleanup.rs             # Background GC for zombie processes
│   ├── recovery.rs            # Panic recovery wrapper
│   ├── services.rs            # Service registry + dispatch
│   └── types.rs               # ProcessControlBlock, ProcessState, etc.
├── ipc/
│   ├── server.rs              # TCP accept loop + connection handler
│   ├── codec.rs               # Msgpack framing (length-prefixed)
│   ├── dispatch.rs            # Request routing to handler modules
│   └── handlers/
│       ├── kernel.rs          # KernelService (12 RPCs)
│       ├── engine.rs          # EngineService (6 RPCs)
│       ├── orchestration.rs   # OrchestrationService (4 RPCs)
│       └── commbus.rs         # CommBusService (4 RPCs)
├── commbus/
│   └── mod.rs                 # Event pub/sub, commands, queries
├── envelope/
│   ├── mod.rs                 # Envelope with 6 semantic sub-structs
│   ├── enums.rs               # InterruptKind, TerminalReason, etc.
│   ├── import.rs              # JSON import (STUB)
│   └── export.rs              # JSON export (STUB)
└── types/
    ├── ids.rs                 # Strongly-typed IDs (ProcessId, etc.)
    ├── errors.rs              # Error enum with thiserror
    └── config.rs              # ServerConfig, IpcConfig, etc.
```

### Process Lifecycle State Machine

```
NEW ──→ READY ──→ RUNNING ──→ WAITING ──→ READY (resume)
                    │  │                     ↑
                    │  └──→ BLOCKED ─────────┘
                    │
                    └──→ TERMINATED ──→ ZOMBIE ──→ (removed by GC)
```

### IPC Protocol

- **Transport**: TCP
- **Encoding**: MessagePack with 4-byte length prefix (big-endian)
- **Max frame**: 16 MB (configurable, default in config is 50 MB)
- **Services**: 4 services, 26 total RPC methods

| Service | Methods | Purpose |
|---------|---------|---------|
| KernelService | 12 | Process CRUD, quota, rate limiting, system status |
| EngineService | 6 | Envelope CRUD, bounds checking, pipeline execution |
| OrchestrationService | 4 | Session management, instruction pipeline |
| CommBusService | 4 | Pub/sub events, commands, queries (with streaming) |

### CommBus Patterns

| Pattern | Delivery | Use Case |
|---------|----------|----------|
| Events (pub/sub) | Fan-out to all subscribers | State change notifications |
| Commands (fire-and-forget) | Single registered handler | Action delegation |
| Queries (request/response) | Single handler with timeout | Data retrieval |

---

## 5. Technology Stack

### Core

| Technology | Version | Purpose |
|------------|---------|---------|
| Rust | 2021 edition, 1.75+ MSRV | Language |
| Tokio | 1.41 | Async runtime (full features) |
| rmp-serde | 1.0 | MessagePack serialization |
| serde / serde_json | 1.0 | Serialization framework |
| thiserror | 2.0 | Error handling |
| anyhow | 1.0 | Flexible error context |

### Observability

| Technology | Version | Purpose |
|------------|---------|---------|
| tracing | 0.1 | Structured logging + spans |
| tracing-subscriber | 0.3 | Log output (env-filter, JSON) |
| opentelemetry | 0.27 | Distributed tracing |
| opentelemetry-otlp | 0.27 | OTLP exporter |
| prometheus | 0.13 | Metrics |

### Utilities

| Technology | Version | Purpose |
|------------|---------|---------|
| uuid | 1.11 | UUID v4 generation |
| chrono | 0.4 | Date/time handling |
| validator | 0.19 | Input validation |
| config | 0.14 | Configuration management |
| itertools | 0.14 | Iterator utilities |

### Testing

| Technology | Version | Purpose |
|------------|---------|---------|
| proptest | 1.6 | Property-based testing |
| mockall | 0.13 | Mocking framework |
| criterion | 0.5 | Benchmarking |
| insta | 1.41 | Snapshot/golden testing |
| pretty_assertions | 1.4 | Better assertion output |
| tokio-test | 0.4 | Async testing utilities |

### Build Profiles

```toml
[profile.release]
opt-level = 3, lto = "thin", codegen-units = 1, strip = true

[profile.dev]
opt-level = 0, debug = true

[profile.test]
opt-level = 1
```

---

## 6. Capability Assessment

### Fully Implemented

| Capability | Module | Maturity |
|------------|--------|----------|
| Process lifecycle (state machine) | `kernel::lifecycle` | Production-ready |
| Resource quota enforcement | `kernel::resources` | Production-ready |
| Interrupt handling (7 types) | `kernel::interrupts` | Production-ready |
| Priority-based scheduling | `kernel::lifecycle` | Production-ready |
| TCP+msgpack IPC server | `ipc::server` | Functional (needs hardening) |
| CommBus (events/commands/queries) | `commbus` | Production-ready |
| Pipeline orchestration | `kernel::orchestrator` | Production-ready |
| Rate limiting (sliding window) | `kernel::rate_limiter` | Functional |
| Zombie process cleanup (GC) | `kernel::cleanup` | Production-ready |
| Panic recovery | `kernel::recovery` | Production-ready |
| Service registry | `kernel::services` | Partial (dispatch is stub) |

### Partially Implemented / Stubs

| Capability | Status | Notes |
|------------|--------|-------|
| Observability (tracing init) | **Stub** | `init_tracing()` and `init_metrics()` are empty |
| Envelope import/export | **Stub** | Returns "not yet implemented" errors |
| Service dispatch | **Stub** | Registry works, actual dispatch returns Ok without invoking |
| Graceful shutdown | **Missing** | No signal handler (SIGTERM/SIGINT) |
| TLS/encryption | **Missing** | TCP transport is plaintext |
| Authentication | **Missing** | No auth on IPC connections |
| Connection limiting | **Missing** | `max_connections` config exists but is not enforced |

### Interrupt Types

| Type | TTL | Auto-expire | Requires Response |
|------|-----|-------------|-------------------|
| Clarification | 24h | Yes | Yes |
| Confirmation | 1h | Yes | Yes |
| AgentReview | 30m | Yes | Yes |
| Checkpoint | None | No | No |
| ResourceExhausted | 5m | Yes | No |
| Timeout | 5m | Yes | No |
| SystemError | 1h | Yes | No |

### Resource Quotas (13 dimensions)

- max_llm_calls, max_tool_calls, max_agent_hops, max_iterations
- max_input_tokens, max_output_tokens
- max_inference_calls, timeout_seconds
- rate_limit_per_minute, rate_limit_burst
- max_input_tokens (per-call), max_output_tokens (per-call)
- timeout computed live from elapsed time

---

## 7. Code Quality Audit

### Strengths

| Property | Status |
|----------|--------|
| Zero `unsafe` blocks | `#![deny(unsafe_code)]` enforced |
| Zero `unwrap()` in production paths | Clean |
| Zero `panic!()` macros | Clean |
| Zero TODO/FIXME/HACK in source | Clean |
| Error handling | All public functions return `Result<T>` |
| Type safety | Strongly-typed newtype IDs, exhaustive matching |
| Clippy compliance | Zero warnings (strict lint config) |

### Lint Configuration

```toml
[lints.rust]
unsafe_code = "deny"
missing_debug_implementations = "warn"

[lints.clippy]
unwrap_used = "warn"
expect_used = "warn"
panic = "warn"
large_enum_variant = "warn"
```

### Code Patterns

**Good patterns observed:**
- `define_id!` macro for strongly-typed newtype IDs
- Builder pattern for FlowInterrupt construction
- Semantic sub-struct decomposition (Envelope → 6 sub-structs)
- `with_recovery()` wrapper for panic isolation
- CommBus event emission after state changes

**Anti-patterns / concerns:**
- Single `Mutex<Kernel>` bottleneck under concurrency
- Some `#[allow(dead_code)]` markers on unused methods
- Repeated `.and_then().unwrap_or()` chains in IPC handlers (could use helper)
- Silent `unwrap_or_default()` on msgpack encoding failures

### Lines of Code by Module

| Module | LOC | % of Total |
|--------|-----|------------|
| kernel/ | ~3,100 | 40% |
| ipc/ | ~1,900 | 25% |
| commbus/ | ~880 | 11% |
| envelope/ | ~560 | 7% |
| types/ | ~320 | 4% |
| other (lib, main, observability) | ~40 | <1% |
| **tests/** | ~900 | 12% |

---

## 8. Security Assessment

### Critical Issues

| ID | Finding | Location | Risk |
|----|---------|----------|------|
| SEC-01 | **Unbounded TCP connections** — Server accepts unlimited connections despite `max_connections` config existing | `ipc/server.rs:40-59` | DDoS via connection exhaustion |
| SEC-02 | **No I/O timeouts** — `read_frame()` and `write_frame()` have no timeout; slow clients hold connections indefinitely | `ipc/server.rs:82-186` | Slowloris-style DoS |

### High Severity Issues

| ID | Finding | Location | Risk |
|----|---------|----------|------|
| SEC-03 | **Integer overflow in quota parsing** — i64→i32 cast without bounds checking; values >2^31 become negative, bypassing quotas | `ipc/handlers/kernel.rs:411-424` | Quota bypass |
| SEC-04 | **Negative usage values accepted** — No validation that usage metrics are non-negative | `ipc/handlers/kernel.rs:186-189` | Usage tracking manipulation |
| SEC-05 | **Unbounded memory growth** — Multiple HashMaps grow without cleanup: `Kernel::envelopes`, `ResourceTracker::user_usage`, `RateLimiter::user_windows` | Multiple locations | OOM crash over time |

### Medium Severity Issues

| ID | Finding | Location | Risk |
|----|---------|----------|------|
| SEC-06 | Large frame allocation (50MB default) without per-connection limits | `ipc/codec.rs:56` | Memory pressure |
| SEC-07 | No authentication or authorization on IPC | `ipc/server.rs` | Unauthorized access |
| SEC-08 | Plaintext TCP transport (no TLS) | `ipc/server.rs` | Data interception |
| SEC-09 | Rate limiter burst window overlaps with per-minute window | `kernel/rate_limiter.rs:78-91` | Burst circumvention |

### Positive Security Properties

- Zero unsafe code — memory safety guaranteed by compiler
- No SQL injection — no database layer
- No command injection — no shell execution
- Panic isolation — `with_recovery()` prevents crash propagation
- Quota enforcement at kernel level — capabilities cannot bypass
- Type-safe IDs prevent ID confusion attacks

---

## 9. Test & Coverage Assessment

### Overview

| Metric | Value | Target | Status |
|--------|-------|--------|--------|
| Total tests | 96 | 75+ | Exceeds |
| Line coverage | 81.36% | 75% | Exceeds (+6.36%) |
| Critical module coverage | 85%+ avg | 80% | Exceeds |
| Integration tests | 1 file (IPC round-trip) | — | Present |
| Property-based tests | Present (proptest) | — | Good |
| Snapshot tests | Present (insta) | — | Good |

### Coverage by Module

| Module | Coverage | Rating |
|--------|----------|--------|
| Interrupts | 91.45% | Excellent |
| CommBus | 90.74% | Excellent |
| Resources | 90.91% | Excellent |
| Orchestrator | 87.91% | Strong |
| Lifecycle | 86.84% | Strong |
| Cleanup | 86.36% | Strong |
| Kernel mod | 82.35% | Strong |
| Services | 82.81% | Strong |
| Rate Limiter | 80.00% | Good |
| Kernel Types | 74.51% | Adequate |
| Envelope | 53.85% | Moderate |
| Errors | 42.86% | Low (expected — constructors) |
| Recovery | 35.48% | Low (expected — panic paths) |
| Import | 0.00% | Uncovered (stub) |

### Testing Gaps

1. **No IPC server load/stress tests** — no validation of behavior under concurrent connections
2. **No fuzz testing** — malformed msgpack frames not tested
3. **No negative input testing** — integer overflow, negative values not tested at IPC boundary
4. **Envelope module at 53%** — complex state transitions undertested
5. **No benchmark results** — criterion configured but no benchmarks checked in

---

## 10. DevOps & Operational Readiness

### Current State: **No DevOps Infrastructure**

| Capability | Status |
|------------|--------|
| CI/CD Pipeline | **None** — no GitHub Actions, no workflows |
| Containerization | **None** — no Dockerfile |
| Container Orchestration | **None** — no docker-compose, Kubernetes manifests |
| Infrastructure-as-Code | **None** — no Terraform/CloudFormation/Pulumi |
| Build Automation | **Cargo only** — no Makefile, no scripts |
| Environment Management | **None** — no .env files, no configuration templates |
| Linting/Formatting Config | **None** — no rustfmt.toml, clippy.toml |
| Pre-commit Hooks | **None** |
| Deployment Scripts | **None** |
| Monitoring/Alerting | **None** — observability is stubbed |
| Secret Management | **None** |
| Release Process | **None** — no tags, no changelog |

### Impact

Without CI/CD:
- No automated testing on pull requests
- No automated security scanning
- No reproducible builds outside developer machines
- No deployment pipeline
- Manual testing is the only quality gate

---

## 11. Documentation Assessment

### Existing Documentation

| Document | Purpose | Quality |
|----------|---------|---------|
| `README.md` | Quick start guide | Good but references deleted `proto/` dir |
| `CONSTITUTION.md` | Architectural principles | Excellent — clear boundaries and criteria |
| `CONTRIBUTING.md` | Development workflow | Present |
| `ANALYSIS.md` | Rust improvement plan | Completed (historical reference) |
| `COVERAGE_REPORT.md` | Test coverage breakdown | Detailed and well-structured |
| `LICENSE` | Apache 2.0 | Standard |

### Documentation Gaps

1. **README references `proto/` directory** which no longer exists (gRPC was removed)
2. **No API documentation** — no rustdoc, no IPC protocol specification
3. **No deployment guide** — no instructions for production deployment
4. **No architecture decision records (ADRs)** — language migrations undocumented
5. **No runbook** — no operational procedures for incidents
6. **No changelog** — version history tracked only via git log

---

## 12. Risk Register

### Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| DDoS via connection flooding | High | High | Implement connection limits (SEC-01) |
| Memory exhaustion from unbounded HashMaps | Medium | High | Add periodic cleanup, capacity limits |
| Quota bypass via integer overflow | Medium | High | Validate input bounds at IPC layer |
| Slowloris attacks on TCP server | Medium | Medium | Add I/O timeouts |
| Single-Mutex contention under load | High | Medium | Consider RwLock or sharding |
| Data interception (no TLS) | Low* | High | Add TLS support (*depends on deployment) |

### Organizational Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| No CI means regressions go undetected | High | High | Set up GitHub Actions |
| 44% AI-authored code may have subtle bugs | Medium | Medium | Human review of all AI PRs |
| Two language rewrites suggest unstable direction | Medium | Medium | Lock on Rust (appears settled) |
| No release process means untracked deploys | High | Medium | Implement semantic versioning + tags |
| Single key contributor (Emper0r) | Medium | High | Document architecture decisions |

---

## 13. Maturity Assessment

Using a 5-level maturity model:

| Dimension | Level | Description |
|-----------|-------|-------------|
| **Architecture** | 4/5 | Well-designed micro-kernel with clear boundaries |
| **Code Quality** | 4/5 | Strong Rust idioms, zero unsafe, proper error handling |
| **Testing** | 3/5 | Good unit coverage but missing stress/fuzz/integration tests |
| **Security** | 2/5 | Type-safe core but input validation and transport security gaps |
| **DevOps** | 1/5 | No CI/CD, no containers, no deployment automation |
| **Observability** | 1/5 | Dependencies present but implementation is stubbed |
| **Documentation** | 3/5 | Good principles docs, missing operational docs |
| **Release Management** | 1/5 | No versioning, no tags, no changelog |

**Overall Maturity: Early Development (2.4/5)**

The codebase has strong architectural foundations and code quality but is missing
the operational infrastructure needed for production deployment.

---

## 14. Recommendations

### Priority 1: Security Hardening (Immediate)

1. **Enforce connection limits** — Add a semaphore or counter in `IpcServer::serve()` using `config.server.max_connections`
2. **Add I/O timeouts** — Wrap `read_frame()`/`write_frame()` with `tokio::time::timeout()`
3. **Validate integer bounds** — Check i64 values fit i32 range before casting in `parse_quota()` and `RecordUsage`
4. **Reject negative usage values** — Validate all usage metrics are >= 0 at IPC boundary
5. **Reduce default max_frame_bytes** — 50MB default is excessive; 4MB or 16MB is more appropriate

### Priority 2: CI/CD Setup (Short-term)

1. **GitHub Actions workflow** — cargo build, cargo test, cargo clippy, cargo fmt --check
2. **Automated coverage** — Run cargo-tarpaulin and fail if coverage drops below 80%
3. **Dependency audit** — Run `cargo audit` in CI for known vulnerabilities
4. **Branch protection** — Require CI pass + review before merge to main

### Priority 3: Operational Readiness (Medium-term)

1. **Implement observability** — Wire up `init_tracing()` and `init_metrics()` (currently stubs)
2. **Add graceful shutdown** — Signal handler for SIGTERM/SIGINT with connection draining
3. **Dockerfile** — Multi-stage build for minimal production image
4. **Health check endpoint** — Expose /health on metrics port
5. **Structured logging** — JSON logs with request correlation IDs

### Priority 4: Testing Improvements (Medium-term)

1. **Fuzz testing** — Use `cargo-fuzz` on the msgpack codec to find parsing bugs
2. **Load testing** — Stress test IPC server with concurrent connections
3. **Negative input testing** — Test integer overflow, malformed frames, huge payloads
4. **Improve envelope coverage** — Currently at 53%, target 80%+
5. **Add benchmarks** — Criterion is configured but no benchmarks are checked in

### Priority 5: Architecture Evolution (Longer-term)

1. **Complete service dispatch** — Currently a stub in `services.rs`
2. **Complete envelope import/export** — Currently returns "not implemented"
3. **Consider RwLock** — For read-heavy paths to reduce Mutex contention
4. **Add TLS support** — For production deployments outside localhost
5. **Add authentication** — Token or certificate-based auth on IPC connections
6. **Implement cleanup for unbounded HashMaps** — user_usage, user_windows, envelopes

### Documentation Fixes

1. **Fix README** — Remove reference to deleted `proto/` directory
2. **Fix README** — Prerequisite still mentions `protoc` which is no longer needed
3. **Add IPC protocol spec** — Document the TCP+msgpack wire format
4. **Add deployment guide** — Document environment variables and production config
5. **Add architecture decision records** — Document Python→Go→Rust rationale

---

*Generated: 2026-02-23 | Audit scope: Full repository (structure, git history, code quality, security, testing, DevOps, documentation)*
