# Deployment Guide

## Prerequisites

- Rust 1.75+ (matches `rust-version` in Cargo.toml)
- Python 3.11+
- Linux or Windows host
- Redis (optional, for distributed state)

## Build

### Kernel (Rust)

```bash
cargo build --release
```

Binary: `target/release/jeeves-kernel`

### Infrastructure (Python)

```bash
cd python
pip install -e ".[all]"
```

### Docker

```bash
docker build -t jeeves-core .
```

## Run

```bash
# Kernel: IPC on 127.0.0.1:50051, metrics on 127.0.0.1:9090
./target/release/jeeves-kernel
```

The Python infrastructure is a library — capabilities compose their own entry points and import `jeeves_infra`.

---

## Kernel Configuration (Rust)

Configuration loaded from TOML/JSON config file or defaults.

### Server

| Field | Default | Description |
|-------|---------|-------------|
| `server.listen_addr` | `127.0.0.1:50051` | IPC TCP bind address |
| `server.metrics_addr` | `127.0.0.1:9090` | Prometheus metrics endpoint |

### Observability

| Field | Default | Description |
|-------|---------|-------------|
| `observability.log_level` | `info` | Log level (trace, debug, info, warn, error) |
| `observability.json_logs` | `false` | JSON-formatted log output |
| `observability.otlp_endpoint` | None | OpenTelemetry OTLP exporter endpoint |

### Default Resource Limits

| Field | Default | Description |
|-------|---------|-------------|
| `defaults.max_llm_calls` | `100` | Max LLM calls per envelope |
| `defaults.max_tool_calls` | `50` | Max tool calls per envelope |
| `defaults.max_agent_hops` | `10` | Max agent hops per envelope |
| `defaults.max_iterations` | `20` | Max iterations per envelope |
| `defaults.process_timeout` | `300s` | Process timeout (humantime format) |

### IPC Transport

| Field | Default | Description |
|-------|---------|-------------|
| `ipc.max_frame_bytes` | `5242880` (5 MB) | Maximum frame payload size |
| `ipc.max_connections` | `1000` | Concurrent TCP connections (semaphore backpressure) |
| `ipc.read_timeout_secs` | `30` | Idle read timeout per frame |
| `ipc.write_timeout_secs` | `10` | Write timeout per frame |
| `ipc.max_query_timeout_ms` | `30000` | Cap on CommBus query timeouts |
| `ipc.default_query_timeout_ms` | `5000` | Default CommBus query timeout |
| `ipc.stream_channel_capacity` | `64` | Channel buffer for streaming responses |

---

## Infrastructure Configuration (Python)

All configuration via environment variables.

### Gateway

| Env Var | Default | Description |
|---------|---------|-------------|
| `API_HOST` | `0.0.0.0` | API server bind address |
| `API_PORT` | `8000` | API server port |
| `CORS_ORIGINS` | `http://localhost:8000,http://localhost:3000` | Comma-separated allowed origins |
| `MAX_REQUEST_BODY_BYTES` | `1048576` (1 MB) | Max request body size |
| `DEBUG` | `false` | FastAPI debug mode |

### Kernel Connection

| Env Var | Default | Description |
|---------|---------|-------------|
| `ORCHESTRATOR_HOST` | `localhost` | Kernel IPC host |
| `ORCHESTRATOR_PORT` | `50051` | Kernel IPC port |
| `JEEVES_KERNEL_ADDRESS` | `localhost:50051` | Alternative: host:port for kernel client |

### LLM Providers

| Env Var | Default | Description |
|---------|---------|-------------|
| `JEEVES_LLM_ADAPTER` | `openai_http` | Provider: `openai_http`, `litellm`, `mock` |
| `JEEVES_LLM_MODEL` | — | Model identifier (required) |
| `JEEVES_LLM_BASE_URL` | — | API base URL |
| `JEEVES_LLM_API_KEY` | — | API key |
| `JEEVES_LLM_TIMEOUT` | `120` | Request timeout (seconds) |
| `JEEVES_LLM_MAX_RETRIES` | `3` | Max retry attempts |

### Pipeline Defaults

| Env Var | Default | Description |
|---------|---------|-------------|
| `CORE_MAX_ITERATIONS` | `3` | Pipeline iterations per request |
| `CORE_MAX_LLM_CALLS` | `10` | LLM invocations per request |
| `CORE_MAX_AGENT_HOPS` | `21` | Agent transitions per request |
| `CORE_MAX_INPUT_TOKENS` | `4096` | Max input tokens |
| `CORE_MAX_OUTPUT_TOKENS` | `2048` | Max output tokens |
| `CORE_MAX_CONTEXT_TOKENS` | `16384` | Max context window |

### Redis (Optional)

| Env Var | Default | Description |
|---------|---------|-------------|
| `REDIS_URL` | `redis://localhost:6379` | Redis connection URL |
| `REDIS_POOL_SIZE` | `10` | Connection pool size |
| `FEATURE_USE_REDIS_STATE` | `false` | Enable Redis state backend |

### Rate Limiting

| Env Var | Default | Description |
|---------|---------|-------------|
| `REQUESTS_PER_MINUTE` | `60` | Per-user rate limit |
| `RATE_LIMIT_INTERVAL_SECONDS` | `60.0` | Sliding window duration |

### Logging

| Env Var | Default | Description |
|---------|---------|-------------|
| `LOG_LEVEL` | `INFO` | Global log level |
| `FEATURE_ENABLE_TRACING` | `false` | OpenTelemetry tracing |
| `FEATURE_ENABLE_DEBUG_LOGGING` | `false` | Verbose debug output |

### Feature Flags

| Env Var | Default | Description |
|---------|---------|-------------|
| `FEATURE_ENABLE_DISTRIBUTED_MODE` | `false` | Multi-node deployment |
| `FEATURE_ENABLE_TRACING` | `false` | Distributed tracing |
| `FEATURE_MEMORY_SEMANTIC_MODE` | `log_and_use` | Semantic search mode |
| `FEATURE_MEMORY_WORKING_MEMORY` | `true` | Session summarization |
| `FEATURE_MEMORY_GRAPH_MODE` | `enabled` | Knowledge graph |

---

## Health Checks

**Kernel:** `GetSystemStatus` IPC method returns process counts, service health, orchestration sessions, and commbus stats.

**Infrastructure:**
- `GET /health` — Always returns 200 (liveness)
- `GET /ready` — Returns 200 when services are registered, 503 otherwise (readiness)

## Monitoring

Prometheus metrics exposed at `metrics_addr` (default `:9090`). OpenTelemetry traces via OTLP when `otlp_endpoint` is configured.

## Production Considerations

- Bind `listen_addr` to `0.0.0.0:50051` for non-localhost access
- Enable `json_logs` for structured log aggregation
- Tune `max_connections` based on expected client count
- Set `otlp_endpoint` for distributed tracing (Jaeger, Grafana Tempo)
- Set `CORS_ORIGINS` to your actual frontend domain(s)
- Set `LOG_LEVEL=WARNING` and `FEATURE_ENABLE_TRACING=true` for production
- Configure `JEEVES_LLM_*` vars for your LLM provider
- Redis credentials in `REDIS_URL` are automatically redacted in logs
- No TLS on kernel IPC — run kernel and infrastructure on the same host or behind a VPN
- Body size limit (1 MB default) applies to all HTTP endpoints
