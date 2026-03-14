# Deployment Guide

Jeeves Core is a library, not a standalone service. It runs embedded in Python capabilities (via PyO3) or as an MCP stdio server for tool aggregation.

## PyO3 (Primary Pattern)

```bash
# Build and install
cd jeeves-core
pip install -e .

# Verify
python -c "from jeeves_core import PipelineRunner; print('ok')"
```

Capabilities import and use the kernel directly:

```python
# capability/app.py
from jeeves_core import PipelineRunner
runner = PipelineRunner.from_json("pipeline.json", prompts_dir="prompts/")
result = runner.run("hello", user_id="user1")
```

## MCP Stdio Server

For tool aggregation (proxying upstream MCP servers):

```bash
cargo build --release --features mcp-stdio
```

Binary: `target/release/jeeves-kernel`

Run with upstream MCP servers:

```bash
JEEVES_MCP_SERVERS='[
  {"name": "fs", "transport": "stdio", "command": "npx", "args": ["-y", "@anthropic-ai/mcp-filesystem"]},
  {"name": "api", "transport": "http", "url": "http://localhost:8081/mcp"}
]' ./target/release/jeeves-kernel
```

The server reads JSON-RPC from stdin and writes to stdout. Tracing goes to stderr.

## Configuration

### Environment Variables

#### LLM

| Env Var | Default | Description |
|---------|---------|-------------|
| `LLM_API_KEY` | — | API key for LLM provider (required for real calls) |
| `LLM_MODEL` | `gpt-4o-mini` | Model name |
| `LLM_BASE_URL` | — | OpenAI-compatible API base URL |

#### Pipeline Bounds

| Env Var | Default | Description |
|---------|---------|-------------|
| `CORE_MAX_ITERATIONS` | `20` | Max iterations per envelope |
| `CORE_MAX_LLM_CALLS` | `100` | Max LLM calls per envelope |
| `CORE_MAX_AGENT_HOPS` | `10` | Max agent hops per envelope |

#### Rate Limiting

| Env Var | Default | Description |
|---------|---------|-------------|
| `CORE_RATE_LIMIT_RPM` | `60` | Per-user requests per minute |
| `CORE_RATE_LIMIT_RPH` | `1000` | Per-user requests per hour |
| `CORE_RATE_LIMIT_BURST` | `10` | Burst size (per 10s window) |

#### Background Cleanup

| Env Var | Default | Description |
|---------|---------|-------------|
| `CORE_CLEANUP_INTERVAL` | `300` | Seconds between cleanup cycles |
| `CORE_SESSION_RETENTION` | `3600` | Seconds to keep stale sessions |

#### Observability

| Env Var | Default | Description |
|---------|---------|-------------|
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |
| `JEEVES_LOG_FORMAT` | `text` | `text` or `json` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | — | OpenTelemetry OTLP exporter (optional) |

#### MCP

| Env Var | Description |
|---------|-------------|
| `JEEVES_MCP_SERVERS` | JSON array of upstream MCP server configs |

## Build from Source

```bash
# Library only (for testing)
cargo build

# MCP stdio binary
cargo build --release --features mcp-stdio

# PyO3 module
pip install -e .

# All features (for CI)
cargo test --all-features
cargo clippy --all-features
```

## Prerequisites

- Rust 1.75+
- Python 3.10+ (for PyO3)
- `LLM_API_KEY` set for real LLM calls (tests use mock provider)
