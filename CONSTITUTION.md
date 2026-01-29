# Jeeves Core Constitution

Architectural principles for the Go micro-kernel.

## Purpose

Jeeves Core is the **micro-kernel** for AI agent orchestration. It provides minimal, essential primitives that all higher layers depend on.

## Core Principles

### 1. Minimal Kernel Surface

The kernel provides only:
- **Envelope** - Immutable state container
- **Pipeline Configuration** - Declarative agent orchestration
- **Bounds Checking** - Resource quota enforcement
- **gRPC Services** - Inter-process communication

The kernel does NOT provide:
- LLM integrations (infrastructure layer)
- Tool implementations (capability layer)
- Prompt templates (capability layer)
- Database access (infrastructure layer)

### 2. Defense in Depth

All execution is bounded:

| Bound | Purpose |
|-------|---------|
| `max_iterations` | Prevent infinite agent loops |
| `max_llm_calls` | Control LLM cost |
| `max_agent_hops` | Limit pipeline depth |

Bounds are enforced at the kernel level. Capabilities cannot bypass them.

### 3. Immutable State Transitions

The `Envelope` follows strict transition rules:
- State changes create new envelope snapshots
- Counters only increment, never decrement
- Outputs append, never overwrite
- Terminal states are final

### 4. Declarative Configuration

Pipelines are defined as data, not code:

```go
PipelineConfig{
    Agents: []AgentConfig{...},
    RoutingRules: []RoutingRule{...},
}
```

This enables:
- Static analysis of pipeline structure
- Serialization for checkpointing
- Configuration without recompilation

### 5. Layer Isolation

```
Capabilities  ─────────────────────────────
     ↑ (cannot import kernel internals)
Infrastructure ────────────────────────────
     ↑ gRPC
Kernel ────────────────────────────────────
```

- Kernel exports public API via gRPC
- Internal packages are not exposed
- Breaking changes require major version bump

## Contribution Criteria

Changes to jeeves-core must demonstrate:

1. **Kernel necessity** - Why can't this live in infrastructure or capability layer?
2. **Minimal surface** - Does this add the minimum required API?
3. **Backward compatibility** - Does this break existing callers?
4. **Bounded execution** - Does this respect resource limits?

### Acceptable Changes

- New envelope fields (additive)
- New routing rule conditions
- Performance improvements
- Bug fixes with test coverage

### Requires Discussion

- New gRPC services
- Changes to bounds enforcement
- New required dependencies

### Not Acceptable

- LLM-specific logic
- Tool implementations
- Domain-specific features
- Capabilities importing kernel internals

## Testing Requirements

All changes must include:
- Unit tests for new functionality
- Integration tests for gRPC changes
- No decrease in coverage for core packages
