# Observability Metrics - Implementation Guide

## Overview

This implementation adds comprehensive Prometheus metrics instrumentation to the entire jeeves-core stack. Metrics are collected from both Go (coreengine) and Python (avionics) layers and exposed via `/metrics` endpoints.

## What Was Implemented

### Go Side (coreengine)

1. **Metrics Package** (`coreengine/observability/metrics.go`)
   - Pipeline execution metrics (count, duration)
   - Agent execution metrics (count, duration)
   - LLM call metrics (count, duration)
   - gRPC request metrics (count, duration)

2. **Instrumentation Points**
   - `runtime/runtime.go`: Pipeline execution tracking
   - `agents/agent.go`: Agent execution tracking
   - `grpc/interceptors.go`: gRPC request tracking
   - `grpc/server.go`: `/metrics` endpoint (port 9090)

3. **Metrics Exposed**
   ```promql
   # Pipeline metrics
   jeeves_pipeline_executions_total{pipeline, status}
   jeeves_pipeline_duration_seconds{pipeline}

   # Agent metrics
   jeeves_agent_executions_total{agent, status}
   jeeves_agent_duration_seconds{agent}

   # LLM metrics (Go layer)
   jeeves_llm_calls_total{provider, model, status}
   jeeves_llm_duration_seconds{provider, model}

   # gRPC metrics
   jeeves_grpc_requests_total{method, status}
   jeeves_grpc_request_duration_seconds{method}
   ```

### Python Side (avionics)

1. **Extended Metrics** (`avionics/observability/metrics.py`)
   - LLM provider metrics (calls, duration, tokens)
   - HTTP gateway metrics (requests, duration)
   - Helper functions: `record_llm_call()`, `record_llm_tokens()`, `record_http_request()`

2. **Instrumentation Points**
   - `llm/gateway.py`: LLM call tracking with token counts
   - `gateway/main.py`: HTTP request middleware + `/metrics` endpoint

3. **Metrics Exposed**
   ```promql
   # LLM provider metrics (Python layer)
   jeeves_llm_provider_calls_total{provider, model, status}
   jeeves_llm_provider_duration_seconds{provider, model}
   jeeves_llm_tokens_total{provider, model, type}  # type: prompt|completion

   # HTTP gateway metrics
   jeeves_http_requests_total{method, path, status_code}
   jeeves_http_request_duration_seconds{method, path}
   ```

### Infrastructure

1. **Prometheus Server** (`docker/docker-compose.yml`)
   - Service: `prometheus` on port 9090
   - 30-day retention
   - Auto-scrapes all endpoints

2. **Prometheus Configuration** (`docker/prometheus.yml`)
   - Scrapes gateway:8000/metrics (Python)
   - Scrapes orchestrator:8000/metrics (Python)
   - Scrapes orchestrator:9090/metrics (Go)
   - 15-second scrape interval

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                 Prometheus Server                    │
│                  localhost:9090                      │
│                                                       │
│  Scrapes every 15s:                                  │
│  - gateway:8000/metrics       (Python HTTP/LLM)     │
│  - orchestrator:8000/metrics  (Python HTTP/LLM)     │
│  - orchestrator:9090/metrics  (Go Pipeline/gRPC)    │
└─────────────────────────────────────────────────────┘
                         ▲
                         │ scrapes
         ┌───────────────┼───────────────┐
         │               │               │
    ┌────▼─────┐   ┌─────▼────┐   ┌─────▼────┐
    │ Gateway  │   │Orchestr. │   │Orchestr. │
    │ :8000    │   │ :8000    │   │ :9090    │
    │ Python   │   │ Python   │   │   Go     │
    │ /metrics │   │ /metrics │   │ /metrics │
    └──────────┘   └──────────┘   └──────────┘
         │              │               │
         │              │               │
    HTTP requests  LLM Gateway    Pipeline/gRPC
    (FastAPI)      (LLM calls)    (core engine)
```

## Getting Started

### 1. Start Services

```bash
cd docker

# Start infrastructure (includes Prometheus)
docker-compose up -d postgres llama-server prometheus

# Start application services
docker-compose up -d gateway orchestrator

# Verify all services are up
docker-compose ps
```

### 2. Verify Metrics Endpoints

```bash
# Python Gateway metrics
curl http://localhost:8000/metrics | grep jeeves_

# Python Orchestrator metrics
curl http://localhost:8000/metrics | grep jeeves_

# Go Orchestrator metrics (note: port 9091 maps to container 9090)
curl http://localhost:9091/metrics | grep jeeves_
```

### 3. Access Prometheus UI

Open browser to: http://localhost:9090

Query examples:
```promql
# Pipeline execution rate
rate(jeeves_pipeline_executions_total[5m])

# Agent execution latency (P95)
histogram_quantile(0.95, rate(jeeves_agent_duration_seconds_bucket[5m]))

# LLM token usage by provider
rate(jeeves_llm_tokens_total[5m])

# HTTP error rate
rate(jeeves_http_requests_total{status_code=~"5.."}[5m])
```

## Running Tests

### Prerequisites

Due to network restrictions in the development environment, you need to run tests in the Docker environment where dependencies are pre-installed.

### Go Tests

```bash
# In Docker (dependencies available)
docker-compose run --rm test go test ./coreengine/... -v

# Specific observability tests
docker-compose run --rm test go test ./coreengine/observability/... -v
```

### Python Tests

```bash
# In Docker
docker-compose run --rm test pytest avionics/observability/ -v

# Test LLM gateway metrics
docker-compose run --rm test pytest avionics/llm/test_gateway.py -v -k metrics
```

### Integration Test (End-to-End)

```bash
# Start all services
docker-compose up -d

# Execute a test pipeline
curl -X POST http://localhost:8000/api/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello", "session_id": "test-123"}'

# Verify metrics recorded
curl http://localhost:9090/api/v1/query?query=jeeves_pipeline_executions_total

# Should show count > 0
```

## Metrics Reference

### Pipeline Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `jeeves_pipeline_executions_total` | Counter | pipeline, status | Total pipeline executions (status: success, error, terminated) |
| `jeeves_pipeline_duration_seconds` | Histogram | pipeline | Pipeline execution duration in seconds |

### Agent Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `jeeves_agent_executions_total` | Counter | agent, status | Total agent executions (status: success, error) |
| `jeeves_agent_duration_seconds` | Histogram | agent | Agent execution duration in seconds |

### LLM Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `jeeves_llm_calls_total` | Counter | provider, model, status | Total LLM API calls (Go layer) |
| `jeeves_llm_duration_seconds` | Histogram | provider, model | LLM call duration (Go layer) |
| `jeeves_llm_provider_calls_total` | Counter | provider, model, status | Total LLM calls (Python layer) |
| `jeeves_llm_provider_duration_seconds` | Histogram | provider, model | LLM call duration (Python layer) |
| `jeeves_llm_tokens_total` | Counter | provider, model, type | Token usage (type: prompt, completion) |

### HTTP Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `jeeves_http_requests_total` | Counter | method, path, status_code | Total HTTP requests to FastAPI gateway |
| `jeeves_http_request_duration_seconds` | Histogram | method, path | HTTP request duration in seconds |

### gRPC Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `jeeves_grpc_requests_total` | Counter | method, status | Total gRPC requests (status: OK, InvalidArgument, etc.) |
| `jeeves_grpc_request_duration_seconds` | Histogram | method | gRPC request duration in seconds |

## PromQL Query Examples

### Performance Monitoring

```promql
# Request rate (requests per second)
rate(jeeves_pipeline_executions_total[1m])

# P50, P95, P99 latency
histogram_quantile(0.50, rate(jeeves_pipeline_duration_seconds_bucket[5m]))
histogram_quantile(0.95, rate(jeeves_pipeline_duration_seconds_bucket[5m]))
histogram_quantile(0.99, rate(jeeves_pipeline_duration_seconds_bucket[5m]))

# Average agent execution time
avg(rate(jeeves_agent_duration_seconds_sum[5m]))
  /
avg(rate(jeeves_agent_duration_seconds_count[5m]))
```

### Error Tracking

```promql
# Pipeline error rate
rate(jeeves_pipeline_executions_total{status="error"}[5m])
  /
rate(jeeves_pipeline_executions_total[5m])

# HTTP 5xx error rate
rate(jeeves_http_requests_total{status_code=~"5.."}[5m])

# Failed LLM calls by provider
rate(jeeves_llm_provider_calls_total{status="error"}[5m])
```

### Resource Usage

```promql
# LLM tokens per second
rate(jeeves_llm_tokens_total[1m])

# Top 5 slowest agents
topk(5,
  histogram_quantile(0.95,
    rate(jeeves_agent_duration_seconds_bucket[5m])
  )
) by (agent)

# Top 5 slowest HTTP endpoints
topk(5,
  histogram_quantile(0.95,
    rate(jeeves_http_request_duration_seconds_bucket[5m])
  )
) by (path)
```

### Business Metrics

```promql
# Total LLM calls in last hour
increase(jeeves_llm_provider_calls_total[1h])

# Token usage breakdown by model
sum(rate(jeeves_llm_tokens_total[5m])) by (model, type)

# Pipeline success rate over 24 hours
sum(increase(jeeves_pipeline_executions_total{status="success"}[24h]))
  /
sum(increase(jeeves_pipeline_executions_total[24h]))
```

## Alerting (Future Enhancement)

Example alert rules (to be added in Phase 3):

```yaml
groups:
  - name: jeeves_alerts
    rules:
      - alert: HighErrorRate
        expr: |
          rate(jeeves_pipeline_executions_total{status="error"}[5m])
            /
          rate(jeeves_pipeline_executions_total[5m])
          > 0.05
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Pipeline error rate above 5%"

      - alert: HighLatency
        expr: |
          histogram_quantile(0.95,
            rate(jeeves_pipeline_duration_seconds_bucket[5m])
          ) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "P95 pipeline latency above 10s"
```

## Troubleshooting

### Metrics Not Showing Up

1. **Check endpoint is accessible**
   ```bash
   curl http://localhost:8000/metrics
   curl http://localhost:9091/metrics
   ```

2. **Check Prometheus targets**
   - Go to http://localhost:9090/targets
   - All targets should show "UP"

3. **Check Prometheus logs**
   ```bash
   docker logs jeeves-prometheus
   ```

### Go Dependencies Issue

If you see network errors when building Go code:

```bash
# Option 1: Use Docker build (recommended)
docker-compose build orchestrator

# Option 2: Use Go module vendor
go mod vendor
go build -mod=vendor ./...
```

### Python Import Errors

If metrics fail to import in local development:

```bash
# Install dependencies
pip install -e avionics[dev]
pip install prometheus-client

# Or use Docker
docker-compose run --rm test python -c "from avionics.observability.metrics import *"
```

## Next Steps (Phase 2 & 3)

### Phase 2: Distributed Tracing
- Add OpenTelemetry instrumentation
- Deploy Jaeger for trace visualization
- Trace requests across Go/Python boundary

### Phase 3: Alerting & SLOs
- Define SLOs for key metrics
- Create alert rules in Prometheus
- Deploy Alertmanager for notifications

### Phase 4: Dashboards
- Deploy Grafana
- Create 5 core dashboards:
  1. Service Overview
  2. Pipeline Performance
  3. LLM Provider Health
  4. Infrastructure
  5. Business Metrics

## Contributing

When adding new metrics:

1. **Follow naming convention**: `jeeves_<component>_<metric>_<unit>`
2. **Use appropriate type**: Counter (cumulative), Gauge (current value), Histogram (distribution)
3. **Keep labels low-cardinality**: Don't use request IDs or timestamps as labels
4. **Document in this README**: Add to metrics reference table
5. **Add PromQL examples**: Show how to query the new metric

## Support

For issues or questions:
- Check existing metrics: http://localhost:9090
- Review logs: `docker logs jeeves-orchestrator`
- See implementation analysis: `OBSERVABILITY_IMPLEMENTATION_ANALYSIS.md`
