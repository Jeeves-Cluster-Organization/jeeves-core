# Observability Improvements Plan

**Date:** 2026-01-23
**Status:** Planned
**Priority:** HIGH
**Timeline:** 4-6 weeks for full implementation

---

## Current State

### What Exists ✅
1. **Structured Logging**
   - Logger protocol throughout all layers
   - gRPC interceptors log all requests with duration
   - CommBus middleware logs message flow
   - Context binding for request correlation

2. **Health Checks**
   - `/health` endpoints in gateway and orchestrator
   - Dependency health checks (PostgreSQL, llama-server)
   - ProcessControlBlock for request lifecycle tracking

3. **Event System**
   - Event sourcing in PostgreSQL for audit trail
   - Real-time event emission via EventContext
   - AgentEventType enum for structured events

### What's Missing ⚠️
1. **Metrics Collection** - No Prometheus/StatsD instrumentation
2. **Distributed Tracing** - No OpenTelemetry integration
3. **Alerting** - No alert rules or SLO definitions
4. **Dashboards** - No operational visualization
5. **Performance Profiling** - No baseline metrics

---

## Phase 1: Metrics Instrumentation (Weeks 1-2)

### 1.1 Add Prometheus Client Libraries

**Go (coreengine):**
```go
import (
    "github.com/prometheus/client_golang/prometheus"
    "github.com/prometheus/client_golang/prometheus/promauto"
    "github.com/prometheus/client_golang/prometheus/promhttp"
)

// Define metrics
var (
    pipelineExecutions = promauto.NewCounterVec(
        prometheus.CounterOpts{
            Name: "jeeves_pipeline_executions_total",
            Help: "Total number of pipeline executions",
        },
        []string{"pipeline", "status"}, // status: success, error, timeout
    )

    pipelineLatency = promauto.NewHistogramVec(
        prometheus.HistogramOpts{
            Name: "jeeves_pipeline_duration_seconds",
            Help: "Pipeline execution duration in seconds",
            Buckets: []float64{0.1, 0.5, 1, 2, 5, 10, 30, 60},
        },
        []string{"pipeline"},
    )

    agentExecutions = promauto.NewCounterVec(
        prometheus.CounterOpts{
            Name: "jeeves_agent_executions_total",
            Help: "Total number of agent executions",
        },
        []string{"agent", "status"},
    )

    agentLatency = promauto.NewHistogramVec(
        prometheus.HistogramOpts{
            Name: "jeeves_agent_duration_seconds",
            Help: "Agent execution duration in seconds",
            Buckets: []float64{0.01, 0.05, 0.1, 0.5, 1, 2, 5},
        },
        []string{"agent"},
    )

    llmCalls = promauto.NewCounterVec(
        prometheus.CounterOpts{
            Name: "jeeves_llm_calls_total",
            Help: "Total number of LLM API calls",
        },
        []string{"provider", "model", "status"},
    )

    llmTokensUsed = promauto.NewCounterVec(
        prometheus.CounterOpts{
            Name: "jeeves_llm_tokens_total",
            Help: "Total LLM tokens used",
        },
        []string{"provider", "model", "type"}, // type: input, output
    )

    llmLatency = promauto.NewHistogramVec(
        prometheus.HistogramOpts{
            Name: "jeeves_llm_duration_seconds",
            Help: "LLM call duration in seconds",
            Buckets: []float64{0.1, 0.5, 1, 2, 5, 10, 30},
        },
        []string{"provider", "model"},
    )

    dbConnections = promauto.NewGauge(
        prometheus.GaugeOpts{
            Name: "jeeves_db_connections",
            Help: "Current number of database connections",
        },
    )

    dbQueryLatency = promauto.NewHistogramVec(
        prometheus.HistogramOpts{
            Name: "jeeves_db_query_duration_seconds",
            Help: "Database query duration in seconds",
            Buckets: []float64{0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1},
        },
        []string{"operation"}, // operation: select, insert, update, delete
    )

    interruptsRaised = promauto.NewCounterVec(
        prometheus.CounterOpts{
            Name: "jeeves_interrupts_total",
            Help: "Total number of interrupts raised",
        },
        []string{"kind", "status"}, // kind: clarification, confirmation, etc.
    )
)
```

**Python (avionics):**
```python
from prometheus_client import Counter, Histogram, Gauge, Info

# HTTP Gateway Metrics
http_requests_total = Counter(
    'jeeves_http_requests_total',
    'Total HTTP requests',
    ['method', 'endpoint', 'status']
)

http_request_duration = Histogram(
    'jeeves_http_request_duration_seconds',
    'HTTP request duration',
    ['method', 'endpoint']
)

# LLM Provider Metrics (Python side)
llm_provider_calls = Counter(
    'jeeves_llm_provider_calls_total',
    'LLM provider calls from Python',
    ['provider', 'model', 'status']
)

llm_provider_latency = Histogram(
    'jeeves_llm_provider_duration_seconds',
    'LLM provider call duration',
    ['provider', 'model'],
    buckets=[0.1, 0.5, 1, 2, 5, 10, 30]
)

# Database Connection Pool Metrics
db_pool_size = Gauge(
    'jeeves_db_pool_size',
    'Database connection pool size'
)

db_pool_active = Gauge(
    'jeeves_db_pool_active_connections',
    'Active database connections'
)

db_pool_idle = Gauge(
    'jeeves_db_pool_idle_connections',
    'Idle database connections'
)
```

### 1.2 Instrumentation Points

**Runtime Package (`coreengine/runtime/runtime.go`):**
```go
func (r *PipelineRunner) Execute(...) (*Envelope, <-chan StageOutput, error) {
    start := time.Now()
    defer func() {
        duration := time.Since(start).Seconds()
        status := "success"
        if result.TerminationReason != nil {
            status = string(*result.TerminationReason)
        }
        pipelineExecutions.WithLabelValues(r.Config.Name, status).Inc()
        pipelineLatency.WithLabelValues(r.Config.Name).Observe(duration)
    }()
    // ... existing code
}
```

**Agent Package (`coreengine/agents/agent.go`):**
```go
func (a *Agent) Process(...) (*Envelope, error) {
    start := time.Now()
    defer func() {
        duration := time.Since(start).Seconds()
        status := "success"
        if err != nil {
            status = "error"
        }
        agentExecutions.WithLabelValues(a.Config.Name, status).Inc()
        agentLatency.WithLabelValues(a.Config.Name).Observe(duration)
    }()
    // ... existing code
}
```

**LLM Provider (`avionics/llm/`):**
```python
async def generate(self, model: str, prompt: str, options: dict) -> str:
    start = time.time()
    status = "success"
    try:
        result = await self._generate_impl(model, prompt, options)
        return result
    except Exception as e:
        status = "error"
        raise
    finally:
        duration = time.time() - start
        llm_provider_calls.labels(
            provider=self.provider_name,
            model=model,
            status=status
        ).inc()
        llm_provider_latency.labels(
            provider=self.provider_name,
            model=model
        ).observe(duration)
```

### 1.3 Metrics Endpoints

**Add to gRPC server (`coreengine/grpc/server.go`):**
```go
import "github.com/prometheus/client_golang/prometheus/promhttp"

func StartMetricsServer(port int) {
    http.Handle("/metrics", promhttp.Handler())
    http.ListenAndServe(fmt.Sprintf(":%d", port), nil)
}
```

**Add to FastAPI gateway (`avionics/gateway/main.py`):**
```python
from prometheus_client import make_asgi_app

# Mount Prometheus metrics
metrics_app = make_asgi_app()
app.mount("/metrics", metrics_app)
```

---

## Phase 2: Distributed Tracing (Weeks 3-4)

### 2.1 OpenTelemetry Integration

**Install dependencies:**
```bash
# Go
go get go.opentelemetry.io/otel
go get go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc
go get go.opentelemetry.io/otel/sdk/trace

# Python
pip install opentelemetry-api opentelemetry-sdk
pip install opentelemetry-instrumentation-fastapi
pip install opentelemetry-instrumentation-sqlalchemy
pip install opentelemetry-exporter-otlp
```

**Initialize tracer (`coreengine/observability/tracer.go`):**
```go
package observability

import (
    "context"
    "go.opentelemetry.io/otel"
    "go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc"
    "go.opentelemetry.io/otel/sdk/trace"
    sdktrace "go.opentelemetry.io/otel/sdk/trace"
)

func InitTracer(serviceName string, endpoint string) (func(), error) {
    ctx := context.Background()

    exporter, err := otlptracegrpc.New(ctx,
        otlptracegrpc.WithEndpoint(endpoint),
        otlptracegrpc.WithInsecure(),
    )
    if err != nil {
        return nil, err
    }

    tp := sdktrace.NewTracerProvider(
        sdktrace.WithBatcher(exporter),
        sdktrace.WithResource(newResource(serviceName)),
    )

    otel.SetTracerProvider(tp)

    return func() {
        _ = tp.Shutdown(ctx)
    }, nil
}
```

**Trace pipeline execution:**
```go
import (
    "go.opentelemetry.io/otel"
    "go.opentelemetry.io/otel/attribute"
)

var tracer = otel.Tracer("jeeves-core")

func (r *PipelineRunner) Execute(...) (*Envelope, <-chan StageOutput, error) {
    ctx, span := tracer.Start(ctx, "pipeline.execute",
        trace.WithAttributes(
            attribute.String("pipeline.name", r.Config.Name),
            attribute.String("request.id", env.RequestID),
            attribute.String("user.id", env.UserID),
        ),
    )
    defer span.End()

    // Add events as execution progresses
    span.AddEvent("pipeline.started")
    // ... execution
    span.AddEvent("pipeline.completed")

    return result, outputChan, err
}
```

**Python tracing setup (`avionics/observability/tracing.py`):**
```python
from opentelemetry import trace
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
from opentelemetry.instrumentation.sqlalchemy import SQLAlchemyInstrumentor

def init_tracing(service_name: str, otlp_endpoint: str):
    provider = TracerProvider(
        resource=Resource.create({"service.name": service_name})
    )

    otlp_exporter = OTLPSpanExporter(endpoint=otlp_endpoint, insecure=True)
    provider.add_span_processor(BatchSpanProcessor(otlp_exporter))

    trace.set_tracer_provider(provider)

    # Auto-instrument FastAPI
    FastAPIInstrumentor.instrument_app(app)

    # Auto-instrument SQLAlchemy
    SQLAlchemyInstrumentor().instrument()
```

### 2.2 Cross-Service Trace Propagation

**gRPC interceptor for trace context:**
```go
func TracingUnaryInterceptor(tracer trace.Tracer) grpc.UnaryServerInterceptor {
    return func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
        ctx, span := tracer.Start(ctx, info.FullMethod)
        defer span.End()

        resp, err := handler(ctx, req)
        if err != nil {
            span.RecordError(err)
        }
        return resp, err
    }
}
```

**HTTP middleware for trace propagation:**
```python
from opentelemetry.propagate import inject, extract

@app.middleware("http")
async def trace_middleware(request: Request, call_next):
    # Extract trace context from headers
    ctx = extract(request.headers)

    with tracer.start_as_current_span("http.request", context=ctx) as span:
        span.set_attribute("http.method", request.method)
        span.set_attribute("http.url", str(request.url))

        response = await call_next(request)

        span.set_attribute("http.status_code", response.status_code)
        return response
```

---

## Phase 3: Alerting & SLOs (Week 5)

### 3.1 Define Service Level Objectives

**SLO Definitions:**
```yaml
# jeeves-slo.yaml
slos:
  - name: pipeline_success_rate
    objective: 99.5%
    window: 30d
    description: "Percentage of successful pipeline executions"
    error_budget: 0.5%

  - name: pipeline_latency_p99
    objective: 5s
    window: 30d
    description: "99th percentile pipeline execution time"

  - name: llm_availability
    objective: 99.0%
    window: 30d
    description: "LLM provider API availability"

  - name: database_availability
    objective: 99.9%
    window: 30d
    description: "PostgreSQL database availability"
```

### 3.2 Prometheus Alert Rules

**Create `alerts.yml`:**
```yaml
groups:
  - name: jeeves_alerts
    interval: 30s
    rules:
      # Error Rate Alerts
      - alert: HighErrorRate
        expr: |
          sum(rate(jeeves_pipeline_executions_total{status!="success"}[5m]))
          /
          sum(rate(jeeves_pipeline_executions_total[5m]))
          > 0.01
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High error rate detected"
          description: "Error rate is {{ $value | humanizePercentage }} over last 5 minutes"

      # Latency Alerts
      - alert: HighLatency
        expr: |
          histogram_quantile(0.99,
            sum(rate(jeeves_pipeline_duration_seconds_bucket[5m])) by (le, pipeline)
          ) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High pipeline latency detected"
          description: "P99 latency is {{ $value }}s for pipeline {{ $labels.pipeline }}"

      # LLM Provider Alerts
      - alert: LLMProviderDown
        expr: |
          sum(rate(jeeves_llm_calls_total{status="error"}[5m]))
          /
          sum(rate(jeeves_llm_calls_total[5m]))
          > 0.1
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "LLM provider {{ $labels.provider }} is experiencing errors"
          description: "Error rate is {{ $value | humanizePercentage }}"

      # Database Alerts
      - alert: DatabasePoolExhaustion
        expr: jeeves_db_pool_active_connections / jeeves_db_pool_size > 0.8
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Database connection pool is nearly exhausted"
          description: "Pool utilization is {{ $value | humanizePercentage }}"

      # Interrupt Alerts
      - alert: HighInterruptRate
        expr: |
          sum(rate(jeeves_interrupts_total[5m])) > 10
        for: 5m
        labels:
          severity: info
        annotations:
          summary: "High interrupt rate detected"
          description: "{{ $value }} interrupts per second"
```

### 3.3 Alertmanager Configuration

**Create `alertmanager.yml`:**
```yaml
global:
  resolve_timeout: 5m

route:
  group_by: ['alertname', 'severity']
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: 'default'
  routes:
    - match:
        severity: critical
      receiver: 'pagerduty'
      continue: true
    - match:
        severity: warning
      receiver: 'slack'

receivers:
  - name: 'default'
    webhook_configs:
      - url: 'http://localhost:5001/webhook'

  - name: 'slack'
    slack_configs:
      - api_url: '{{ SLACK_WEBHOOK_URL }}'
        channel: '#jeeves-alerts'
        title: '{{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.description }}{{ end }}'

  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: '{{ PAGERDUTY_SERVICE_KEY }}'
        description: '{{ .CommonAnnotations.summary }}'
```

---

## Phase 4: Dashboards (Week 6)

### 4.1 Grafana Dashboard - Service Overview

**Dashboard JSON template:**
```json
{
  "dashboard": {
    "title": "Jeeves Core - Service Overview",
    "panels": [
      {
        "title": "Request Rate",
        "targets": [{
          "expr": "sum(rate(jeeves_pipeline_executions_total[5m]))"
        }],
        "type": "graph"
      },
      {
        "title": "Error Rate",
        "targets": [{
          "expr": "sum(rate(jeeves_pipeline_executions_total{status!=\"success\"}[5m])) / sum(rate(jeeves_pipeline_executions_total[5m]))"
        }],
        "type": "graph"
      },
      {
        "title": "Latency Percentiles",
        "targets": [
          {
            "expr": "histogram_quantile(0.50, sum(rate(jeeves_pipeline_duration_seconds_bucket[5m])) by (le))",
            "legendFormat": "p50"
          },
          {
            "expr": "histogram_quantile(0.95, sum(rate(jeeves_pipeline_duration_seconds_bucket[5m])) by (le))",
            "legendFormat": "p95"
          },
          {
            "expr": "histogram_quantile(0.99, sum(rate(jeeves_pipeline_duration_seconds_bucket[5m])) by (le))",
            "legendFormat": "p99"
          }
        ],
        "type": "graph"
      },
      {
        "title": "LLM Token Usage",
        "targets": [{
          "expr": "sum(rate(jeeves_llm_tokens_total[5m])) by (provider, type)"
        }],
        "type": "graph"
      }
    ]
  }
}
```

### 4.2 Key Dashboards to Create

1. **Service Overview**
   - Request rate, error rate, latency
   - Top errors
   - Service health status

2. **Pipeline Performance**
   - Per-pipeline success rate
   - Per-pipeline latency
   - Agent execution times
   - Routing decisions

3. **LLM Provider Health**
   - API call rate by provider
   - Token usage by model
   - Provider latency
   - Provider errors

4. **Infrastructure**
   - Database connection pool utilization
   - gRPC connection status
   - Memory and CPU usage
   - Event queue depth

5. **Business Metrics**
   - Interrupts by type
   - User sessions
   - Feature usage
   - Pipeline outcomes

---

## Phase 5: Implementation Plan

### Week 1: Metrics Foundation
- [ ] Add Prometheus client libraries to Go and Python
- [ ] Define core metrics (pipelines, agents, LLM calls)
- [ ] Instrument runtime.Execute() and agent.Process()
- [ ] Add metrics endpoints (/metrics)
- [ ] Deploy Prometheus server in staging

### Week 2: Comprehensive Metrics
- [ ] Add database connection pool metrics
- [ ] Add HTTP gateway metrics
- [ ] Add interrupt metrics
- [ ] Add custom business metrics
- [ ] Create initial Grafana dashboards

### Week 3: Tracing Setup
- [ ] Add OpenTelemetry dependencies
- [ ] Initialize tracers in Go and Python
- [ ] Add gRPC trace propagation interceptor
- [ ] Add HTTP trace propagation middleware
- [ ] Deploy Jaeger in staging

### Week 4: Tracing Instrumentation
- [ ] Trace pipeline execution end-to-end
- [ ] Trace agent execution
- [ ] Trace LLM calls with token counts
- [ ] Trace database queries
- [ ] Verify traces in Jaeger UI

### Week 5: Alerting
- [ ] Define SLOs for key metrics
- [ ] Create Prometheus alert rules
- [ ] Deploy Alertmanager
- [ ] Configure Slack/PagerDuty integrations
- [ ] Test alert firing and resolution

### Week 6: Dashboards & Documentation
- [ ] Create 5 core Grafana dashboards
- [ ] Document metrics and their meanings
- [ ] Create runbooks for common alerts
- [ ] Train team on observability tools
- [ ] Deploy to production

---

## Infrastructure Requirements

### New Services
1. **Prometheus** (metrics storage)
   - Container: `prom/prometheus:latest`
   - Port: 9090
   - Storage: 50GB persistent volume
   - Retention: 30 days

2. **Grafana** (visualization)
   - Container: `grafana/grafana:latest`
   - Port: 3000
   - Storage: 10GB for dashboards

3. **Jaeger** (tracing)
   - Container: `jaegertracing/all-in-one:latest`
   - Ports: 16686 (UI), 14268 (collector)
   - Storage: Elasticsearch or in-memory

4. **Alertmanager** (alerting)
   - Container: `prom/alertmanager:latest`
   - Port: 9093

### docker-compose.yml Updates
```yaml
services:
  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
      - ./alerts.yml:/etc/prometheus/alerts.yml
      - prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.retention.time=30d'

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    volumes:
      - grafana_data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_ADMIN_PASSWORD}

  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # UI
      - "14268:14268"  # Collector
      - "4317:4317"    # OTLP gRPC
    environment:
      - COLLECTOR_OTLP_ENABLED=true

  alertmanager:
    image: prom/alertmanager:latest
    ports:
      - "9093:9093"
    volumes:
      - ./alertmanager.yml:/etc/alertmanager/alertmanager.yml
      - alertmanager_data:/alertmanager
```

---

## Success Metrics

### After Phase 1 (Metrics)
- [ ] All critical code paths instrumented
- [ ] Metrics endpoint available at /metrics
- [ ] Prometheus scraping metrics successfully
- [ ] Basic Grafana dashboard shows live data

### After Phase 2 (Tracing)
- [ ] End-to-end traces visible in Jaeger
- [ ] Request correlation works across Go/Python boundary
- [ ] Average 95% trace completion rate
- [ ] <100ms tracing overhead

### After Phase 3 (Alerting)
- [ ] All critical alerts defined
- [ ] Alerts fire correctly in staging
- [ ] Alert routing to Slack/PagerDuty works
- [ ] <5 min time to detect critical issues (MTTD)

### After Phase 4 (Dashboards)
- [ ] 5 core dashboards deployed
- [ ] Team trained on dashboard usage
- [ ] Dashboards used in incident response
- [ ] <15 min time to resolve issues (MTTR)

---

## Risks & Mitigation

### Performance Overhead
**Risk:** Metrics/tracing adds latency
**Mitigation:**
- Use sampling for tracing (10% sample rate)
- Async metrics export
- Buffer metrics in memory
- Monitor overhead with benchmarks

### Operational Complexity
**Risk:** More moving parts to maintain
**Mitigation:**
- Use managed services where possible (Grafana Cloud, Datadog)
- Comprehensive documentation
- Automated deployment via docker-compose
- Team training sessions

### Alert Fatigue
**Risk:** Too many alerts desensitize team
**Mitigation:**
- Start with high-threshold alerts only
- Iterate based on real incidents
- Group related alerts
- Regular alert rule reviews

---

## Cost Estimate

### Self-Hosted (Recommended for POC)
- **Infrastructure:** ~$100/month (Prometheus, Grafana, Jaeger on cloud VMs)
- **Engineering Time:** 4-6 weeks (1 engineer)
- **Total:** ~$15,000 one-time + $100/month

### Managed Services (Production)
- **Grafana Cloud:** ~$200/month (10K metrics)
- **Datadog:** ~$500/month (includes APM)
- **Engineering Time:** 2-3 weeks (reduced due to managed services)
- **Total:** ~$10,000 one-time + $500/month

---

## Next Steps

1. **Review & Approval** (Week 0)
   - Get stakeholder buy-in
   - Choose self-hosted vs managed
   - Allocate engineering resources

2. **Kickoff** (Week 1, Day 1)
   - Set up Prometheus/Grafana locally
   - Add first metrics to runtime package
   - Verify metrics collection works

3. **Weekly Reviews** (Weeks 1-6)
   - Demo new capabilities each week
   - Adjust plan based on learnings
   - Track progress against success metrics

4. **Production Rollout** (Week 7)
   - Deploy observability stack to production
   - Monitor for 1 week with enhanced logging
   - Declare success or iterate

---

**Status:** Ready for implementation pending approval
**Owner:** TBD
**Contact:** [Your contact info]
