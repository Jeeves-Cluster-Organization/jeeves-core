# Deployment Guide

## Prerequisites

- Rust 1.75+ (matches `rust-version` in Cargo.toml)
- Linux or Windows host

## Build

```bash
cargo build --release
```

Binary: `target/release/jeeves-kernel`

### Docker

```bash
docker build -t jeeves-core .
docker run -p 8080:8080 jeeves-core
```

## Run

```bash
# HTTP on 0.0.0.0:8080 (default)
./target/release/jeeves-kernel run

# Custom address
./target/release/jeeves-kernel run --http-addr 0.0.0.0:9000

# With LLM configuration
./target/release/jeeves-kernel run \
  --llm-model gpt-4o \
  --llm-base-url https://api.openai.com/v1
```

---

## Configuration

### Server

| Field | Default | Env Var | Description |
|-------|---------|---------|-------------|
| `server.listen_addr` | `0.0.0.0:8080` | `JEEVES_HTTP_ADDR` | HTTP bind address |
| `server.metrics_addr` | `127.0.0.1:9090` | `JEEVES_METRICS_ADDR` | Prometheus metrics endpoint |

### Observability

| Field | Default | Env Var | Description |
|-------|---------|---------|-------------|
| `observability.log_level` | `info` | `RUST_LOG` | Log level (trace, debug, info, warn, error) |
| `observability.json_logs` | `false` | `JEEVES_LOG_FORMAT=json` | JSON-formatted log output |
| `observability.otlp_endpoint` | None | `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry OTLP exporter |

### Default Resource Limits

| Field | Default | Env Var | Description |
|-------|---------|---------|-------------|
| `defaults.max_llm_calls` | `100` | `CORE_MAX_LLM_CALLS` | Max LLM calls per envelope |
| `defaults.max_agent_hops` | `10` | `CORE_MAX_AGENT_HOPS` | Max agent hops per envelope |
| `defaults.max_iterations` | `20` | `CORE_MAX_ITERATIONS` | Max iterations per envelope |
| `defaults.process_timeout` | `300s` | — | Process timeout |

### Rate Limiting

| Env Var | Default | Description |
|---------|---------|-------------|
| `CORE_RATE_LIMIT_RPM` | `60` | Per-user requests per minute |
| `CORE_RATE_LIMIT_RPH` | `1000` | Per-user requests per hour |
| `CORE_RATE_LIMIT_BURST` | `10` | Burst size (per 10s window) |

### LLM

| Env Var / CLI Flag | Description |
|-------------------|-------------|
| `LLM_API_KEY` / `--llm-api-key` | API key for LLM provider |
| `LLM_MODEL` / `--llm-model` | Model name (default: `gpt-4o-mini`) |
| `LLM_BASE_URL` / `--llm-base-url` | OpenAI-compatible API base URL |

---

## Health Checks

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Liveness — always returns 200 |
| `GET /ready` | Readiness — returns 200 when kernel is operational |
| `GET /api/v1/status` | System status — process counts, health metrics |

## Monitoring

Prometheus metrics exposed at `metrics_addr` (default `:9090`). OpenTelemetry traces via OTLP when `OTEL_EXPORTER_OTLP_ENDPOINT` is set.

## Production Considerations

- Bind `JEEVES_HTTP_ADDR` to `0.0.0.0:8080` for non-localhost access
- Set `JEEVES_LOG_FORMAT=json` for structured log aggregation
- Set `OTEL_EXPORTER_OTLP_ENDPOINT` for distributed tracing (Jaeger, Grafana Tempo)
- Configure `LLM_API_KEY` and `LLM_MODEL` for your LLM provider
- Tune `CORE_MAX_*` bounds based on expected workload
