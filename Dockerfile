# Stage 1: Build Rust kernel
FROM rust:1.75-slim-bookworm AS rust-builder
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src/ src/
RUN cargo build --release --bin jeeves-kernel

# Stage 2: Build Python wheel
FROM python:3.12-slim-bookworm AS py-builder
WORKDIR /build
COPY python/ python/
RUN pip wheel --no-deps --wheel-dir /wheels python/

# Stage 3: Runtime
FROM python:3.12-slim-bookworm
COPY --from=rust-builder /build/target/release/jeeves-kernel /usr/local/bin/
COPY --from=py-builder /wheels/*.whl /tmp/
RUN pip install /tmp/*.whl && rm /tmp/*.whl
EXPOSE 50051
CMD ["jeeves-kernel"]
