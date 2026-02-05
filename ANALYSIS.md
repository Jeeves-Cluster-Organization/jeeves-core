# Jeeves Core: Codebase Analysis & Improvement Plan

## 1. What This Code Is

**Jeeves Core** is an AI agent orchestration micro-kernel in Rust (~8,700 LOC, 30 .rs files). It treats AI agents like OS processes: a lifecycle state machine, resource quotas, priority scheduling, human-in-the-loop interrupts, and kernel-mediated IPC — exposed via 4 gRPC services (26 RPCs).

### Architecture

```
gRPC (4 services) → Arc<Mutex<Kernel>> → subsystems (plain structs, not actors)
```

Single `Kernel` struct owns all mutable state. All gRPC handlers contend on one tokio `Mutex`. Subsystems are plain structs (lifecycle, resources, rate limiter, interrupts, orchestrator, commbus) composed into the Kernel — no actor framework, no internal message queues.

### Subsystem map

| Subsystem | Files | Unit tests | What it does |
|-----------|-------|------------|--------------|
| Lifecycle | `lifecycle.rs` (726 LOC) | 15 | State machine + BinaryHeap scheduler |
| Resources | `resources.rs` | 8 | Quota enforcement per-process |
| Rate Limiter | `rate_limiter.rs` | 9 | Sliding-window per-user limiting |
| Interrupts | `interrupts.rs` | 12 | Human-in-the-loop flow control |
| Orchestrator | `orchestrator.rs` | 13 | Pipeline stage routing + execution |
| CommBus | `commbus/mod.rs` (877 LOC) | 16 | Pub/sub, commands, queries |
| Services | `services.rs` | 9 | Service registry + dispatch |
| Recovery | `recovery.rs` | 9 | `catch_unwind` isolation |
| Cleanup | `cleanup.rs` | 5 | Zombie reaping background task |
| Envelope | `envelope/mod.rs` (384 LOC) | 0 | 30+ field request state container |
| gRPC layer | `grpc/*.rs` (~1500 LOC) | 11 (integration) | Proto ↔ domain glue |

**Total: 96 unit tests + 11 integration tests. 81.36% coverage.**

---

## 2. Value

### Domain value

Unconstrained AI agents are dangerous. This kernel enforces:
- **Hard resource caps** — LLM calls, tokens, agent hops, iterations, wall-clock time
- **Human oversight gates** — structured interrupts for clarification, confirmation, approval
- **Fault isolation** — `catch_unwind` prevents one agent panic from crashing the kernel
- **Process scheduling** — priority queue with preemption, Unix-like state machine

### Why Rust specifically

- `#![deny(unsafe_code)]` — zero unsafe blocks, memory safety is structural
- No GC pauses — matters for a scheduling kernel
- `Send + Sync` — data race prevention at compile time
- Single binary — no runtime dependencies for deployment
- The type system can encode invariants that Go (the origin language) cannot

---

## 3. What Rust Already Buys Here

| Pattern | Where | What the compiler enforces |
|---------|-------|---------------------------|
| Exhaustive state match | `types.rs:46-72` | Adding a `ProcessState` variant forces handling everywhere |
| Newtype IDs | `ids.rs` | Can't pass `ProcessId` where `SessionId` expected |
| `thiserror` + `From<Error> for Status` | `errors.rs` | Domain errors auto-map to gRPC codes; `?` works end-to-end |
| `Option<T>` over sentinel values | `types.rs:252-259` | Must handle `None` — no "check if timestamp is zero" |
| Builder via consuming self | `envelope/mod.rs:61-93` | Can't use partially-built `FlowInterrupt` |
| `catch_unwind` at subsystem boundary | `recovery.rs` | Agent panics become `Result::Err`, not process death |

---

## 4. Improvements — Impact-First Analysis

Each improvement below includes: what changes, exact blast radius, what to evaluate before starting, wire compatibility impact, test rewrite scope, and dependency on other improvements.

### 4.1 ID type macro (collapse boilerplate)

**What changes**: Replace 5 hand-written newtype ID structs (162 LOC) with a `define_id!` macro (~35 LOC).

**Blast radius**: 1 file (`src/types/ids.rs`). Zero external API change — the public types and methods remain identical.

**Wire compat**: None. IDs are `String` on the wire. Internal representation unchanged.

**Tests affected**: 0. No tests exist for ID types directly. Existing tests use them and will catch regressions.

**Evaluate before starting**:
- `UserId` lacks `new()` and `Default` unlike the other 4 — macro needs an opt-out or `UserId` needs alignment
- Decide whether macro should also impl `AsRef<str>`, `Borrow<str>`, `From<String>` for ecosystem interop

**Depends on**: Nothing. Can execute first.

---

### 4.2 `HashMap<String, bool>` → `HashSet<String>`

**What changes**: `Envelope.active_stages` and `Envelope.completed_stage_set` change from `Option<HashMap<String, bool>>` to `HashSet<String>` (drop the `Option` wrapper too — empty set is the zero value).

**Blast radius**: 2 files, 13 call sites
- `src/envelope/mod.rs` — field definitions + 7 method bodies (`start_stage`, `complete_stage`, `is_stage_completed`, constructor)
- `src/grpc/conversions.rs` — 4 sites where proto `map<string, bool>` converts to/from domain type

**Wire compat**: **Proto stays `map<string, bool>`** (engine.proto lines 117-118). Conversion layer translates: `HashSet` → `HashMap<String, bool>` on the way out, reverse on the way in. Wire format unchanged.

**Tests affected**: 0 directly (Envelope has no unit tests). Integration tests exercise via gRPC and will catch conversion bugs.

**Evaluate before starting**:
- `failed_stages` is `Option<HashMap<String, String>>` (error messages as values) — this one stays as-is, only the `bool` maps convert
- Removing the `Option` wrapper changes serde output for JSON serialization — verify no external consumers depend on `null` vs `{}` for these fields
- Decide whether `active_stages` should also lose `Option` (currently `Some(HashMap::new())` at construction — the `Option` serves no purpose)

**Depends on**: Nothing.

---

### 4.3 Structured `QuotaViolation` enum

**What changes**: `ResourceUsage::exceeds_quota()` returns `Option<QuotaViolation>` instead of `Option<String>`.

**Blast radius**: 4 files, 5 call sites
- `src/kernel/types.rs:168-224` — rewrite `exceeds_quota()` body + define `QuotaViolation` enum (9 variants matching 9 checks)
- `src/kernel/types.rs:353-355` — `ProcessControlBlock::check_quota()` passthrough (signature change)
- `src/kernel/resources.rs:27` — `ResourceTracker::check_quota()` converts `QuotaViolation` → `Error::QuotaExceeded`
- `src/kernel/mod.rs:157-163` — `Kernel::check_quota()` delegation (no logic change)
- `src/grpc/kernel_service.rs:230-232` — the only external consumer; currently does `result.err().map(|e| e.to_string())`. **No caller parses the string today.**

**Wire compat**: None. gRPC `QuotaResult` message has `string exceeded_reason` — we still `.to_string()` the enum at the gRPC boundary. Optionally add a `violation_kind` field to proto later.

**Tests affected**: `src/kernel/resources.rs` has 8 tests — some assert on quota exceeded behavior. Need to update assertions from string matching to enum variant matching. Cleaner tests result.

**Evaluate before starting**:
- Implement `Display` on `QuotaViolation` so `.to_string()` at the gRPC boundary works without change
- Decide: should `exceeds_quota` return `Vec<QuotaViolation>` (all violations) or `Option<QuotaViolation>` (first violation)? Current code returns on first — `Vec` is more useful for debugging but changes semantics

**Depends on**: Nothing.

---

### 4.4 Proto enum conversions with `num_enum`

**What changes**: Replace hand-written `TryFrom<i32>` and `From<X> for i32` impls for 5 enums (30 `match` arms each direction) with derived impls.

**Blast radius**: 2 files
- `Cargo.toml` — add `num_enum = "0.7"` dependency
- `src/grpc/conversions.rs` — delete ~110 lines (lines 21-118, 125-170, 666-690) of manual `match` arms

**Problem**: **`num_enum` derives on the Rust enum, but these enums must map to proto-generated `i32` values that start at offset** (proto `PROCESS_STATE_NEW = 1`, not 0). The domain enums (`ProcessState::New`, `ProcessState::Ready`, ...) don't carry discriminant values today. You need to add explicit discriminants:

```rust
#[derive(TryFromPrimitive, IntoPrimitive)]
#[repr(i32)]
pub enum ProcessState {
    New = 1,
    Ready = 2,
    // ...
}
```

**Wire compat**: Must verify discriminant values match proto exactly. Proto `PROCESS_STATE_NEW = 1`, `PROCESS_STATE_READY = 2`, etc. — verify for all 5 enums (38 total variants).

**Tests affected**: 0 direct. Proto conversion tests in integration suite validate round-trip.

**Evaluate before starting**:
- Proto enums have `UNSPECIFIED = 0` variants that the domain enums don't. `num_enum` will return `TryFromPrimitiveError` for 0 — verify this maps to the same `Error::Validation` as the current code
- The domain `ProcessState` currently derives `Serialize, Deserialize` with `#[serde(rename_all = "lowercase")]` — adding `#[repr(i32)]` doesn't conflict, but verify
- `InstructionKind` has only 3 variants — too small to bother? Include anyway for consistency

**Depends on**: Nothing. But if combined with 4.5 (newtype IDs in PCB), do this first since it simplifies the conversion file before the larger PCB refactor touches it.

---

### 4.5 Use newtype IDs in `ProcessControlBlock`

**What changes**: `ProcessControlBlock.pid: String` → `ProcessControlBlock.pid: ProcessId`, and similarly for `request_id`, `user_id`, `session_id`, `parent_pid`, `child_pids`.

**Blast radius**: 5 files, ~66 call sites
- `src/kernel/types.rs` — PCB struct definition (6 fields) + constructor
- `src/kernel/lifecycle.rs` — 15+ sites that access `pcb.pid` as `&str` (use `.as_str()`)
- `src/grpc/kernel_service.rs` — 20+ sites that pass `req.pid` (String from proto) into kernel methods
- `src/grpc/conversions.rs` — PCB `TryFrom`/`From` impls, ~10 sites
- `src/kernel/cleanup.rs` — test assertions comparing PIDs

**Wire compat**: Proto `ProcessControlBlock.pid` is `string`. Conversion layer does `ProcessId::from_string(proto.pid)?` inbound and `pcb.pid.to_string()` outbound. Wire format unchanged.

**Tests affected**: 15 lifecycle tests + 5 cleanup tests + 11 integration tests will need PID construction changes (`.to_string()` → `ProcessId::from_string()`). This is the most test churn of any change.

**Evaluate before starting**:
- The `Kernel::create_process` method signature takes `envelope_id: String` — this becomes `EnvelopeId`. But `LifecycleManager::submit` also takes `pid: String`. The entire internal API signature chain changes. **Map the full call chain: gRPC handler → Kernel method → subsystem method** and change all three layers in one pass
- `HashMap<String, ProcessControlBlock>` in `LifecycleManager` becomes `HashMap<ProcessId, ProcessControlBlock>` — `ProcessId` already derives `Hash + Eq`, so this works
- `HashMap<String, Envelope>` in `Kernel` keyed by PID — also needs to change
- `PriorityItem.pid: String` in lifecycle.rs needs to become `ProcessId`
- Decide: should `ProcessControlBlock::new()` generate a `ProcessId::new()` internally, or require one as input? Current code takes `pid: String` from the caller

**Depends on**: 4.1 (ID macro) should go first so the ID types are clean before propagating them.

---

### 4.6 Envelope decomposition into sub-structs

**What changes**: Split 30+ fields of `Envelope` into ~6 nested structs: `Identity`, `PipelineState`, `BoundsTracker`, `InterruptState`, `ExecutionState`, `AuditTrail`.

**Blast radius**: **Highest of any change.** 5 files, 72+ direct field accesses
- `src/envelope/mod.rs` — struct definition + all methods (entire file rewrites)
- `src/grpc/engine_service.rs` — 28 field accesses become `env.bounds.llm_call_count`, `env.pipeline.current_stage`, etc.
- `src/kernel/orchestrator.rs` — 35 field accesses, densest consumer
- `src/grpc/orchestration_service.rs` — 4 accesses
- `src/grpc/conversions.rs` — Envelope `TryFrom`/`From` impls rebuild entire struct

**Wire compat**: None. Proto `Envelope` message is flat. Conversion layer maps flat proto → nested domain struct and back. Proto file untouched.

**Tests affected**: Orchestrator has 13 tests that construct `Envelope::new()` and read fields. All need updating to use nested access paths. No logic changes, just `env.field` → `env.group.field` mechanical rewrites.

**Evaluate before starting**:
- **Define the groupings precisely before writing code.** Proposed:
  - `Identity` — `envelope_id`, `request_id`, `user_id`, `session_id`
  - `Pipeline` — `current_stage`, `stage_order`, `iteration`, `max_iterations`, `active_stages`, `completed_stage_set`, `failed_stages`, `parallel_mode`
  - `Bounds` — `llm_call_count`, `max_llm_calls`, `tool_call_count`, `agent_hop_count`, `max_agent_hops`, `tokens_in`, `tokens_out`, `terminal_reason`
  - `Interrupts` — `interrupt_pending`, `interrupt`
  - `Execution` — `completed_stages`, `current_stage_number`, `max_stages`, `all_goals`, `remaining_goals`, `goal_completion_status`, `prior_plans`, `loop_feedback`
  - `Audit` — `processing_history`, `errors`
  - Top-level: `raw_input`, `received_at`, `outputs`, `terminated`, `termination_reason`, `created_at`, `completed_at`, `metadata`
- **Serde compatibility**: Use `#[serde(flatten)]` on each sub-struct so JSON serialization remains flat. This preserves backward compatibility for any non-proto JSON consumers. Verify `flatten` works with `skip_serializing_if` on nested fields
- `Envelope::new()` constructor must build all sub-structs — use `Default` impls on each
- Methods like `at_limit()`, `terminate()`, `set_interrupt()` move to the relevant sub-struct or become thin delegations

**Depends on**: 4.2 (HashSet) should go first — changing `active_stages`/`completed_stage_set` types before decomposing avoids doing both at once. 4.5 (newtype IDs) in Envelope identity fields should also be settled first.

---

### 4.7 Lock sharding

**What changes**: Replace `Arc<Mutex<Kernel>>` with per-subsystem locks.

**Blast radius**: 6+ files
- `src/main.rs` — Kernel construction becomes `SharedKernel` construction
- `src/kernel/mod.rs` — Kernel struct either splits or grows accessor methods that take individual locks
- All 4 gRPC service files — every handler changes from `self.kernel.lock().await` to targeted lock acquisitions
- `tests/grpc_integration.rs` — test kernel setup changes

**The hard part: lock ordering.** 10 methods require multiple subsystems atomically:
- `create_process`: rate_limiter → lifecycle
- `record_tool_call`, `record_agent_hop`: lifecycle → envelopes
- `wait_process`, `resume_process`, `terminate_process`: lifecycle → envelopes
- `check_quota`: lifecycle → resources
- `cleanup_process`: lifecycle → envelopes

If two RPCs acquire (lifecycle, envelopes) and (envelopes, lifecycle) concurrently — **deadlock**. Must enforce a global lock ordering or use `try_lock` with retry.

**Wire compat**: None (internal only).

**Tests affected**: All 11 integration tests need `SharedKernel` setup. Unit tests are unaffected (they construct subsystems directly).

**Evaluate before starting**:
- **Is this actually needed yet?** Profile under realistic load first. Single-mutex is simple and correct. Sharding adds deadlock risk and cognitive overhead. Only do this if benchmarks show contention
- `CommBus` already uses internal `Arc<RwLock>` — it's already sharded. The question is whether lifecycle/resources/interrupts need sharding
- `DashMap` is already in `Cargo.toml` but unused — it was likely planned for this
- Consider an intermediate step: `RwLock<Kernel>` instead of `Mutex<Kernel>` — all read-only RPCs (`GetProcess`, `CheckQuota`, `ListProcesses`) take a read lock, only mutating RPCs take a write lock. Much simpler than full sharding

**Depends on**: Everything else should be done first. This is the riskiest change and should come last.

---

### 4.8 Kernel facade thinning (defer, don't eliminate)

**Current state**: Kernel has 30+ delegation methods. This is verbose but provides a single point for cross-cutting concerns.

**Recommendation**: **Don't change this now.** The delegation pattern is actually correct for a kernel that will add logging, metrics, and audit at the facade layer. Instead:
- Add `#[inline]` to trivial delegations so they compile away
- Only thin the facade if/when you do lock sharding (4.7), since the facade can't survive per-subsystem locks unchanged

**Depends on**: 4.7 (lock sharding). Do them together or not at all.

---

### 4.9 Typestate for process lifecycle (defer)

**Recommendation**: **Don't do this in the same pass.** Here's why:

The typestate pattern (`Process<Running>`, `Process<Ready>`) is elegant but fundamentally incompatible with `HashMap<K, PCB>` storage. You'd need:
- An enum wrapper `AnyProcess` that erases the state type parameter
- Pattern-matching to extract the typed process from the wrapper
- Re-insertion after state transition

This is more code than the current approach, not less. The current runtime checks are correct, well-tested (15 tests), and the exhaustive `match` in `can_transition_to` already catches missing transitions at compile time when new states are added.

**When to reconsider**: If the state machine grows beyond ~10 states or if invalid-transition bugs actually occur in production.

---

## 5. Pre-Flight Evaluation Checklist

Before executing any improvement, verify these:

### External consumers
- [ ] **Who calls this gRPC API?** Python orchestrator? Go services? The proto file comments reference both. Any internal change that alters proto conversion behavior could break non-Rust consumers
- [ ] **Is there a JSON API consumer?** Envelope serialization via serde could have consumers that expect the current flat JSON shape. `#[serde(flatten)]` preserves this, but verify
- [ ] **Are there snapshot tests?** `insta` is in `Cargo.toml` — check if any `.snap` files exist that will break

### Build system
- [ ] **Proto regeneration**: `build.rs` uses `tonic-build` to generate proto types. Adding `num_enum` discriminants to domain enums is a domain-side change — proto codegen is unaffected. But verify
- [ ] **CI pipeline**: If CI runs `cargo clippy` with the strict lints in `Cargo.toml` (`unwrap_used = "deny"`, `panic = "warn"`), new code must comply. The `num_enum` derive expands to code that doesn't use `unwrap`

### Test infrastructure
- [ ] **Property tests**: `proptest` is in dev-dependencies. Check if any `.proptest-regressions` files exist — these are generated regression cases that must be preserved
- [ ] **Mock objects**: `mockall` is in dev-dependencies. If any subsystem is mocked via trait, changing the subsystem's type signatures breaks the mock

### Dependency additions
- [ ] **Only `num_enum` is new.** All other changes use existing dependencies. Verify `num_enum 0.7` is compatible with MSRV (`rust-version = "1.75"`)
- [ ] `parking_lot` and `dashmap` are already in `Cargo.toml` — they were likely intended for future use and can be leveraged

---

## 6. Execution Order for One-Shot Delivery

The improvements have dependency relationships:

```
  4.1 ID macro ─────────────────┐
                                 ├─→ 4.5 Newtype IDs in PCB ──┐
  4.4 num_enum ────────────────┘                               │
                                                               ├─→ 4.6 Envelope decomposition
  4.2 HashSet ──────────────────────────────────────────────────┘

  4.3 QuotaViolation ──── (independent)

  4.7 Lock sharding ──── (last, only if profiling justifies)
```

### Recommended execution sequence

| Step | Change | Files touched | Tests to rewrite | New deps | Risk |
|------|--------|---------------|------------------|----------|------|
| 1 | **4.1 ID macro** | 1 (`ids.rs`) | 0 | 0 | Minimal |
| 2 | **4.2 HashSet** | 2 (`envelope/mod.rs`, `conversions.rs`) | 0 | 0 | Low |
| 3 | **4.3 QuotaViolation** | 4 (`types.rs`, `resources.rs`, `mod.rs`, `kernel_service.rs`) | ~3 | 0 | Low |
| 4 | **4.4 num_enum** | 2 (`Cargo.toml`, `conversions.rs`) | 0 | `num_enum` | Low |
| 5 | **4.5 Newtype IDs in PCB** | 5 (`types.rs`, `lifecycle.rs`, `kernel_service.rs`, `conversions.rs`, `cleanup.rs`) | ~20 | 0 | Medium |
| 6 | **4.6 Envelope decomposition** | 5 (`envelope/mod.rs`, `engine_service.rs`, `orchestrator.rs`, `orchestration_service.rs`, `conversions.rs`) | ~13 | 0 | Medium-High |
| — | **4.7 Lock sharding** | 6+ files | ~11 | 0 | **High (defer)** |

### Verification gates between steps

After each step, run:
```bash
cargo build                    # Compilation check
cargo test                     # All 107 tests pass
cargo clippy -- -D warnings    # No new warnings
```

If any gate fails, fix before proceeding. Each step is independently committable and revertable.

### Expected total delta

| Metric | Before | After (steps 1-6) |
|--------|--------|--------------------|
| LOC in `ids.rs` | 162 | ~35 |
| LOC in `conversions.rs` | 760 | ~650 |
| `HashMap<String, bool>` instances | 2 | 0 |
| `Option<String>` quota violations | 1 | 0 (structured enum) |
| Bare `String` IDs in PCB | 6 fields | 0 |
| Flat fields on Envelope | 30+ | ~8 top-level + 6 sub-structs |
| Runtime state-transition checks | 8 in lifecycle.rs | 8 (unchanged — typestate deferred) |
| Test count | 107 | 107 (same tests, updated assertions) |

---

## 7. What NOT to Change

| Temptation | Why to resist |
|------------|---------------|
| Typestate for process lifecycle | Incompatible with `HashMap` storage; current `match` + tests already catch missing transitions |
| Kernel facade elimination | Facade is the correct place for cross-cutting concerns; thin it only when sharding |
| `serde_json::Value` → typed outputs | Outputs are intentionally dynamic to support arbitrary agent architectures. Typing them prematurely couples the kernel to specific agent schemas |
| `Arc<str>` for identifiers | Profile first. UUID-length strings are 36 bytes. The clone cost is negligible at current scale. Premature optimization adds complexity to every ID construction site |
| Domain-specific quota types (`TokenCount`, `CallCount`) | 14 newtype wrappers + arithmetic impls for types that are only compared in one function. The boilerplate exceeds the bug-prevention value at this scale |

---

## 8. Things To Evaluate That This Analysis Does Not Cover

1. **Load profile**: How many concurrent processes are expected? 10? 1000? 100,000? This determines whether lock sharding (4.7) is needed and whether `Arc<str>` identifiers matter
2. **Proto evolution plan**: Is the proto file shared with non-Rust consumers? If Python/Go clients exist, proto field additions need coordinated rollout. The proto comments reference both Go and Python
3. **Snapshot files**: `insta` is in `Cargo.toml` — do `.snap` files exist? Any serialization change will break them
4. **Property test regressions**: `proptest` generates `.proptest-regressions` files. These must survive refactoring
5. **Benchmark baselines**: `criterion` is in dev-dependencies. Run benchmarks before and after to catch performance regressions
6. **Clippy CI strictness**: `Cargo.toml` denies `unwrap_used` — all new code paths must avoid `.unwrap()`. Verify derived `num_enum` code complies
7. **MSRV compatibility**: `rust-version = "1.75"`. Verify `num_enum 0.7` supports this. (It does — `num_enum 0.7` requires Rust 1.64+)
8. **Orphan rule constraints**: Adding `From<ProcessId> for String` impls in `ids.rs` is fine (own type). But if newtype IDs are used with third-party types (e.g., `From<ProcessId> for tonic::metadata::MetadataValue`), orphan rules may block the impl
