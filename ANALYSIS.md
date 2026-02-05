# Jeeves Core — Rust Improvement Plan (Completed)

AI agent orchestration micro-kernel (~8,700 LOC). Process lifecycle, resource quotas, human-in-the-loop interrupts, IPC — via 4 gRPC services. Single `Kernel` struct behind `Arc<Mutex<Kernel>>`. Ported from Go. 108 tests, 81% coverage. `#![deny(unsafe_code)]`.

---

## Status — All 6 Steps Complete

| Step | Description | Status | Commit |
|------|-------------|--------|--------|
| 1 | `define_id!` macro — 5 ID types from 162→75 LOC | **Done** | `f77af63` |
| 2 | `HashSet<String>` for stage sets | **Done** | `fa8e991` |
| 3 | Structured `QuotaViolation` enum (9 variants) | **Done** | `52a3d6c` |
| 4 | `proto_enum_conv!` macro — 166 LOC eliminated | **Done** | `1e74bd5` |
| 5 | Newtype IDs in PCB — end-to-end type safety | **Done** | `3b2c6f1` |
| 6 | Envelope decomposition → 6 sub-structs | **Done** | `a4f974c` |

108 tests pass. All steps are clean and independently revertable.

---

## What Changed

### Step 5 — Newtype IDs in PCB

**Approach**: Rejected `Borrow<str>` (defeats type safety — the whole point). Instead:
- Changed all method signatures to take `&ProcessId` instead of `&str`
- gRPC boundary constructs typed IDs from proto strings (`parse_pid` helper)
- Added `must()` convenience constructor for tests/constants
- `envelopes: HashMap<String, Envelope>` kept as-is (keyed by envelope_id, not ProcessId)
- 9 files, ~20 tests updated

**Files**: `ids.rs`, `types.rs`, `lifecycle.rs`, `mod.rs`, `cleanup.rs`, `resources.rs`, `conversions.rs`, `kernel_service.rs`, `tests/grpc_integration.rs`

### Step 6 — Envelope Decomposition

35 flat fields → 6 semantic sub-structs:

| Sub-struct | Fields | Purpose |
|-----------|--------|---------|
| `Identity` | envelope_id, request_id, user_id, session_id | Who |
| `Pipeline` | current_stage, stage_order, iteration, max_iterations, active_stages, completed_stage_set, failed_stages, parallel_mode | Pipeline sequencing |
| `Bounds` | llm_call_count, max_llm_calls, tool_call_count, agent_hop_count, max_agent_hops, tokens_in, tokens_out, terminal_reason, terminated, termination_reason | Resource limits |
| `InterruptState` | interrupt_pending, interrupt | Human-in-the-loop |
| `Execution` | completed_stages, current_stage_number, max_stages, all_goals, remaining_goals, goal_completion_status, prior_plans, loop_feedback | Multi-stage goals |
| `Audit` | processing_history, errors, created_at, completed_at, metadata | Observability |

Access: `envelope.bounds.llm_call_count`, `envelope.pipeline.current_stage`, etc.
Methods (`terminate()`, `set_interrupt()`, `complete_stage()`) operate through sub-structs.
No `#[serde(flatten)]` — nested JSON layout (no backward compat per P0 directive).

~72 field accesses updated across 5 consumer files.

---

## Audit Findings

- **Step 2 JSON serde**: `HashSet` → array `["s1"]` vs old `{"s1":true}`. Proto/gRPC unaffected. Accepted per no-backward-compat directive.
- **Step 5 design**: No `Borrow<str>` — type safety is enforced end-to-end. `HashMap<ProcessId, PCB>` cannot be queried with `&str`, which is intentional.
- **Step 6 JSON serde**: Nested JSON layout replaces flat. Accepted per no-backward-compat directive.
- **Pre-existing bugs fixed**: `orchestrator.rs` field name `stages:` → `agents:`, and test using uninitialized envelope.

---

## Not Doing (and Why)

| Idea | Why not |
|------|---------|
| Typestate process lifecycle | Incompatible with `HashMap` storage — enum wrapper adds more code than it removes |
| Lock sharding | 10 methods need multiple subsystems atomically → deadlock risk. Profile first |
| `Arc<str>` for IDs | 36-byte UUID strings. Clone cost negligible. Profile first |
| `TokenCount`/`CallCount` newtypes | 14 wrappers + arithmetic impls for fields compared in one function |
