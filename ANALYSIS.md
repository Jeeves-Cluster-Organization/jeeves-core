# Jeeves Core — Rust Improvement Plan

AI agent orchestration micro-kernel (~8,700 LOC). Process lifecycle, resource quotas, human-in-the-loop interrupts, IPC — via 4 gRPC services. Single `Kernel` struct behind `Arc<Mutex<Kernel>>`. Ported from Go. 107 tests, 81% coverage. `#![deny(unsafe_code)]`.

---

## Execution Plan

6 changes. No wire-format breaks. No proto file changes. Each step is independently committable.

Run after every step:
```bash
cargo build && cargo test && cargo clippy -- -D warnings
```

### Dependency graph
```
1 ID macro ──────────┐
                      ├─→ 5 Newtype IDs in PCB ──┐
4 num_enum ──────────┘                             ├─→ 6 Envelope decomposition
2 HashSet ─────────────────────────────────────────┘
3 QuotaViolation ──── (independent, slot anywhere)
```

---

### Step 1 — ID type macro

**Win**: 162 LOC → ~35 LOC. One-line ID type creation. Impossible to forget a trait impl.

| | |
|-|-|
| Files | 1 (`src/types/ids.rs`) |
| Tests to update | 0 |
| Wire impact | None |

Replace 5 identical newtype wrappers with `define_id!` macro. Public API unchanged.

**Watch out**: `UserId` lacks `new()` and `Default` — macro needs an opt-out flag, or align `UserId` with the others.

---

### Step 2 — `HashMap<String, bool>` → `HashSet<String>`

**Win**: Correct semantics. `contains()` instead of `.get().copied().unwrap_or(false)`. Drop useless `Option` wrappers.

| | |
|-|-|
| Files | 2 (`envelope/mod.rs`, `conversions.rs`) |
| Call sites | 13 |
| Tests to update | 0 (Envelope has no unit tests) |
| Wire impact | None — proto stays `map<string, bool>`, conversion layer translates |

Changes `active_stages` and `completed_stage_set`. Leave `failed_stages: HashMap<String, String>` alone (values are error messages).

**Watch out**: Dropping `Option` changes serde JSON from `null` to `[]` for empty — verify no external consumer depends on `null`.

---

### Step 3 — Structured `QuotaViolation` enum

**Win**: Callers can match on *which* quota was exceeded. Enables differentiated backoff (token exhaustion vs timeout). No string parsing.

| | |
|-|-|
| Files | 4 (`types.rs`, `resources.rs`, `mod.rs`, `kernel_service.rs`) |
| Call sites | 5 |
| Tests to update | ~3 (assertions change from string match to variant match) |
| Wire impact | None — `.to_string()` at gRPC boundary via `Display` impl |

```rust
pub enum QuotaViolation {
    LlmCalls { used: i32, limit: i32 },
    TokensIn { used: i64, limit: i64 },
    Timeout { elapsed: f64, limit: f64 },
    // ... 9 variants matching 9 current checks
}
```

**Watch out**: Decide `Option<QuotaViolation>` (first violation, current behavior) vs `Vec<QuotaViolation>` (all violations, more useful for debugging).

---

### Step 4 — `num_enum` for proto enum conversions

**Win**: Delete ~110 lines of hand-written `match` arms across 5 enums (38 variants). Derive-based — new variants are one line.

| | |
|-|-|
| Files | 2 (`Cargo.toml`, `conversions.rs`) |
| Tests to update | 0 |
| Wire impact | None if discriminants match proto exactly |
| New dep | `num_enum = "0.7"` (MSRV 1.64, ours is 1.75) |

Add `#[repr(i32)]` with explicit discriminants to domain enums. Proto `UNSPECIFIED = 0` has no domain equivalent — `TryFromPrimitiveError` maps to `Error::Validation`, same as today.

**Watch out**: `#[repr(i32)]` + `#[serde(rename_all = "lowercase")]` coexist fine, but verify. Check that `num_enum` derive expansion doesn't trigger `unwrap_used = "deny"` clippy lint.

---

### Step 5 — Newtype IDs in `ProcessControlBlock`

**Win**: The newtype IDs exist but PCB uses bare `String`. This closes that gap — can't pass a `user_id` where a `session_id` goes. Compiler catches it.

| | |
|-|-|
| Files | 5 (`types.rs`, `lifecycle.rs`, `kernel_service.rs`, `conversions.rs`, `cleanup.rs`) |
| Call sites | ~66 |
| Tests to update | ~20 (most test churn of any step) |
| Wire impact | None — conversion layer does `ProcessId::from_string(proto.pid)?` / `.to_string()` |

Changes 6 PCB fields: `pid`, `request_id`, `user_id`, `session_id`, `parent_pid`, `child_pids`. Also changes `HashMap<String, PCB>` → `HashMap<ProcessId, PCB>` and `PriorityItem.pid`.

**Watch out**: Full call chain changes together — gRPC handler → `Kernel` method → `LifecycleManager` method. Don't do one layer at a time. Also change `HashMap<String, Envelope>` in Kernel (keyed by PID).

---

### Step 6 — Envelope decomposition

**Win**: 30+ flat fields → 6 coherent sub-structs. Self-documenting (`env.bounds.at_limit()` vs `env.at_limit()`). Different contributors can own different concerns.

| | |
|-|-|
| Files | 5 (`envelope/mod.rs`, `engine_service.rs`, `orchestrator.rs`, `orchestration_service.rs`, `conversions.rs`) |
| Field accesses to rewrite | 72 |
| Tests to update | ~13 (orchestrator tests, mechanical `env.x` → `env.group.x`) |
| Wire impact | None — proto `Envelope` stays flat, conversion layer maps |

Groupings:
- **Identity** — `envelope_id`, `request_id`, `user_id`, `session_id`
- **Pipeline** — `current_stage`, `stage_order`, `iteration`, `max_iterations`, `active_stages`, `completed_stage_set`, `failed_stages`, `parallel_mode`
- **Bounds** — `llm_call_count`, `max_llm_calls`, `tool_call_count`, `agent_hop_count`, `max_agent_hops`, `tokens_in`, `tokens_out`, `terminal_reason`
- **Interrupts** — `interrupt_pending`, `interrupt`
- **Execution** — `completed_stages`, `current_stage_number`, `max_stages`, `all_goals`, `remaining_goals`, `goal_completion_status`, `prior_plans`, `loop_feedback`
- **Audit** — `processing_history`, `errors`
- **Top-level** — `raw_input`, `received_at`, `outputs`, `terminated`, `termination_reason`, `created_at`, `completed_at`, `metadata`

**Watch out**: Use `#[serde(flatten)]` on sub-structs to keep JSON serialization flat (backward compat). Verify `flatten` + `skip_serializing_if` interact correctly. Give each sub-struct a `Default` impl so `Envelope::new()` stays clean.

---

## Expected delta

| Metric | Before | After |
|--------|--------|-------|
| `ids.rs` LOC | 162 | ~35 |
| `conversions.rs` LOC | 760 | ~650 |
| `HashMap<_, bool>` sets | 2 | 0 |
| Opaque `Option<String>` quota errors | 1 | 0 |
| Bare `String` IDs in PCB | 6 | 0 |
| Flat Envelope fields | 30+ | 8 top-level + 6 sub-structs |
| Test count | 107 | 107 |

---

## Pre-flight checklist

- [ ] Identify external gRPC consumers (proto comments reference Python + Go) — verify no conversion behavior changes break them
- [ ] Check for `.snap` files (`insta`) and `.proptest-regressions` — serialization changes break snapshots
- [ ] Run `criterion` benchmarks before starting for regression baseline
- [ ] Verify `num_enum 0.7` clippy compliance under `unwrap_used = "deny"`

---

## Not doing (and why)

| Idea | Why not |
|------|---------|
| Typestate process lifecycle | Incompatible with `HashMap` storage — would need enum wrapper that adds more code than it removes. Current `match` + 15 tests already catches missing transitions |
| Lock sharding | 10 methods need multiple subsystems atomically → deadlock risk. Profile first. Consider `RwLock<Kernel>` as cheaper intermediate step |
| Kernel facade thinning | Delegation is the correct place for future cross-cutting concerns (metrics, audit) |
| Typed agent outputs | Outputs are intentionally dynamic — typing couples kernel to agent schemas |
| `Arc<str>` for IDs | 36-byte UUID strings. Clone cost is negligible. Profile first |
| `TokenCount`/`CallCount` newtypes | 14 wrappers + arithmetic impls for fields compared in one function |
