# jeeves-core Evolution

Full history of how jeeves-core has changed across its lifetime.

## Era 1: Python Monolith (pre-Jan 25 — PRs #1–#36)

At the earliest visible commit (PR #37, Jan 25 2026), the codebase was a large Python monolith with a Go execution sidecar:

- **335 Python files** across 6 domain-named packages
- **39 Go files** in `coreengine/`
- **1 proto file** (`engine.proto`) bridging Python↔Go via gRPC

Original Python packages:

| Package | Role |
|---|---|
| `jeeves_mission_system` | Orchestrator, prompts, LLM services, contracts |
| `jeeves_avionics` | Gateway, database, LLM providers, observability |
| `jeeves_control_tower` | Kernel lifecycle, IPC, events, resources |
| `jeeves_memory_module` | Memory adapters, repositories, message store |
| `jeeves_protocols` | Shared protocol definitions |
| `jeeves_shared` | Cross-cutting logging utilities |

Go `coreengine/` handled: agents, envelope routing, runtime, tools, gRPC server, and `commbus/`.

## Era 2: Gateway Refactoring (Jan 25)

Rapid cleanup of the Python gateway layer:
- EventHandler Strategy Pattern
- Extracted helpers (`_build_grpc_request`, `_is_internal_event`, `_process_event_stream`)
- Simplified `send_message` orchestration
- Removed legacy event naming fallback

## Era 3: Namespace Cleanup (Jan 26)

- Removed `jeeves_` prefix from all module names (PR #39)
- Added `GenerationParams` — K8s-style generation controls for optimistic concurrency

## Era 4: Python-to-Go Migration (Jan 27–29)

Three-step migration of all type definitions from Python to Go:

1. Enums → Go kernel
2. Envelope and interrupt types → Go kernel
3. Config types → Go kernel

LiteLLM made optional. Python packages moved to separate `jeeves-infra` repo.
Culminating commit: "Session 19 — Make jeeves-core 100% Pure Go".

**Result: 0 Python files, 46 Go files.**

Additional Go work:
- CommBusService gRPC
- Microkernel validation boundary pattern + agentic security validation
- Kernel hardening: orchestration, cleanup, panic recovery
- Kernel-mediated inference tracking

## Era 5: Go-to-Rust Rewrite (Jan 31 – Feb 2)

Complete rewrite in ~3 days:

| Phase | Scope |
|---|---|
| 1–2 | Rust scaffold + Envelope domain |
| 3 | Kernel state machine (LifecycleManager + ProcessControlBlock) |
| 4A | Proto conversions + KernelService gRPC |
| 4B–D | InterruptService, ServiceRegistry, Orchestrator |
| 4E–H | Full kernel integration, all 4 gRPC services |

Achieved 81.38% test coverage, 100% Go parity. Go `coreengine/` deleted.

**Result: 0 Python, 29 Rust files.**

## Era 6: Rust Idiom Hardening (Feb 5–7)

- `define_id!` macro (replaced 5 hand-written newtypes)
- `HashSet<String>` (replaced `HashMap<String, bool>`)
- `QuotaViolation` enum (replaced `Option<String>`)
- `proto_enum_conv!` macro (replaced hand-written conversions)
- Newtype IDs for PCB (replaced stringly-typed fields)
- Envelope decomposed into 6 semantic sub-structs
- Audit: zero clippy warnings, no panics, wired config

## Era 7: gRPC-to-IPC Pivot (Feb 11–12)

Dropped gRPC for raw TCP IPC:
- Deleted `engine.proto` — "code-is-the-contract"
- Custom IPC TCP handlers with approach-based pipeline routing
- Rate limiter, process cleanup, lifecycle events, timeout tracking

## Era 8: Python Reunion (Feb 24)

`jeeves-airframe` merged back into jeeves-core under `python/`:
- 129 Python files returned as `python/jeeves_core/`
- Unified naming: `jeeves_core` for both Rust crate and Python package
- Architecture: Rust microkernel + Python application layer

Python layer: gateway, database, LLM providers, memory, orchestrator,
events, distributed redis bus, observability, middleware, tools, protocols.

## Era 9: Kernel Hardening (Feb 25 – Mar 1)

Rust kernel:
- Bounded kernel actor queue, audit remediation
- Kernel loop extraction, interrupt service, session state
- Envelope ownership refactor (kernel owns envelopes, field-level borrow safety)
- Serde safety, atomic IPC, dead code elimination
- Tool catalog, access policy, health tracking

Python layer:
- Error code propagation, fail-fast validation
- Codegen Python enums from Rust definitions
- Token streaming in kernel-driven pipeline

## Era 10: Auto-Wiring Primitives (Mar 5)

- `CapabilityService` for dynamic capability registration
- `@tool` decorator for Python tool definitions
- `chain()` and `from_decorated()` composition primitives

## Timeline Summary

```
Jan 25   [335 .py + 39 .go]     Python monolith + Go sidecar
Jan 26   Remove jeeves_ prefix
Jan 28   [0 .py + 46 .go]      Pure Go microkernel
Feb 2    [0 .py + 29 .rs]      Complete Rust rewrite
Feb 11   gRPC → IPC TCP         Drop protobuf, raw TCP
Feb 24   [129 .py + 33 .rs]    Python merges back
Mar 5    [145 .py + 42 .rs]    Hardened kernel + app layer
```

Three complete rewrites in 40 days: Python → Go → Rust. The Python
application layer returned to live alongside the Rust kernel,
communicating over custom IPC/TCP.
