# Install `just`: https://github.com/casey/just

default: check

check:
    cargo fmt --all -- --check
    cargo check --all-targets
    cargo test --no-fail-fast
    cargo clippy --all-targets -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps

test:
    cargo test --no-fail-fast

lint:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt --all

doc:
    cargo doc --no-deps --open

cpp-check:
    cmake -S . -B build-cpp
    cmake --build build-cpp -j 4
    ctest --test-dir build-cpp --output-on-failure
