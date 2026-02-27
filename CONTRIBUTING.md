# Contributing to Jeeves Core

Thank you for your interest in contributing to Jeeves Core!

## Before You Start

Please read our [CONSTITUTION.md](CONSTITUTION.md) to understand the architectural principles. Changes to the micro-kernel require careful consideration of their impact on the entire ecosystem.

## Contribution Guidelines

### What We're Looking For

Jeeves Core is the **micro-kernel** layer. Contributions should:

1. **Demonstrate kernel necessity** - Explain why this can't live in the infrastructure or capability layer
2. **Add minimal API surface** - The kernel should remain minimal
3. **Maintain backward compatibility** - Breaking changes require major version bumps
4. **Include comprehensive tests** - All new code needs test coverage

### Layer Boundaries

Before contributing, verify your change belongs in this layer:

| Change Type | Belongs In |
|-------------|------------|
| Process lifecycle management | jeeves-core (here) |
| Resource quota enforcement | jeeves-core (here) |
| Interrupt handling (HITL) | jeeves-core (here) |
| Inter-process communication | jeeves-core (here) |
| LLM provider adapters | jeeves-airframe |
| Database clients | jeeves-airframe |
| Domain-specific tools | capability layer |
| Prompt templates | capability layer |

## How to Contribute

### Reporting Issues

Please use the following format for issues:

\`\`\`markdown
## Summary
Brief description of the issue or feature request.

## Layer Verification
- [ ] This belongs in the kernel layer (not infra or capability)
- [ ] I've read CONSTITUTION.md

## Current Behavior
What happens now?

## Expected Behavior
What should happen?

## Steps to Reproduce (for bugs)
1. Step one
2. Step two
3. ...

## Environment
- Rust version: (run \`rustc --version\`)
- OS:
- Commit hash:

## Additional Context
Any other relevant information.
\`\`\`

### Submitting Pull Requests

1. Fork the repository
2. Create a feature branch from \`main\`
3. Make your changes with tests
4. Ensure all tests pass: \`cargo test\`
5. Ensure code is formatted: \`cargo fmt\`
6. Ensure no clippy warnings: \`cargo clippy\`
7. Submit a PR with the following template:

\`\`\`markdown
## Summary
What does this PR do?

## Layer Justification
Why does this belong in the kernel layer?

## Changes
- List of changes

## Testing
- How was this tested?
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] All tests pass (\`cargo test\`)

## Checklist
- [ ] I've read CONSTITUTION.md
- [ ] Tests pass locally
- [ ] Code is formatted (\`cargo fmt\`)
- [ ] No clippy warnings (\`cargo clippy\`)
- [ ] No breaking changes (or major version bump discussed)
- [ ] Documentation updated if needed
\`\`\`

## Development Setup

\`\`\`bash
# Clone the repository
git clone https://github.com/Jeeves-Cluster-Organization/jeeves-core.git
cd jeeves-core

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Run tests
cargo test

# Run with coverage
cargo tarpaulin --lib --out Stdout

# Build release
cargo build --release
\`\`\`

## Code Style

- Follow standard Rust conventions
- Run \`cargo fmt\` before committing (enforces rustfmt style)
- Address all \`cargo clippy\` warnings
- Use meaningful variable and function names
- Add documentation comments (///) for public APIs
- Keep functions small and focused

### Example Documentation

\`\`\`rust
/// Create a new process with the specified parameters.
///
/// # Arguments
///
/// * \`pid\` - Unique process identifier
/// * \`priority\` - Scheduling priority (High, Normal, Low)
/// * \`quota\` - Optional resource quota limits
///
/// # Returns
///
/// Returns the created ProcessControlBlock or an error if:
/// - Process ID already exists
/// - Rate limit exceeded for this user
/// - Invalid priority value
///
/// # Example
///
/// \`\`\`
/// let process = kernel.create_process(
///     "proc-123".to_string(),
///     SchedulingPriority::Normal,
///     Some(quota),
/// )?;
/// \`\`\`
pub fn create_process(
    &mut self,
    pid: String,
    priority: SchedulingPriority,
    quota: Option<ResourceQuota>,
) -> Result<ProcessControlBlock> {
    // Implementation
}
\`\`\`

## Testing Requirements

All contributions must include tests:

### Unit Tests

Place unit tests in the same file as the code:

\`\`\`rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_process() {
        let mut kernel = Kernel::new();
        let result = kernel.create_process(
            "test-123".to_string(),
            SchedulingPriority::Normal,
            None,
        );
        assert!(result.is_ok());
    }
}
\`\`\`

### Integration Tests

Place integration tests in \`tests/\` directory:

\`\`\`rust
use jeeves_core::kernel::Kernel;

#[test]
fn test_full_workflow() {
    let mut kernel = Kernel::new();
    // Test complete workflow
}
\`\`\`

### Coverage Requirements

- New code should maintain or improve overall coverage
- Critical modules (lifecycle, resources, interrupts) must have >80% coverage
- Run \`cargo tarpaulin\` to check coverage locally

## Commit Message Guidelines

Follow conventional commits format:

\`\`\`
<type>(<scope>): <subject>

<body>

<footer>
\`\`\`

**Types:**
- \`feat\`: New feature
- \`fix\`: Bug fix
- \`docs\`: Documentation changes
- \`test\`: Adding or updating tests
- \`refactor\`: Code refactoring
- \`perf\`: Performance improvements
- \`chore\`: Maintenance tasks

**Examples:**

\`\`\`
feat(kernel): Add background cleanup service

Implement automatic garbage collection of zombie processes
with configurable retention periods.

Closes #123
\`\`\`

\`\`\`
fix(interrupts): Prevent duplicate interrupt creation

Check for existing pending interrupts before creating new ones
to avoid race conditions.
\`\`\`

## Questions?

Open an issue with the \`question\` label or start a discussion.

## Code of Conduct

Be respectful, constructive, and professional in all interactions.

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
