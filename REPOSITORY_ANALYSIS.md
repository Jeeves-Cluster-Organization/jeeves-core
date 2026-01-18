# Jeeves-Core Repository Analysis

**Generated:** 2026-01-18
**Branch:** cursor/repository-history-analysis-405c

---

## Executive Summary

This repository contains **jeeves-core**, a layered agentic runtime platform combining Python application logic with a Go orchestration engine. The project is designed to be a reusable foundation for building AI agent capabilities.

---

## Repository Statistics

### Lines of Code

| Metric | Count |
|--------|-------|
| **Total Lines Added (all time)** | 123,005 |
| **Total Lines Removed (all time)** | 18,156 |
| **Net Lines** | 104,849 |
| **Current Python Lines** | 74,278 |
| **Current Go Lines** | 7,985 |
| **Total Files (current)** | ~418 files |

### File Counts by Type

| Type | Count |
|------|-------|
| Python (.py) | 317 |
| Go (.go) | 21 |
| Markdown (.md) | 18 |

### Lines by Module

| Module | Files | Lines | Purpose |
|--------|-------|-------|---------|
| `jeeves_mission_system` | 136 | 28,074 | L4 - API layer, orchestration, services |
| `jeeves_avionics` | 76 | 16,690 | L3 - Infrastructure (LLM, DB, Gateway) |
| `jeeves_memory_module` | 49 | 14,907 | L2 - Event sourcing, semantic memory |
| `coreengine` | 14 | 5,718 | Go - Pipeline orchestration, DAG executor |
| `jeeves_protocols` | 21 | 5,616 | L0 - Type contracts, protocols |
| `jeeves_control_tower` | 23 | 5,550 | L1 - OS-like kernel, lifecycle management |
| `commbus` | 6 | 1,969 | Go - Communication bus (pub/sub) |
| `jeeves_shared` | 5 | 907 | L0 - Shared utilities |

---

## Contribution Statistics

| Author | Lines Added | Lines Removed | Total Changes |
|--------|-------------|---------------|---------------|
| Emper0r | +115,760 | - | 115,760 |
| Claude | +6,308 | - | 6,308 |
| Cursor Agent | +937 | - | 937 |

---

## Development Timeline

### Commit History by Date

| Date | Commits |
|------|---------|
| 2025-12-13 | 8 (Initial commit + core platform) |
| 2025-12-14 | 14 |
| 2025-12-15 | 1 |
| 2025-12-16 | 8 |
| 2025-12-18 | 13 |
| 2025-12-19 | 2 |
| 2025-12-31 | 1 |
| 2026-01-06 | 2 |
| 2026-01-14 | 4 |

**Total Commits:** 53 commits over ~5 weeks

### Project Lifecycle

1. **2025-12-13**: Initial commit with 107,287 lines establishing the core reusable platform
2. **2025-12-14 to 2025-12-18**: Rapid iteration with Docker config, capability contracts, and cleanup
3. **2025-12-16 to 2025-12-18**: Major audit and refactoring phase:
   - Dead code removal (~10,000+ lines removed)
   - Shell script consolidation
   - Frontend assets moved to capability layer
   - Documentation cleanup
4. **2025-12-19 to 2026-01-06**: Stabilization and feature development
5. **2026-01-14**: Documentation updates and Go binary builds

### Commit Types (Conventional Commits)

| Type | Count |
|------|-------|
| docs | 8 |
| refactor | 7 |
| fix | 2 |
| chore | 1 |

---

## What the Codebase Is Doing

### Core Purpose

**Jeeves-core is a layered agentic runtime** that provides:

1. **Pipeline Orchestration**: Sequential and DAG-based agent execution
2. **LLM Provider Abstraction**: Support for OpenAI, Anthropic, Azure, LlamaServer, LlamaCpp
3. **Four-Layer Memory Architecture**:
   - L1 Episodic: Per-request working memory
   - L2 Event Log: Permanent audit trail (PostgreSQL)
   - L3 Working Memory: Session state persistence
   - L4 Persistent Cache: Semantic search (PostgreSQL + pgvector)
4. **Tool Registry**: Risk-classified tools with per-agent access control
5. **Interrupt System**: Clarification, confirmation, human-in-the-loop workflows
6. **Enterprise Features**: Rate limiting, circuit breakers, checkpointing

### Architecture Layers

```
┌─────────────────────────────────────────────────────────────────┐
│ YOUR CAPABILITY (e.g., jeeves-capability-finetuning)            │
│     - Domain-specific agents, tools, configs                    │
├─────────────────────────────────────────────────────────────────┤
│ L4: jeeves_mission_system                                       │
│     - Orchestration framework, HTTP/gRPC API                    │
│     - Capability registration, adapters                         │
├─────────────────────────────────────────────────────────────────┤
│ L3: jeeves_avionics                                             │
│     - Infrastructure (LLM providers, DB, Gateway)               │
│     - Tool execution, settings, feature flags                   │
├─────────────────────────────────────────────────────────────────┤
│ L2: jeeves_memory_module                                        │
│     - Event sourcing, semantic memory, session state            │
│     - Entity graphs, tool metrics                               │
├─────────────────────────────────────────────────────────────────┤
│ L1: jeeves_control_tower                                        │
│     - OS-like kernel (process lifecycle, resources)             │
│     - Rate limiting, IPC coordination                           │
├─────────────────────────────────────────────────────────────────┤
│ L0: jeeves_protocols + jeeves_shared                            │
│     - Type contracts (zero dependencies)                        │
│     - Shared utilities (UUID, logging, serialization)           │
├─────────────────────────────────────────────────────────────────┤
│ GO: coreengine + commbus                                        │
│     - Pipeline orchestration (DAG executor)                     │
│     - Unified agent execution, communication bus                │
└─────────────────────────────────────────────────────────────────┘
```

### Key Design Principles

1. **Protocol-First**: All components depend on protocols, not implementations
2. **Configuration Over Code**: Agents defined via config, not inheritance
3. **Layered Architecture**: Strict import boundaries (L0 → L4)
4. **Capability Ownership**: Capabilities own their domain config
5. **Bounded Efficiency**: All operations have resource limits

---

## What the Codebase Is Attempting to Do

### Strategic Goals

1. **Create a Reusable AI Agent Platform**
   - Core runtime that can be used as a submodule/foundation
   - Clean separation between "core" infrastructure and "capability" layers
   - Capabilities plug in without modifying core

2. **Hybrid Python/Go Architecture**
   - Python for application logic, protocols, and APIs
   - Go for high-performance pipeline orchestration and DAG execution
   - CLI bridge between the two (subprocess communication)

3. **Enterprise-Ready Agent Infrastructure**
   - Rate limiting and quota enforcement
   - Checkpointing and state recovery
   - Audit trails via event sourcing
   - Human-in-the-loop interrupt handling

4. **Memory-Rich Agent Systems**
   - Four-layer memory for different scopes and persistence needs
   - Semantic search with pgvector
   - Entity graph relationships
   - Session state management

### Active Development Areas

Based on recent commits:

1. **Go Engine Maturity**: Building the Go binary, DAG executor implementation
2. **Contract Enforcement**: Envelope sync between Go and Python
3. **Documentation**: Comprehensive handoff docs for capability developers
4. **Code Quality**: Audit-driven cleanup, dead code removal, refactoring

### Architectural Contracts (13 defined)

1. Protocol-Based State Management
2. Context-Aware Global State (Contextvars)
3. Dependency Injection via Adapters
4. Absolute Imports in TYPE_CHECKING Blocks
5. Single Source for Protocols
6. Dead Code Removal
7. Interrupt Handling Ownership
8. Span Tracking Isolation
9. Logger Fallback Pattern
10. Capability-Owned ToolId
11. Envelope Sync (Go ↔ Python)
12. Bounds Authority Split (Go in-loop, Python post-hoc)
13. Core Doesn't Reject Cycles

---

## Notable Patterns

### Agent Configuration (No Inheritance)

```python
PLANNER_CONFIG = AgentConfig(
    name="finetuning_planner",
    stage_order=1,
    has_llm=True,
    has_tools=False,
    prompt_key="finetuning_planner",
    output_key="plan",
    routing_rules=[
        RoutingRule(condition="has_plan", value=True, target="executor"),
    ],
)
```

### Tool Registration (Capability-Owned)

```python
@catalog.register(
    tool_id="my_capability.my_tool",
    description="What this tool does",
    category=ToolCategory.COMPOSITE,
    risk_level=RiskLevel.LOW,
)
async def my_tool(param1: str) -> dict:
    return {"status": "success", "data": {...}}
```

### Pipeline Flow

```
start → planner → executor → critic → end
                    ↑          │
                    └──[retry]─┘
           ↑                    │
           └────[replan]────────┘
```

---

## Summary

Jeeves-core is a sophisticated, enterprise-grade AI agent runtime platform that:

- Contains **~82,000 lines of code** (74K Python + 8K Go)
- Has been developed over **~5 weeks** with **53 commits**
- Follows a **strict 5-layer architecture** (L0-L4 + Go)
- Implements **13 architectural contracts** for consistency
- Provides a **capability plugin system** for domain-specific agents
- Combines **Python flexibility** with **Go performance**
- Includes **comprehensive documentation** for capability developers

The project appears to be in active development, with a focus on code quality (audits, refactoring), documentation (handoff docs, contracts), and Go engine maturity.
