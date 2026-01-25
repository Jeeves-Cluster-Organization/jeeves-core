# Observability Implementation Analysis

**Date:** 2026-01-23
**Session:** claude/implement-observability-Wxng5
**Status:** Analysis Complete - Ready for Implementation
**Based on:** session_01EbbHeGiAChYZ7hLLUH7Qmd test fixes + OBSERVABILITY_IMPROVEMENTS.md Phase 1

---

## Executive Summary

This document analyzes the jeeves-core codebase to determine the optimal implementation patterns for Prometheus metrics instrumentation (Phase 1 of observability improvements). After thorough code exploration, we've identified **excellent instrumentation points** and **existing patterns** that make metrics integration straightforward.

**Key Finding:** The codebase already has 80% of the infrastructure needed - timing calculations, event logging, clean boundaries, and Python Prometheus support. We just need to:
1. Add Prometheus client to Go
2. Insert metrics calls at existing timing points
3. Add metrics interceptor to gRPC chain
4. Expose /metrics endpoints

---

## 1. Current State Assessment

### ✅ What Already Exists (Strong Foundation)

#### Python Side
- **Prometheus Client Library**: Already integrated in `avionics/observability/metrics.py`
  - Graceful fallback if not installed (lines 5-37)
  - Production-ready patterns: Counter, Gauge, Histogram
  - Existing metrics: orchestrator, meta-validation, retry tracking
  - Clean API: `orchestrator_started()`, `orchestrator_completed(outcome, duration)`

- **OpenTelemetry**: Dependencies already in `avionics/pyproject.toml`
  - `opentelemetry-api>=1.22.0`
  - `opentelemetry-sdk>=1.22.0`
  - Ready for Phase 2 (distributed tracing)

- **Timing Infrastructure**: LLM Gateway (`llm/gateway.py`)
  - Line 228: `start_time = time.time()`
  - Line 356: `latency_seconds = time.time() - start_time` (already calculated!)
  - Lines 359-361: Token counts already tracked
  - Lines 365-370: Cost calculation already implemented
  - **Perfect metrics insertion point** - just add Prometheus calls

#### Go Side
- **Timing Patterns**: Both critical paths already measure duration
  - `runtime.Execute()` (runtime.go:152, 193): `startTime`, `durationMS`
  - `agent.Process()` (agents/agent.go:104, 116): `startTime`, `durationMS`
  - Defer pattern for guaranteed metric recording (line 115-127)

- **Interceptor Architecture**: Clean, extensible design
  - `grpc/interceptors.go` has logging + recovery interceptors
  - Chain pattern for composable interceptors (lines 177-197)
  - Already tracks timing: `duration := time.Since(start)` (line 39)
  - **Perfect place for metrics interceptor**

- **Structured Logging**: Logger interface everywhere
  - Consistent event names: `pipeline_started`, `pipeline_completed`
  - Key-value pairs: `duration_ms`, `envelope_id`, `request_id`
  - Easy to parallel with metrics

### ⚠️ What's Missing (Our Work)

#### Go Side
- No Prometheus client library in `go.mod`
- No metrics package/registry
- No metrics calls at instrumentation points
- No metrics endpoint handler

#### Python Side
- FastAPI gateway (`avionics/gateway/main.py`) doesn't expose `/metrics`
- LLM provider metrics not recorded (gateway has timing, but no Prometheus calls)
- No HTTP request middleware for metrics

#### Infrastructure
- No Prometheus server in `docker/docker-compose.yml`
- No `prometheus.yml` configuration
- No Grafana for visualization

---

## 2. Architecture Analysis

### Service Boundaries

```
┌─────────────────────────────────────────────────────────────┐
│                     External Clients                         │
│                  (HTTP/WebSocket requests)                   │
└────────────────────────────┬────────────────────────────────┘
                             │
                   ┌─────────▼──────────┐
                   │   FastAPI Gateway   │ ← Python metrics here
                   │  (avionics)  │   (HTTP middleware)
                   │   Port 8000         │
                   └─────────┬───────────┘
                             │ gRPC
                   ┌─────────▼───────────┐
                   │  gRPC Server        │ ← Go metrics here
                   │  (coreengine)       │   (interceptors)
                   │  Port 50051         │
                   └─────────┬───────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
  ┌─────▼──────┐   ┌────────▼────────┐   ┌──────▼──────┐
  │  Pipeline  │   │  Agent.Process  │   │  LLM Bridge │
  │  Execute   │   │  (Go→Python)    │   │  (gRPC)     │
  └────────────┘   └─────────────────┘   └──────┬──────┘
                                                 │ gRPC
                                         ┌───────▼────────┐
                                         │  LLM Gateway   │ ← Python metrics
                                         │ (Python side)  │   (already timed)
                                         └───────┬────────┘
                                                 │
                                    ┌────────────┴────────────┐
                                    │                         │
                              ┌─────▼──────┐          ┌──────▼──────┐
                              │ LlamaServer│          │  OpenAI/    │
                              │  Provider  │          │  Anthropic  │
                              └────────────┘          └─────────────┘
```

### Key Metrics Collection Points

| Component | File | Line | Current State | Metrics to Add |
|-----------|------|------|---------------|----------------|
| **Pipeline Execution** | `runtime/runtime.go` | 134-207 | ✅ Timing tracked | Counter, Histogram |
| **Agent Execution** | `agents/agent.go` | 103-159 | ✅ Timing tracked | Counter, Histogram |
| **LLM Calls (Go side)** | `agents/agent.go` | 198 | ✅ Call counted | Counter (via LLM interface) |
| **LLM Gateway (Python)** | `llm/gateway.py` | 201-310 | ✅ Timing/tokens tracked | Counter, Histogram |
| **gRPC Requests** | `grpc/interceptors.go` | 22-59 | ✅ Timing in logs | Counter, Histogram |
| **HTTP Requests** | `gateway/main.py` | 130-263 | ❌ No metrics | Counter, Histogram |

---

## 3. Implementation Strategy

### Design Principles

1. **Non-Invasive**: Add metrics without disrupting existing code flow
2. **Fail-Safe**: Metrics errors should never break application logic
3. **Low Overhead**: Use `promauto` (Go) for zero-allocation registration
4. **Consistent Labels**: Same label scheme across Go/Python
5. **Reuse Timing**: Don't add new time.Now() calls - use existing timing
6. **Follow Existing Patterns**: Mirror Python's metrics.py structure in Go

### Label Schema (Consistent Across Services)

```yaml
# Pipeline metrics
jeeves_pipeline_executions_total:
  labels: [pipeline, status]  # status: success, error, terminated

jeeves_pipeline_duration_seconds:
  labels: [pipeline]

# Agent metrics
jeeves_agent_executions_total:
  labels: [agent, status]  # status: success, error

jeeves_agent_duration_seconds:
  labels: [agent]

# LLM metrics (Go and Python should match)
jeeves_llm_calls_total:
  labels: [provider, model, status]  # provider: llamaserver, openai, anthropic

jeeves_llm_duration_seconds:
  labels: [provider, model]

jeeves_llm_tokens_total:
  labels: [provider, model, type]  # type: prompt, completion

# gRPC metrics
jeeves_grpc_requests_total:
  labels: [method, status]  # method: /engine.v1.EngineService/ExecutePipeline

jeeves_grpc_request_duration_seconds:
  labels: [method]

# HTTP metrics
jeeves_http_requests_total:
  labels: [method, path, status_code]

jeeves_http_request_duration_seconds:
  labels: [method, path]
```

---

## 4. Detailed Implementation Plan

### Phase 1A: Go Infrastructure (Day 1)

#### Step 1: Add Prometheus Dependency

**File:** `go.mod`

```bash
cd /home/user/jeeves-core
go get github.com/prometheus/client_golang@latest
```

This adds:
- `github.com/prometheus/client_golang/prometheus` - Core library
- `github.com/prometheus/client_golang/prometheus/promauto` - Auto-registration
- `github.com/prometheus/client_golang/prometheus/promhttp` - HTTP handler

#### Step 2: Create Metrics Package

**File:** `coreengine/observability/metrics.go` (NEW)

```go
package observability

import (
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
)

// Pipeline metrics
var (
	pipelineExecutionsTotal = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "jeeves_pipeline_executions_total",
			Help: "Total number of pipeline executions",
		},
		[]string{"pipeline", "status"},
	)

	pipelineDurationSeconds = promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "jeeves_pipeline_duration_seconds",
			Help:    "Pipeline execution duration in seconds",
			Buckets: []float64{0.1, 0.5, 1, 2, 5, 10, 30, 60},
		},
		[]string{"pipeline"},
	)
)

// Agent metrics
var (
	agentExecutionsTotal = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "jeeves_agent_executions_total",
			Help: "Total number of agent executions",
		},
		[]string{"agent", "status"},
	)

	agentDurationSeconds = promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "jeeves_agent_duration_seconds",
			Help:    "Agent execution duration in seconds",
			Buckets: []float64{0.01, 0.05, 0.1, 0.5, 1, 2, 5},
		},
		[]string{"agent"},
	)
)

// LLM metrics
var (
	llmCallsTotal = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "jeeves_llm_calls_total",
			Help: "Total number of LLM API calls",
		},
		[]string{"provider", "model", "status"},
	)

	llmDurationSeconds = promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "jeeves_llm_duration_seconds",
			Help:    "LLM call duration in seconds",
			Buckets: []float64{0.1, 0.5, 1, 2, 5, 10, 30},
		},
		[]string{"provider", "model"},
	)
)

// gRPC metrics
var (
	grpcRequestsTotal = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "jeeves_grpc_requests_total",
			Help: "Total gRPC requests",
		},
		[]string{"method", "status"},
	)

	grpcRequestDurationSeconds = promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "jeeves_grpc_request_duration_seconds",
			Help:    "gRPC request duration in seconds",
			Buckets: []float64{0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1, 2, 5},
		},
		[]string{"method"},
	)
)

// ============================================================
// Public API (mirrors Python's metrics.py)
// ============================================================

// RecordPipelineExecution records pipeline execution metrics
func RecordPipelineExecution(pipeline string, status string, durationMS int) {
	pipelineExecutionsTotal.WithLabelValues(pipeline, status).Inc()
	pipelineDurationSeconds.WithLabelValues(pipeline).Observe(float64(durationMS) / 1000.0)
}

// RecordAgentExecution records agent execution metrics
func RecordAgentExecution(agent string, status string, durationMS int) {
	agentExecutionsTotal.WithLabelValues(agent, status).Inc()
	agentDurationSeconds.WithLabelValues(agent).Observe(float64(durationMS) / 1000.0)
}

// RecordLLMCall records LLM call metrics
func RecordLLMCall(provider string, model string, status string, durationMS int) {
	llmCallsTotal.WithLabelValues(provider, model, status).Inc()
	llmDurationSeconds.WithLabelValues(provider, model).Observe(float64(durationMS) / 1000.0)
}

// RecordGRPCRequest records gRPC request metrics
func RecordGRPCRequest(method string, status string, durationMS int) {
	grpcRequestsTotal.WithLabelValues(method, status).Inc()
	grpcRequestDurationSeconds.WithLabelValues(method).Observe(float64(durationMS) / 1000.0)
}
```

**Why this design:**
- ✅ Uses `promauto` for automatic registration (no manual Register() calls)
- ✅ Package-level vars prevent re-registration
- ✅ Clean API functions hide Prometheus details
- ✅ Mirrors Python's `metrics.py` structure
- ✅ Duration conversion (ms → seconds) encapsulated

#### Step 3: Instrument Pipeline Execution

**File:** `coreengine/runtime/runtime.go`

**Current code (lines 193-206):**
```go
	// Log completion
	durationMS := int(time.Since(startTime).Milliseconds())
	completeEvent := "pipeline_completed"
	if opts.Mode == RunModeParallel {
		completeEvent = "pipeline_parallel_completed"
	}

	r.Logger.Info(completeEvent,
		"envelope_id", resultEnv.EnvelopeID,
		"request_id", resultEnv.RequestID,
		"final_stage", resultEnv.CurrentStage,
		"terminated", resultEnv.Terminated,
		"duration_ms", durationMS,
	)
```

**Add after line 193:**
```go
	import "github.com/jeeves-cluster-organization/codeanalysis/coreengine/observability"

	// Log completion
	durationMS := int(time.Since(startTime).Milliseconds())

	// Record metrics
	status := "success"
	if err != nil {
		status = "error"
	} else if resultEnv.Terminated {
		status = "terminated"
	}
	observability.RecordPipelineExecution(r.Config.Name, status, durationMS)

	completeEvent := "pipeline_completed"
	// ... rest of logging code
```

**Impact:** 3 lines added, no changes to existing flow

#### Step 4: Instrument Agent Execution

**File:** `coreengine/agents/agent.go`

**Current defer block (lines 115-127):**
```go
	defer func() {
		durationMS := int(time.Since(startTime).Milliseconds())
		if err != nil {
			a.Logger.Error(fmt.Sprintf("%s_error", a.Name), "error", err.Error(), "duration_ms", durationMS)
			errStr := err.Error()
			env.RecordAgentComplete(a.Name, "error", &errStr, llmCalls, durationMS)
			a.emitCompleted("error", durationMS, err)
		} else {
			a.Logger.Info(fmt.Sprintf("%s_completed", a.Name), "duration_ms", durationMS, "next_stage", env.CurrentStage)
			env.RecordAgentComplete(a.Name, "success", nil, llmCalls, durationMS)
			a.emitCompleted("success", durationMS, nil)
		}
	}()
```

**Modified defer block:**
```go
	import "github.com/jeeves-cluster-organization/codeanalysis/coreengine/observability"

	defer func() {
		durationMS := int(time.Since(startTime).Milliseconds())
		if err != nil {
			observability.RecordAgentExecution(a.Name, "error", durationMS)
			a.Logger.Error(fmt.Sprintf("%s_error", a.Name), "error", err.Error(), "duration_ms", durationMS)
			errStr := err.Error()
			env.RecordAgentComplete(a.Name, "error", &errStr, llmCalls, durationMS)
			a.emitCompleted("error", durationMS, err)
		} else {
			observability.RecordAgentExecution(a.Name, "success", durationMS)
			a.Logger.Info(fmt.Sprintf("%s_completed", a.Name), "duration_ms", durationMS, "next_stage", env.CurrentStage)
			env.RecordAgentComplete(a.Name, "success", nil, llmCalls, durationMS)
			a.emitCompleted("success", durationMS, nil)
		}
	}()
```

**Impact:** 2 lines added in existing defer block

#### Step 5: Add gRPC Metrics Interceptor

**File:** `coreengine/grpc/interceptors.go`

**Add new interceptor (after RecoveryInterceptor):**
```go
import "github.com/jeeves-cluster-organization/codeanalysis/coreengine/observability"

// MetricsInterceptor creates a unary server interceptor that records Prometheus metrics.
func MetricsInterceptor() grpc.UnaryServerInterceptor {
	return func(
		ctx context.Context,
		req interface{},
		info *grpc.UnaryServerInfo,
		handler grpc.UnaryHandler,
	) (interface{}, error) {
		start := time.Now()

		// Call the handler
		resp, err := handler(ctx, req)

		// Record metrics
		durationMS := int(time.Since(start).Milliseconds())
		status := "success"
		if err != nil {
			st, _ := status.FromError(err)
			status = st.Code().String()
		}
		observability.RecordGRPCRequest(info.FullMethod, status, durationMS)

		return resp, err
	}
}

// StreamMetricsInterceptor creates a stream server interceptor for metrics.
func StreamMetricsInterceptor() grpc.StreamServerInterceptor {
	return func(
		srv interface{},
		ss grpc.ServerStream,
		info *grpc.StreamServerInfo,
		handler grpc.StreamHandler,
	) error {
		start := time.Now()

		// Call the handler
		err := handler(srv, ss)

		// Record metrics
		durationMS := int(time.Since(start).Milliseconds())
		status := "success"
		if err != nil {
			st, _ := status.FromError(err)
			status = st.Code().String()
		}
		observability.RecordGRPCRequest(info.FullMethod, status, durationMS)

		return err
	}
}
```

**Update ServerOptions (line 226):**
```go
func ServerOptions(logger Logger) []grpc.ServerOption {
	// Create interceptor chains
	unaryInterceptor := ChainUnaryInterceptors(
		RecoveryInterceptor(logger, nil),
		MetricsInterceptor(),        // ADD THIS
		LoggingInterceptor(logger),
	)

	streamInterceptor := ChainStreamInterceptors(
		StreamRecoveryInterceptor(logger, nil),
		StreamMetricsInterceptor(),  // ADD THIS
		StreamLoggingInterceptor(logger),
	)

	return []grpc.ServerOption{
		grpc.UnaryInterceptor(unaryInterceptor),
		grpc.StreamInterceptor(streamInterceptor),
	}
}
```

#### Step 6: Add /metrics Endpoint to gRPC Server

**File:** `coreengine/grpc/server.go`

**Add import:**
```go
import "github.com/prometheus/client_golang/prometheus/promhttp"
```

**Add method to EngineServer:**
```go
// StartMetricsServer starts an HTTP server on the given port to expose Prometheus metrics.
// This should be called in a separate goroutine alongside the gRPC server.
func (s *EngineServer) StartMetricsServer(port int) error {
	mux := http.NewServeMux()
	mux.Handle("/metrics", promhttp.Handler())

	addr := fmt.Sprintf(":%d", port)
	s.logger.Info("metrics_server_started", "port", port, "path", "/metrics")

	return http.ListenAndServe(addr, mux)
}
```

**Update main.go (orchestrator startup):**
```go
// Start metrics server in background
go func() {
	if err := server.StartMetricsServer(9090); err != nil {
		logger.Error("metrics_server_failed", "error", err)
	}
}()

// Start gRPC server
if err := server.Start(":50051"); err != nil {
	logger.Error("server_failed", "error", err)
	os.Exit(1)
}
```

---

### Phase 1B: Python Instrumentation (Day 2)

#### Step 1: Add LLM Gateway Metrics

**File:** `avionics/llm/gateway.py`

**Current code (lines 345-370):**
```python
async def _call_provider(...):
    # ... setup code ...

    start_time = time.time()
    response = await provider.generate(...)
    latency_seconds = time.time() - start_time

    # Token tracking
    prompt_tokens = count_tokens(combined_prompt, model)
    completion_tokens = count_tokens(response, model)

    # Cost calculation
    cost = self.cost_calculator.calculate_cost(...)
```

**Add metrics (import at top):**
```python
from avionics.observability.metrics import (
    llm_provider_calls,
    llm_provider_latency,
    llm_tokens_used
)
```

**Add after line 370:**
```python
    # Record metrics
    status = "success"
    llm_provider_calls.labels(
        provider=provider_name,
        model=model,
        status=status
    ).inc()
    llm_provider_latency.labels(
        provider=provider_name,
        model=model
    ).observe(latency_seconds)

    # Token metrics
    if prompt_tokens:
        llm_tokens_used.labels(
            provider=provider_name,
            model=model,
            type="prompt"
        ).inc(prompt_tokens)
    if completion_tokens:
        llm_tokens_used.labels(
            provider=provider_name,
            model=model,
            type="completion"
        ).inc(completion_tokens)
```

**Handle errors (in except blocks):**
```python
except Exception as e:
    status = "error"
    llm_provider_calls.labels(
        provider=provider_name,
        model=model,
        status=status
    ).inc()
    raise
```

#### Step 2: Update metrics.py with Missing Metrics

**File:** `avionics/observability/metrics.py`

**Add after line 100:**
```python
# ============================================================
# LLM Provider Metrics
# ============================================================

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

llm_tokens_used = Counter(
    'jeeves_llm_tokens_total',
    'Total LLM tokens used',
    ['provider', 'model', 'type']  # type: prompt, completion
)

# ============================================================
# HTTP Gateway Metrics
# ============================================================

http_requests_total = Counter(
    'jeeves_http_requests_total',
    'Total HTTP requests',
    ['method', 'path', 'status_code']
)

http_request_duration = Histogram(
    'jeeves_http_request_duration_seconds',
    'HTTP request duration',
    ['method', 'path'],
    buckets=[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10]
)
```

#### Step 3: Add FastAPI Metrics Middleware

**File:** `avionics/gateway/main.py`

**Add import (after line 73):**
```python
from avionics.observability.metrics import http_requests_total, http_request_duration
import time
```

**Add middleware (after CORS middleware, ~line 150):**
```python
@app.middleware("http")
async def metrics_middleware(request: Request, call_next):
    """Record Prometheus metrics for all HTTP requests."""
    start_time = time.time()

    # Process request
    response = await call_next(request)

    # Record metrics
    duration = time.time() - start_time
    http_requests_total.labels(
        method=request.method,
        path=request.url.path,
        status_code=response.status_code
    ).inc()
    http_request_duration.labels(
        method=request.method,
        path=request.url.path
    ).observe(duration)

    return response
```

#### Step 4: Expose /metrics Endpoint

**File:** `avionics/gateway/main.py`

**Add import:**
```python
from prometheus_client import make_asgi_app
```

**Mount metrics app (after line 263):**
```python
# Mount Prometheus metrics endpoint
metrics_app = make_asgi_app()
app.mount("/metrics", metrics_app)
```

---

### Phase 1C: Infrastructure (Day 3)

#### Step 1: Add Prometheus to docker-compose.yml

**File:** `docker/docker-compose.yml`

**Add after postgres service:**
```yaml
  prometheus:
    image: prom/prometheus:latest
    container_name: jeeves-prometheus
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=30d'
      - '--web.enable-lifecycle'
    networks:
      - jeeves-network
    restart: unless-stopped

volumes:
  prometheus_data:
    driver: local
```

#### Step 2: Create prometheus.yml

**File:** `docker/prometheus.yml` (NEW)

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s
  external_labels:
    cluster: 'jeeves-core'
    environment: 'development'

scrape_configs:
  # Go gRPC server metrics
  - job_name: 'jeeves-grpc'
    static_configs:
      - targets: ['orchestrator:9090']
        labels:
          service: 'coreengine'
          component: 'grpc-server'

  # Python FastAPI gateway metrics
  - job_name: 'jeeves-http-gateway'
    static_configs:
      - targets: ['gateway:8000']
        labels:
          service: 'avionics'
          component: 'http-gateway'

  # Python orchestrator metrics (if separate)
  - job_name: 'jeeves-orchestrator'
    metrics_path: '/metrics'
    static_configs:
      - targets: ['orchestrator:8000']
        labels:
          service: 'avionics'
          component: 'orchestrator'

  # Prometheus self-monitoring
  - job_name: 'prometheus'
    static_configs:
      - targets: ['localhost:9090']
```

#### Step 3: Update Docker Services to Expose Metrics Ports

**File:** `docker/docker-compose.yml`

**Update orchestrator service:**
```yaml
  orchestrator:
    # ... existing config ...
    ports:
      - "8000:8000"   # HTTP gateway
      - "50051:50051" # gRPC
      - "9090:9090"   # Metrics (ADD THIS)
```

**Update gateway service:**
```yaml
  gateway:
    # ... existing config ...
    ports:
      - "8000:8000"   # HTTP + /metrics endpoint (already exposed)
```

---

## 5. Testing Strategy

### Unit Tests

#### Go Metrics Test
**File:** `coreengine/observability/metrics_test.go` (NEW)

```go
package observability_test

import (
	"testing"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/testutil"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/observability"
)

func TestRecordPipelineExecution(t *testing.T) {
	// Record a successful pipeline execution
	observability.RecordPipelineExecution("test-pipeline", "success", 1500)

	// Verify counter incremented
	// Verify histogram recorded
	// (Use prometheus/testutil for assertions)
}

func TestRecordAgentExecution(t *testing.T) {
	observability.RecordAgentExecution("test-agent", "success", 250)
	// Assertions...
}
```

#### Python Metrics Test
**File:** `avionics/observability/test_metrics.py` (NEW)

```python
import pytest
from avionics.observability.metrics import (
    llm_provider_calls,
    llm_provider_latency,
    http_requests_total
)

def test_llm_provider_metrics():
    """Test LLM provider metrics recording."""
    initial = llm_provider_calls.labels(
        provider="llamaserver",
        model="llama-3",
        status="success"
    )._value.get()

    llm_provider_calls.labels(
        provider="llamaserver",
        model="llama-3",
        status="success"
    ).inc()

    assert llm_provider_calls.labels(
        provider="llamaserver",
        model="llama-3",
        status="success"
    )._value.get() == initial + 1
```

### Integration Tests

#### Test 1: Metrics Endpoint Availability
```bash
# Start services
cd docker && docker-compose up -d

# Wait for startup
sleep 10

# Test Go metrics endpoint
curl http://localhost:9090/metrics | grep jeeves_

# Test Python metrics endpoint
curl http://localhost:8000/metrics | grep jeeves_

# Expected: Both return Prometheus-formatted metrics
```

#### Test 2: Pipeline Execution Metrics
```bash
# Execute a test pipeline via gRPC
grpcurl -d '{"envelope": {...}}' localhost:50051 engine.v1.EngineService/ExecutePipeline

# Check metrics
curl -s http://localhost:9090/metrics | grep jeeves_pipeline_executions_total

# Expected: Counter incremented, histogram has new sample
```

#### Test 3: Prometheus Scraping
```bash
# Check Prometheus targets
curl http://localhost:9090/api/v1/targets

# Expected: All targets (jeeves-grpc, jeeves-http-gateway) are "up"
```

#### Test 4: Query Metrics via PromQL
```bash
# Query total pipeline executions
curl 'http://localhost:9090/api/v1/query?query=jeeves_pipeline_executions_total'

# Query P95 latency
curl 'http://localhost:9090/api/v1/query?query=histogram_quantile(0.95, rate(jeeves_pipeline_duration_seconds_bucket[5m]))'

# Expected: Valid JSON responses with metric values
```

---

## 6. Implementation Order & Timeline

### Day 1: Go Infrastructure
- [ ] Add Prometheus Go client dependency (`go get`)
- [ ] Create `coreengine/observability/metrics.go`
- [ ] Instrument `runtime.Execute()` in `runtime/runtime.go`
- [ ] Instrument `agent.Process()` in `agents/agent.go`
- [ ] Add gRPC metrics interceptors to `grpc/interceptors.go`
- [ ] Add metrics endpoint to `grpc/server.go`
- [ ] Test: `go test ./coreengine/observability/...`

**Validation:** Run orchestrator, curl `localhost:9090/metrics`, see Go metrics

### Day 2: Python Instrumentation
- [ ] Add LLM metrics to `observability/metrics.py`
- [ ] Instrument LLM Gateway in `llm/gateway.py`
- [ ] Add HTTP middleware to `gateway/main.py`
- [ ] Mount `/metrics` endpoint in `gateway/main.py`
- [ ] Test: `pytest avionics/observability/`

**Validation:** Run gateway, curl `localhost:8000/metrics`, see Python metrics

### Day 3: Prometheus Integration
- [ ] Create `docker/prometheus.yml`
- [ ] Add Prometheus service to `docker-compose.yml`
- [ ] Expose metrics ports in service configs
- [ ] Start full stack: `docker-compose up`
- [ ] Verify Prometheus scraping all targets

**Validation:** Open `http://localhost:9090`, run queries, see data

### Day 4: Verification & Documentation
- [ ] Run full integration test suite
- [ ] Execute sample pipelines, verify metrics update
- [ ] Test error cases (agent failures, LLM errors)
- [ ] Document new metrics in README
- [ ] Create PromQL query examples
- [ ] Commit & push changes

---

## 7. Metrics Reference

### Pipeline Metrics

```promql
# Total pipeline executions by status
jeeves_pipeline_executions_total{pipeline="default-pipeline", status="success"}

# P95 pipeline latency
histogram_quantile(0.95,
  rate(jeeves_pipeline_duration_seconds_bucket[5m])
)

# Pipeline error rate
rate(jeeves_pipeline_executions_total{status="error"}[5m])
/
rate(jeeves_pipeline_executions_total[5m])
```

### Agent Metrics

```promql
# Agent execution rate
rate(jeeves_agent_executions_total[5m])

# Slowest agents (P99 latency)
topk(5,
  histogram_quantile(0.99,
    rate(jeeves_agent_duration_seconds_bucket[5m])
  )
)

# Agent error rate by agent
rate(jeeves_agent_executions_total{status="error"}[5m])
```

### LLM Metrics

```promql
# LLM calls per second by provider
rate(jeeves_llm_calls_total[1m])

# Token usage by model
rate(jeeves_llm_tokens_total{type="completion"}[5m])

# LLM latency by provider
avg(rate(jeeves_llm_duration_seconds_sum[5m]))
by (provider)

# LLM error rate
rate(jeeves_llm_calls_total{status="error"}[5m])
/
rate(jeeves_llm_calls_total[5m])
```

### HTTP Metrics

```promql
# Request rate by endpoint
rate(jeeves_http_requests_total[5m])

# 4xx/5xx error rates
rate(jeeves_http_requests_total{status_code=~"4.."}[5m])
rate(jeeves_http_requests_total{status_code=~"5.."}[5m])

# Slow HTTP endpoints
topk(10,
  histogram_quantile(0.95,
    rate(jeeves_http_request_duration_seconds_bucket[5m])
  )
)
```

### gRPC Metrics

```promql
# gRPC request rate
rate(jeeves_grpc_requests_total[5m])

# gRPC errors by method
rate(jeeves_grpc_requests_total{status!="success"}[5m])

# gRPC P99 latency
histogram_quantile(0.99,
  rate(jeeves_grpc_request_duration_seconds_bucket[5m])
)
```

---

## 8. Common Pitfalls & Solutions

### Issue: Metrics Port Conflicts
**Problem:** Port 9090 already in use
**Solution:** Use environment variable for metrics port
```go
metricsPort := os.Getenv("METRICS_PORT")
if metricsPort == "" {
    metricsPort = "9090"
}
```

### Issue: Duplicate Metric Registration
**Problem:** `panic: duplicate metrics collector registration attempted`
**Solution:** Use `promauto` (done) or check if metric exists before registering

### Issue: High Cardinality Labels
**Problem:** Using `request_id` or `envelope_id` as labels causes memory issues
**Solution:** Only use low-cardinality labels (pipeline name, agent name, status). Never use IDs.

### Issue: Metrics Missing in Prometheus
**Problem:** Prometheus shows "No data"
**Solution:**
1. Check Prometheus targets: `http://localhost:9090/targets`
2. Verify service exposes /metrics: `curl http://service:port/metrics`
3. Check Prometheus logs: `docker logs jeeves-prometheus`

### Issue: Wrong Duration Units
**Problem:** Histograms show milliseconds instead of seconds
**Solution:** Always convert to seconds: `float64(durationMS) / 1000.0`

---

## 9. Next Steps (Post-Phase 1)

### Phase 2: Distributed Tracing (Week 2-3)
- Add OpenTelemetry tracing to Go (already have SDK in Python)
- Instrument same points as metrics
- Add Jaeger to docker-compose
- Trace requests across Go/Python boundary via gRPC metadata

### Phase 3: Alerting (Week 4)
- Create alert rules in `prometheus/alerts.yml`
- Add Alertmanager to docker-compose
- Configure Slack/email notifications
- Define SLOs for key metrics

### Phase 4: Dashboards (Week 5)
- Add Grafana to docker-compose
- Create 5 core dashboards (service, pipeline, LLM, infra, business)
- Set up dashboard auto-provisioning
- Document dashboard usage

---

## 10. Success Criteria

### Phase 1 Complete When:
- ✅ All Go services expose `/metrics` endpoint
- ✅ All Python services expose `/metrics` endpoint
- ✅ Prometheus successfully scrapes all targets
- ✅ Can query `jeeves_pipeline_executions_total` and see data
- ✅ Can query `jeeves_llm_calls_total` and see provider breakdown
- ✅ Can query `jeeves_http_requests_total` and see endpoint latencies
- ✅ All tests passing (543 tests + new metrics tests)
- ✅ Documentation updated with metrics reference
- ✅ docker-compose up starts full observability stack

---

## Summary

This codebase is **exceptionally well-positioned** for observability implementation:

**Strengths:**
- Clean architecture with clear service boundaries
- Timing already calculated at all key points
- Strong logging infrastructure (easy to mirror with metrics)
- Python Prometheus support already exists
- Interceptor pattern makes gRPC instrumentation trivial
- Defer patterns ensure metrics recorded even on errors

**Implementation is straightforward:**
1. Add 1 Go package (metrics.go) - ~150 lines
2. Add 6 single-line metrics calls to existing code
3. Add 1 gRPC interceptor - ~40 lines
4. Add Python metrics definitions - ~30 lines
5. Add FastAPI middleware - ~15 lines
6. Update docker-compose - ~20 lines

**Total new code:** ~300 lines
**Modified existing code:** ~10 locations, ~20 lines total
**Estimated implementation time:** 3-4 days
**Risk level:** Low (non-invasive, fail-safe design)

The hardest part is already done (timing infrastructure). Now we just need to expose it as Prometheus metrics. 🚀
