# Jeeves Core Constitution

Architectural principles for the Rust micro-kernel.

## Purpose

Jeeves Core is the **micro-kernel** for AI agent orchestration. It provides minimal, essential primitives that all higher layers depend on.

## Core Principles

### 1. Minimal Kernel Surface

The kernel provides only:
- **Process Lifecycle** - Unix-like state machine for agent execution
- **Resource Quotas** - Defense-in-depth bounds enforcement
- **Interrupt Handling** - Human-in-the-loop patterns
- **Inter-Process Communication** - Kernel-mediated message bus (CommBus)
- **IPC Services** - Cross-language communication interface

The kernel does NOT provide:
- LLM integrations (infrastructure layer)
- Tool implementations (capability layer)
- Prompt templates (capability layer)
- Database access (infrastructure layer)

### 2. Defense in Depth

All execution is bounded:

| Bound | Purpose |
|-------|---------|
| \`max_iterations\` | Prevent infinite agent loops |
| \`max_llm_calls\` | Control LLM API costs |
| \`max_agent_hops\` | Limit pipeline depth |
| \`max_output_tokens\` | Prevent excessive token generation |

Bounds are enforced at the kernel level. Capabilities cannot bypass them.

### 3. Kernel-Mediated Communication

The CommBus provides three IPC patterns:

- **Events** - Pub/sub with fan-out to all subscribers
- **Commands** - Fire-and-forget to single handler
- **Queries** - Request/response with timeout

All inter-agent communication flows through the kernel, enabling:
- Message quotas and rate limiting
- Full tracing and observability
- Security and access control
- Fault isolation

### 4. Process Isolation

Processes follow Unix-like principles:

\`\`\`
New → Ready → Running → Blocked → Terminated → Zombie
\`\`\`

- Each process has isolated resource quota
- Processes cannot directly access other processes
- All IPC is kernel-mediated
- Panics in one process don't crash the kernel (panic recovery)

### 5. Type Safety

The Rust type system provides compile-time guarantees:

- **No null pointer dereferences** - Option<T> instead of null
- **No data races** - Ownership and borrow checking
- **No use-after-free** - Lifetime tracking
- **Exhaustive pattern matching** - All cases must be handled

### 6. Layer Isolation

\`\`\`
Capabilities  ─────────────────────────────
     ↑ (cannot import kernel internals)
Infrastructure ────────────────────────────
     ↑ IPC
Kernel ────────────────────────────────────
\`\`\`

- Kernel exports public API via IPC
- Internal modules are not exposed
- Breaking changes require major version bump

## Contribution Criteria

Changes to jeeves-core must demonstrate:

1. **Kernel necessity** - Why can't this live in infrastructure or capability layer?
2. **Minimal surface** - Does this add the minimum required API?
3. **Backward compatibility** - Does this break existing callers?
4. **Bounded execution** - Does this respect resource limits?
5. **Type safety** - Does this leverage Rust's type system?

### Acceptable Changes

- New process state transitions (with tests)
- New interrupt types (with proper validation)
- Performance improvements (with benchmarks)
- Bug fixes with comprehensive test coverage
- Additional resource quota types

### Requires Discussion

- New IPC services
- Changes to bounds enforcement
- New required dependencies
- Breaking API changes

### Not Acceptable

- LLM-specific logic
- Tool implementations
- Domain-specific features
- Capabilities importing kernel internals
- Unsafe code blocks (without strong justification)

## Testing Requirements

All changes must include:

- **Unit tests** for new functionality
- **Integration tests** for IPC changes
- **No decrease in coverage** for core packages
- **All clippy warnings addressed**
- **Code formatted with \`cargo fmt\`**

### Coverage Targets

- **Overall**: Maintain 80%+ line coverage
- **Critical modules** (lifecycle, resources, interrupts): 85%+ coverage
- **New modules**: 80%+ coverage from day one

## Safety Requirements

- **Zero unsafe code blocks** unless absolutely necessary
  - If unsafe is required, document invariants thoroughly
  - Provide safety proof in comments
  - Add comprehensive tests

- **Panic recovery** for all external operations
  - Use \`with_recovery()\` wrapper for potentially panicking code
  - Never let agent panics crash the kernel

- **Resource cleanup**
  - All resources must have defined cleanup paths
  - Use RAII patterns (Drop trait) for automatic cleanup
  - Background cleanup service handles zombie processes

## Performance Guidelines

- **Prefer zero-copy** operations via Arc and Cow
- **Minimize lock contention** - use fine-grained locking
- **Use async/await** for I/O-bound operations
- **Avoid allocations** in hot paths
- **Profile before optimizing** - use cargo-flamegraph

### Acceptable Performance Trade-offs

- Safety over speed (bounds checking)
- Correctness over optimization
- Clarity over cleverness

## Documentation Standards

All public APIs must have:

\`\`\`rust
/// Brief one-line description.
///
/// More detailed explanation of what this does and why.
///
/// # Arguments
///
/// * \`arg1\` - Description of arg1
/// * \`arg2\` - Description of arg2
///
/// # Returns
///
/// Description of return value and error conditions.
///
/// # Example
///
/// \\\`\\\`\\\`
/// let result = function(arg1, arg2)?;
/// \\\`\\\`\\\`
///
/// # Safety (for unsafe functions only)
///
/// Explanation of invariants and safety requirements.
pub fn function(arg1: Type1, arg2: Type2) -> Result<ReturnType> {
    // Implementation
}
\`\`\`

## Questions?

If you're unsure whether a change belongs in the kernel layer:

1. Could this be implemented in jeeves-infra? → Do it there
2. Is this domain-specific? → Belongs in capability layer
3. Does this require kernel primitives? → Maybe kernel (discuss first)

Open an issue for architectural discussions before implementing large changes.
