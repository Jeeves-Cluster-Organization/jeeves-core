# Jeeves Core — Development Commands
# Install: https://github.com/casey/just

default: check

# Full check: compile + lint + test
check:
    cargo check
    cargo clippy -- -D warnings
    cargo test

# Run the kernel IPC server
run:
    cargo run

# Run tests only
test:
    cargo test

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Lint
lint:
    cargo clippy -- -D warnings

# Format
fmt:
    cargo fmt

# Build release binary
build:
    cargo build --release

# Clean build artifacts
clean:
    cargo clean
