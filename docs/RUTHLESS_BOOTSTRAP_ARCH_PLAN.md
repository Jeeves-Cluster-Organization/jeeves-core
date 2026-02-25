# Jeeves Core — Ruthless Bootstrap Architecture Plan (Zero-Compatibility Mode)

## Operating principle

If we are truly in **0 tech debt / no backward compatibility** mode, optimize for:

1. **Single canonical architecture** (no transitional abstractions)
2. **Low conceptual surface area** (fewer modules, fewer nouns)
3. **Performance by construction** (avoid global serialization)
4. **Replaceability** (clear seams where future changes don’t require migration layers)

This document intentionally proposes deleting compatibility scaffolding and placeholders.

---

## Ruthless decisions (non-negotiable)

### 1) Delete global `Arc<Mutex<Kernel>>` IPC path

Current model serializes request handling behind one kernel mutex and allows async work while the lock is effectively in the path.

**Replace with:**
- `KernelCore` (single-threaded state machine / actor task)
- `RuntimeServices` (commbus/orchestration clients that are async + internally concurrent)
- IPC router submits typed commands to core via channel and awaits typed responses

**Rule:** no `.await` while owning mutable core state.

### 2) Remove all stringly-typed IPC routing in runtime path

Service/method strings are parse-time compatibility concerns, not domain concerns.

**Replace with:**
- one parse boundary: wire JSON/msgpack -> typed `IpcCommand`
- everything after parse uses enums/structs only

**Rule:** unknown strings fail at parse boundary only.

### 3) Collapse naming to 3 layers only

Keep only these layers:

1. `ipc` = transport + framing + parse/serialize
2. `app` = command orchestration/use-cases
3. `core` = kernel state + invariants

Delete/merge ambiguous layers where needed (e.g., transport "dispatch" vs internal "service dispatch" nomenclature collisions).

### 4) Remove placeholder/service-registry machinery unless actively used

If `ServiceRegistry` dispatch handlers are placeholder or future-facing, remove them now.

**Rule:** no speculative abstractions in bootstrap branch.

### 5) Make mutability private and capability-based

`Kernel` internals should not be publicly reachable as fields.

**Replace with:**
- private fields
- explicit command handlers and query interfaces
- immutable snapshots for status responses

### 6) Make protocol versioning explicit but minimal

No broad compatibility matrix. Only:
- `protocol_version` integer
- one supported major
- hard fail for mismatches

**Rule:** prefer break-fast over compatibility glue.

---

## What to delete immediately

1. Duplicate "dispatch" naming across modules; rename to unambiguous router/invoker terms.
2. Placeholder registration/handler structures in kernel services not required by current runtime path.
3. Any optional code paths that preserve old response shapes.
4. Any cross-layer reach-through calls from IPC handlers into deep kernel internals.

---

## What to add immediately

1. **Typed command model**
   - `enum IpcCommand` with per-service payload structs
   - `enum IpcReply`
2. **Core actor loop**
   - single task owns lifecycle/resources/rate-limit/envelopes
   - receives `CoreCommand`, returns `CoreReply`
3. **Async side-effect executors**
   - commbus publish/query/subscribe handling outside core lock scope
4. **Architecture tests**
   - compile-time test that IPC modules cannot access core fields directly
   - runtime test that no await occurs in core command handlers (enforced structurally)
5. **Performance guardrail benchmark**
   - p95/p99 under mixed workload with interrupts + quota checks
   - benchmark is required to merge architectural changes

---

## Target module map (post-cleanup)

```text
src/
  ipc/
    server.rs          # frame IO, connection limits
    protocol.rs        # wire -> typed commands
    router.rs          # command routing to app
  app/
    mod.rs
    kernel_app.rs      # use-case handlers
    commbus_app.rs     # async bus interactions
  core/
    mod.rs
    kernel_core.rs     # authoritative state/invariants
    lifecycle.rs
    resources.rs
    rate_limit.rs
```

Notes:
- `kernel::services` is either removed or moved under `app` if truly needed.
- transport concerns stay out of `core`.

---

## Execution sequence (fastest path)

### Phase 1 (1-2 days): naming + deletion pass

- Rename IPC dispatch module to router terminology.
- Remove/trim placeholder service-registry dispatch path.
- Privatize kernel fields and break direct reach-through usage.

### Phase 2 (2-4 days): typed command boundary

- Introduce `IpcCommand` parser and migrate handlers.
- Keep behavior parity but no compatibility aliases.

### Phase 3 (3-5 days): core actorization

- Introduce `KernelCore` task + channels.
- Move mutable kernel state ownership into actor.
- Keep commbus async operations outside core state handling.

### Phase 4 (1-2 days): hardening

- Add perf benchmark gate.
- Add architecture assertions/tests.
- Document protocol major policy.

---

## Merge gate (strict)

A PR is blocked unless it passes all:

1. No `Arc<Mutex<Kernel>>` in IPC request path.
2. No string service/method switching after parse boundary.
3. No public mutable kernel internals.
4. Benchmarks reported (throughput + p95/p99 latency).
5. Protocol version behavior documented and tested.

---

## Anti-patterns to reject during bootstrap

- "Temporary" adapters for old API names
- Dual-path execution for old/new routing
- Optional feature flags for architecture transitions
- Re-exporting internals for convenience
- Placeholder abstractions without immediate call sites

In bootstrap mode, every extra seam becomes permanent debt. Prefer deletion + re-adding later if proven necessary.


---

## Dry run on current codebase (complexity + decisions)

This is a **non-coding dry run** of the migration against current Rust modules, intended to expose hidden coupling before implementation.

### Baseline inventory (observed)

- Global serialized request path exists in `src/ipc/server.rs` (`Arc<Mutex<Kernel>>` + lock in connection handler).
- Transport routing uses string service/method switches in `src/ipc/dispatch.rs`.
- IPC handlers directly touch kernel internals in `src/ipc/handlers/*.rs`.
- Kernel internals are publicly reachable in `src/kernel/mod.rs`.
- Service-registry dispatch naming collides conceptually with IPC dispatch in `src/kernel/services.rs`.

### Phase 1 dry run — naming + deletion pass

**Changes to attempt**
1. Rename `src/ipc/dispatch.rs` -> `src/ipc/router.rs` and rename `dispatch()` -> `route_request()`.
2. Rename `DispatchTarget/DispatchResult/dispatch` in `src/kernel/services.rs` to unambiguous service-invocation names.
3. Make `Kernel` fields private in `src/kernel/mod.rs` and add minimum method shims needed by handlers.

**Complexity estimate:** Medium (6/10)
- Mostly mechanical renames, but private-field change fan-out is high.

**Key decisions to confirm before coding**
- Whether to keep `ServiceRegistry` at all during bootstrap or delete now.
- Final naming vocabulary (`router`, `invoker`, `call`, etc.) to avoid second rename cycle.

### Phase 2 dry run — typed command boundary

**Changes to attempt**
1. Add `src/ipc/protocol.rs` with typed enums/structs for each service command.
2. Parse wire request once in server/router and pass typed command to application layer.
3. Remove all `match` on raw method strings outside parse boundary.

**Complexity estimate:** High (8/10)
- Largest behavior-preserving refactor due to breadth of methods and payload schemas.

**Key decisions to confirm before coding**
- Single `IpcCommand` enum vs per-service command enums wrapped in top-level enum.
- Error shape for parse failures (strictness vs diagnostics).

### Phase 3 dry run — core actorization

**Changes to attempt**
1. Replace shared kernel mutex with a single `KernelCore` task owning mutable state.
2. Use channel request/response messages from router/app into core.
3. Split async side effects (e.g., commbus publish/query/subscribe) out of core state path.

**Complexity estimate:** Very high (9/10)
- Concurrency model rewrite; highest risk phase for latent ordering bugs.

**Key decisions to confirm before coding**
- Ordering semantics: global FIFO vs per-process fairness.
- Backpressure policy for bounded channels under burst load.
- Cancellation semantics for timed-out client requests (drop vs cooperative cancel).

### Phase 4 dry run — hardening + gates

**Changes to attempt**
1. Add benchmark scenarios for mixed lifecycle/interrupt/quota workloads.
2. Add architecture tests asserting layer boundaries and visibility constraints.
3. Add protocol-major behavior tests.

**Complexity estimate:** Medium (5/10)
- Mostly additive; depends on how cleanly Phase 2/3 abstractions land.

**Key decisions to confirm before coding**
- Exact benchmark acceptance thresholds and CI enforcement policy.
- Which architecture checks are compile-time vs test-time.

### Critical risk register (with mitigation)

1. **Risk:** Hidden reliance on direct mutable kernel field access in handlers.
   - **Mitigation:** Land private-field shim methods first (Phase 1) before deeper refactors.
2. **Risk:** Regressions in streaming behavior for `Subscribe`.
   - **Mitigation:** Preserve existing frame-level contract in router tests while internals change.
3. **Risk:** Throughput improves but tail latency regresses due to channel contention.
   - **Mitigation:** Benchmark p95/p99 for mixed workloads, not only steady-state throughput.
4. **Risk:** Scope creep from speculative abstractions during actorization.
   - **Mitigation:** Enforce “one core actor + minimal app services” rule; reject optional paths.

### Recommended implementation order inside each phase

1. Introduce compile-safe aliases/tests first.
2. Perform one semantic change at a time.
3. Run tests/benchmarks after each semantic step.
4. Delete dead paths immediately after replacement (no dual-path grace period).

### Exit criteria for “dry run accepted”

Proceed to code only after these decisions are explicitly approved:

1. Keep vs delete `ServiceRegistry` now.
2. Final command model shape (`IpcCommand` hierarchy).
3. Actor ordering and backpressure policy.
4. Benchmark acceptance thresholds.
5. Protocol-major strictness behavior.


---

## Dry run v2 (after feedback): keep-callers-safe + one-major policy

This revision incorporates explicit product constraints from review:

1. **Do not remove components that may still have callers** (deprecate + isolate first).
2. **Protocol policy remains single major** (hard-fail cross-major).

### Decision lock (updated)

| Topic | Decision | Rationale |
|---|---|---|
| Placeholder/legacy components | **Retain initially if caller uncertainty exists**; isolate behind facade and mark deprecated | Avoid accidental breakage while still reducing debt |
| Protocol compatibility | **One-major only** | Bootstrap speed + deterministic behavior |
| Ordering semantics | Per-process FIFO + weighted global fairness | Avoid global serialization while preserving process coherence |
| Backpressure | Bounded queues + reject with retry hints | Prevent tail collapse under burst load |
| Cancellation | Cooperative cancel for long operations | Preserve consistency while allowing resource recovery |

### Revised phase complexity (with caller-safety constraint)

#### Phase 1 — naming + encapsulation (no behavioral deletion)

**Scope**
- Rename ambiguous transport naming (`dispatch` -> router terminology).
- Keep `ServiceRegistry` present if uncertain callers exist, but:
  - move behind trait/facade,
  - mark placeholder paths deprecated,
  - add usage telemetry/log warnings.
- Privatize kernel fields and replace direct field access with method surface.

**Complexity:** 7/10 (increased due to compatibility shims)

**Primary risk**
- Hidden transitive usage through tests/integration paths.

**Mitigation**
- Add compile-time and runtime call-site detection before any future deletion.

#### Phase 2 — typed command boundary

**Scope**
- Parse wire request once into typed command tree.
- Preserve external wire shape for current major.
- Keep temporary adapter only at boundary for unresolved legacy callers.

**Complexity:** 8/10

**Primary risk**
- Payload-schema drift across services.

**Mitigation**
- Golden tests per IPC method and centralized command schema table.

#### Phase 3 — core actorization

**Scope**
- Introduce `KernelCore` single-owner task for mutable state.
- Route handler calls to core command channel.
- Keep side-effectful async IO out of core critical path.

**Complexity:** 9/10

**Primary risk**
- Throughput wins but p99 regression from queue contention.

**Mitigation**
- Queue-depth metrics + burst benchmarks + strict p99 gates.

#### Phase 4 — one-major enforcement + hardening

**Scope**
- Add explicit `protocol_version` negotiation with hard cross-major fail.
- Keep only one active major and remove per-minor branching in runtime.
- Publish deprecation/upgrade note in docs.

**Complexity:** 4/10

**Primary risk**
- Integrator surprise on major bumps.

**Mitigation**
- Error payload includes supported major and migration pointer.

### Caller-safe deprecation workflow (replaces immediate deletion)

When a module might still be called:

1. **Isolate** behind interface/facade.
2. **Instrument** all entry points (counter + warning + caller tag).
3. **Observe** over agreed window (e.g., 2 release cycles or N days).
4. **Prove zero usage** in tests + runtime telemetry.
5. **Delete** in dedicated cleanup PR.

This preserves your “minimal debt” goal while avoiding accidental breakage from unknown consumers.

### Detailed complexity breakout by workstream

| Workstream | Est. Effort | Complexity Drivers | Exit Check |
|---|---:|---|---|
| Rename/router coherence | 1-2 days | cross-module import churn | all references compile + docs updated |
| Kernel encapsulation | 2-3 days | high fan-out from public fields | no direct field access from IPC handlers |
| Typed command schemas | 3-4 days | many methods + validation details | all methods mapped to typed command variants |
| Actor command loop | 4-6 days | concurrency semantics + cancellation | no global kernel mutex on request path |
| Backpressure/cancellation policy | 2-3 days | queue sizing + fairness tradeoffs | bounded queue saturation tests pass |
| One-major protocol guard | 1 day | parse-path changes + errors | major mismatch tests pass |
| Observability + telemetry | 1-2 days | metric cardinality choices | dashboards expose queue/latency/errors |
| Performance envelope | 2-3 days | realistic mixed workload design | p95/p99 gates enforced |

### Questions requiring explicit user decision (v2)

1. For uncertain callers, what observation window is acceptable before deletion (time-based vs release-count)?
2. Should deprecated paths be warning-only or fail-fast in non-prod profiles?
3. For one-major policy, do we allow a temporary env override in staging only?
4. What fairness policy do you prefer at saturation: tenant-weighted, process-priority-weighted, or hybrid?
5. What p99 budget should be merge-gating for mixed interrupt+quota load?

### v2 go/no-go gates

Proceed to invasive refactor only after:

1. Caller-safety workflow is approved.
2. One-major protocol behavior and error payload are approved.
3. Queue/backpressure fairness policy is selected.
4. Performance SLO thresholds are selected.
5. Observability dimensions are approved (to avoid metric rework).
