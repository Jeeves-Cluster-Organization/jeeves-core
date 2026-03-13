# Stage 1: Build Rust binary
FROM rust:1.75-slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release

# Stage 2: Minimal runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/jeeves-kernel /usr/local/bin/jeeves-kernel
EXPOSE 8080
CMD ["jeeves-kernel", "run"]
