# Stage 1: Build Rust kernel + Python wheel via maturin
FROM rust:1.75-slim-bookworm AS builder
RUN pip install maturin
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs pyproject.toml ./
COPY src/ src/
COPY python/ python/
RUN maturin build --release --out /wheels

# Stage 2: Runtime
FROM python:3.12-slim-bookworm
COPY --from=builder /wheels/*.whl /tmp/
RUN pip install /tmp/*.whl && rm /tmp/*.whl
EXPOSE 50051
CMD ["jeeves-kernel"]
