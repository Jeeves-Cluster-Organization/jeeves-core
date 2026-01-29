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
| Pipeline orchestration | jeeves-core (here) |
| Envelope state management | jeeves-core (here) |
| Resource quota enforcement | jeeves-core (here) |
| LLM provider adapters | jeeves-infra |
| Database clients | jeeves-infra |
| Domain-specific tools | capability layer |
| Prompt templates | capability layer |

## How to Contribute

### Reporting Issues

Please use the following format for issues:

```markdown
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
- Go version:
- OS:
- Commit hash:

## Additional Context
Any other relevant information.
```

### Submitting Pull Requests

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes with tests
4. Ensure all tests pass: `go test ./...`
5. Submit a PR with the following template:

```markdown
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

## Checklist
- [ ] I've read CONSTITUTION.md
- [ ] Tests pass locally
- [ ] No breaking changes (or major version bump discussed)
- [ ] Documentation updated if needed
```

## Development Setup

```bash
# Clone the repository
git clone https://github.com/Jeeves-Cluster-Organization/jeeves-core.git
cd jeeves-core

# Run tests
go test ./...

# Run with coverage
go test ./... -cover

# Build
go build ./...
```

## Code Style

- Follow standard Go conventions
- Run `go fmt` before committing
- Use meaningful variable and function names
- Add comments for exported functions

## Questions?

Open an issue with the `question` label or start a discussion.

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
