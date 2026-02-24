# Deployment Guide

## Prerequisites

- Rust 1.75+ (matches `rust-version` in Cargo.toml)
- Linux or Windows host

## Build

```bash
cargo build --release
```

Binary: `target/release/jeeves-kernel`

## Run

```bash
# Defaults: IPC on 127.0.0.1:50051, metrics on 127.0.0.1:9090
./target/release/jeeves-kernel
```

## Configuration

Configuration is loaded from a TOML/JSON config file or defaults. All fields have sensible defaults for single-node deployment.

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

## Health Check

`GetSystemStatus` IPC method returns process counts, service health, orchestration sessions, and commbus stats.

## Monitoring

Prometheus metrics exposed at `metrics_addr` (default `:9090`). OpenTelemetry traces via OTLP when `otlp_endpoint` is configured.

## Production Considerations

- Bind `listen_addr` to `0.0.0.0:50051` for non-localhost access
- Enable `json_logs` for structured log aggregation
- Tune `max_connections` based on expected client count
- Set `otlp_endpoint` for distributed tracing (Jaeger, Grafana Tempo)
- No TLS or authentication — run behind a firewall or VPN for non-localhost deployments
