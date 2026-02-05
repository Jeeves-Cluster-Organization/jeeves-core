# Jeeves Core — Rust Improvement Plan

AI agent orchestration micro-kernel (~8,700 LOC). Process lifecycle, resource quotas, human-in-the-loop interrupts, IPC — via 4 gRPC services. Single `Kernel` struct behind `Arc<Mutex<Kernel>>`. Ported from Go. 108 tests, 81% coverage. `#![deny(unsafe_code)]`.

---

## Status

| Step | Description | Status | Commit |
|------|-------------|--------|--------|
| 1 | ID type macro | **Done** | `f77af63` |
| 2 | HashSet for stage sets | **Done** | `fa8e991` |
| 3 | Structured QuotaViolation | **Done** | `52a3d6c` |
| 4 | Proto enum conversion macro | **Done** | `1e74bd5` |
| 5 | Newtype IDs in PCB | **Stashed (WIP)** | — |
| 6 | Envelope decomposition | Not started | — |

108 tests pass on committed code. Steps 1-4 are clean and independently revertable.

---

## Audit findings

### Step 2 — JSON serde format change (known risk)

The `HashSet<String>` serializes as a JSON array `["stage1", "stage2"]`, not the old `HashMap<String, bool>` format `{"stage1": true}`. **Proto/gRPC is unaffected** — the conversion layer handles translation. But if any external system deserializes `Envelope` from JSON (not proto), old payloads will fail to parse.

**Mitigation if needed**: Add a custom deserializer that accepts both formats, or confirm all Envelope serialization goes through proto.

### Steps 1, 3, 4 — Clean

- Step 1: Macro correctly handles uuid/no-uuid variants. UserId lacks new()/Default as intended.
- Step 3: Display impl produces identical strings to old format!(). Wire compat preserved.
- Step 4: Macro handles UNSPECIFIED=0 correctly (falls to error arm). All 38 variants across 5 enums verified against proto.

### Pre-existing bugs fixed along the way

- `orchestrator.rs:471`: `stages:` → `agents:` (field name mismatch, test didn't compile under `cargo test`)
- `orchestrator.rs:672`: `test_report_agent_result_updates_metrics` passed a blank envelope instead of the initialized one (panic at runtime)

---

## Where Step 5 got stuck

Step 5 (newtype IDs in PCB) is partially implemented and stashed (`git stash list` to see). The PCB struct, lifecycle manager struct, PriorityItem, and submit() signature are all updated. The remaining work hit 3 blockers:

### Blocker 1: `ids` module is private

`src/types/mod.rs` doesn't `pub mod ids;`. The lifecycle and types modules import `crate::types::ids::ProcessId` but it's not public.

**Fix**: Add `pub mod ids;` to `src/types/mod.rs`.

### Blocker 2: `HashMap<ProcessId, V>` can't be indexed by `&str`

All lifecycle methods (`schedule`, `start`, `terminate`, etc.) take `pid: &str` and do `self.processes.get(pid)`. After changing to `HashMap<ProcessId, V>`, this requires either:

- **Option A**: Implement `Borrow<str>` on ProcessId so HashMap lookup works with `&str`. Added the import but didn't implement the trait in the macro yet.
- **Option B**: Change all method signatures to take `&ProcessId`. Simpler but forces all callers (Kernel, gRPC handlers) to construct a ProcessId before calling.
- **Recommended**: Option A — add `Borrow<str>` to the macro. This keeps the internal API ergonomic (`lifecycle.get("pid1")` still works in tests) while the external API (gRPC handlers) constructs typed IDs at the boundary.

### Blocker 3: Cascade to Kernel + gRPC + conversions + tests

Once lifecycle signatures change, the Kernel delegation methods, gRPC handlers, proto conversions, and ~20 tests all need updating. This is the highest-churn step and should be done as one atomic pass across all 5 files, not incrementally.

**Estimated remaining work**: Add `Borrow<str>` to macro → make `ids` module public → update Kernel methods → update gRPC handlers → update conversions → update 20+ tests.

---

## Remaining execution plan

### Step 5 — Newtype IDs in PCB (resume from stash)

```
git stash pop
```

Then:
1. Add `Borrow<str>` impl to both macro variants in `ids.rs`
2. Add `pub mod ids;` to `src/types/mod.rs`
3. Keep lifecycle methods taking `&str` (Borrow<str> makes this work)
4. Update `Kernel::create_process` signature: `envelope_id: String` → `ProcessId` etc.
5. Update `Kernel::envelopes: HashMap<String, Envelope>` → `HashMap<ProcessId, Envelope>`
6. Update gRPC handlers: construct typed IDs from proto strings at the boundary
7. Update conversions: `ProcessId::from_string(proto.pid)?` inbound, `pcb.pid.to_string()` outbound
8. Update all tests: `"pid1".to_string()` → `ProcessId::from_string("pid1".to_string()).unwrap()`

### Step 6 — Envelope decomposition

No changes from original plan. Depends on step 5 (identity fields move to sub-struct).

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
