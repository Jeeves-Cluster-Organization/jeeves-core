.PHONY: check fmt-check lint test fmt setup

# Run all CI checks (formatting, linting, tests)
check: fmt-check lint test

# Verify code formatting without modifying files
fmt-check:
	cargo fmt -- --check

# Run clippy with warnings as errors
lint:
	cargo clippy -- -D warnings

# Run all tests
test:
	cargo test

# Auto-format code
fmt:
	cargo fmt

# Install git hooks for local CI
setup:
	./scripts/install-hooks.sh
