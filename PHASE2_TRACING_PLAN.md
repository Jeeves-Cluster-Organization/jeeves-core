# Phase 2: Distributed Tracing Implementation Plan

**Date:** 2026-01-23
**Status:** PLANNED - Ready for Implementation
**Priority:** HIGH (P1)
**Prerequisites:** ✅ Phase 1 (Metrics) Complete
**Estimated Duration:** 4-5 days

---

## Executive Summary

With Phase 1 (Prometheus metrics) successfully implemented, Phase 2 adds distributed tracing to enable request-level debugging and performance analysis across the Go/Python boundary. This will provide end-to-end visibility into individual requests as they flow through the system.

**Value Proposition:**
- Debug slow requests by seeing exact component timings
- Identify bottlenecks in multi-agent pipelines
- Trace LLM calls with context propagation
- Correlate errors across service boundaries
- Optimize based on real request flows

---

## Current State Analysis

### What We Have (From Phase 1)

**Timing Infrastructure:**
- ✅ Pipeline execution timing (`runtime/runtime.go:152, 193`)
- ✅ Agent execution timing (`agents/agent.go:104, 116`)
- ✅ LLM call timing (`llm/gateway.py:356`)
- ✅ HTTP request timing (FastAPI middleware)
- ✅ gRPC request timing (interceptors)

**Existing Dependencies:**
- ✅ Python: `opentelemetry-api>=1.22.0`, `opentelemetry-sdk>=1.22.0` (already in pyproject.toml)
- ❌ Go: Need to add OpenTelemetry dependencies

**Service Architecture:**
```
HTTP Client
    ↓
FastAPI Gateway (Python) - Need tracing
    ↓ gRPC
Go Orchestrator (coreengine) - Need tracing
    ↓
Agent Execution
    ↓ gRPC
LLM Gateway (Python) - Need tracing
    ↓
LLM Provider
```

### What's Missing

1. **Go Side:**
   - OpenTelemetry SDK dependencies
   - Tracer initialization
   - Span creation in pipelines/agents
   - gRPC trace context propagation

2. **Python Side:**
   - Tracer initialization (SDK installed but not configured)
   - Span creation in LLM gateway
   - HTTP trace context extraction/injection
   - gRPC trace context propagation

3. **Infrastructure:**
   - Jaeger backend for trace collection
   - Trace storage configuration
   - Sampling strategy

---

## Architecture Design

### Trace Context Flow

```
┌─────────────────────────────────────────────────────────────────┐
│ HTTP Request                                                     │
│ Trace ID: abc123                                                 │
└───────────────────────────┬─────────────────────────────────────┘
                            ↓
                  ┌─────────────────────┐
                  │ FastAPI Gateway     │
                  │ Span: http.request  │ ← Extract trace from headers
                  └──────────┬──────────┘
                            ↓ gRPC (inject trace)
                  ┌─────────────────────┐
                  │ Go Orchestrator     │
                  │ Span: grpc.execute  │ ← Extract from gRPC metadata
                  └──────────┬──────────┘
                            ↓
              ┌─────────────┴─────────────┐
              ↓                           ↓
    ┌─────────────────┐         ┌─────────────────┐
    │ Pipeline        │         │ Agent           │
    │ Span: pipeline  │         │ Span: agent     │
    └─────────┬───────┘         └─────────┬───────┘
              ↓                           ↓ gRPC
    ┌─────────────────┐         ┌─────────────────┐
    │ Agent Process   │         │ LLM Gateway     │
    │ Span: agent     │         │ Span: llm.call  │
    └─────────────────┘         └─────────────────┘
```

### Span Hierarchy

```
http.request (FastAPI) [500ms]
├─ grpc.ExecutePipeline (gRPC client) [450ms]
│  └─ pipeline.execute (Go runtime) [445ms]
│     ├─ agent.process (Router) [50ms]
│     ├─ agent.process (Planner) [200ms]
│     │  └─ llm.generate (Python) [195ms]
│     │     └─ llm.provider.call (LlamaServer) [190ms]
│     └─ agent.process (Executor) [150ms]
└─ http.response (FastAPI) [3ms]
```

### Trace Attributes

**Standard Attributes:**
- `service.name`: "jeeves-gateway" | "jeeves-orchestrator"
- `service.version`: "4.0.0"
- `deployment.environment`: "development" | "production"

**Custom Attributes:**
- `jeeves.pipeline.name`: Pipeline configuration name
- `jeeves.agent.name`: Agent name
- `jeeves.agent.type`: "llm" | "tool" | "service"
- `jeeves.llm.provider`: "llamaserver" | "openai" | "anthropic"
- `jeeves.llm.model`: Model name
- `jeeves.llm.tokens.prompt`: Token count
- `jeeves.llm.tokens.completion`: Token count
- `jeeves.request.id`: Envelope/request ID
- `jeeves.user.id`: User ID (if available)

---

## Implementation Plan

### Day 1: Go Infrastructure Setup

#### Task 1.1: Add Dependencies

**File:** `go.mod`

```go
require (
    // ... existing ...
    go.opentelemetry.io/otel v1.22.0
    go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc v1.22.0
    go.opentelemetry.io/otel/sdk v1.22.0
    go.opentelemetry.io/contrib/instrumentation/google.golang.org/grpc/otelgrpc v0.47.0
)
```

**Commands:**
```bash
go get go.opentelemetry.io/otel@v1.22.0
go get go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc@v1.22.0
go get go.opentelemetry.io/otel/sdk@v1.22.0
go get go.opentelemetry.io/contrib/instrumentation/google.golang.org/grpc/otelgrpc@v0.47.0
```

#### Task 1.2: Create Tracing Package

**File:** `coreengine/observability/tracing.go` (NEW)

```go
package observability

import (
    "context"
    "fmt"

    "go.opentelemetry.io/otel"
    "go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc"
    "go.opentelemetry.io/otel/propagation"
    "go.opentelemetry.io/otel/sdk/resource"
    "go.opentelemetry.io/otel/sdk/trace"
    semconv "go.opentelemetry.io/otel/semconv/v1.21.0"
)

// InitTracer initializes OpenTelemetry tracing with OTLP exporter.
// Returns a shutdown function that should be called on service termination.
func InitTracer(serviceName, jaegerEndpoint string) (func(context.Context) error, error) {
    ctx := context.Background()

    // Create OTLP trace exporter
    exporter, err := otlptracegrpc.New(ctx,
        otlptracegrpc.WithEndpoint(jaegerEndpoint),
        otlptracegrpc.WithInsecure(), // Use TLS in production
    )
    if err != nil {
        return nil, fmt.Errorf("failed to create trace exporter: %w", err)
    }

    // Create resource with service information
    res, err := resource.New(ctx,
        resource.WithAttributes(
            semconv.ServiceName(serviceName),
            semconv.ServiceVersion("4.0.0"),
            semconv.DeploymentEnvironment("development"),
        ),
    )
    if err != nil {
        return nil, fmt.Errorf("failed to create resource: %w", err)
    }

    // Create trace provider
    tp := trace.NewTracerProvider(
        trace.WithBatcher(exporter),
        trace.WithResource(res),
        trace.WithSampler(trace.AlwaysSample()), // Use ParentBased(TraceIDRatioBased(0.1)) in production
    )

    // Set global tracer provider
    otel.SetTracerProvider(tp)

    // Set global propagator for context propagation
    otel.SetTextMapPropagator(propagation.NewCompositeTextMapPropagator(
        propagation.TraceContext{},
        propagation.Baggage{},
    ))

    // Return shutdown function
    return tp.Shutdown, nil
}

// Tracer returns a tracer for the given name
func Tracer(name string) trace.Tracer {
    return otel.Tracer(name)
}
```

#### Task 1.3: Instrument Pipeline Execution

**File:** `coreengine/runtime/runtime.go`

**Modifications:**
```go
import (
    "go.opentelemetry.io/otel"
    "go.opentelemetry.io/otel/attribute"
    "go.opentelemetry.io/otel/codes"
    "go.opentelemetry.io/otel/trace"
)

var tracer = otel.Tracer("jeeves-core/runtime")

func (r *PipelineRunner) Execute(ctx context.Context, env *envelope.Envelope, opts RunOptions) (*envelope.Envelope, <-chan StageOutput, error) {
    // Create span
    ctx, span := tracer.Start(ctx, "pipeline.execute",
        trace.WithAttributes(
            attribute.String("jeeves.pipeline.name", r.Config.Name),
            attribute.String("jeeves.request.id", env.RequestID),
            attribute.String("jeeves.envelope.id", env.EnvelopeID),
            attribute.String("pipeline.mode", string(opts.Mode)),
        ),
    )
    defer span.End()

    // ... existing initialization code ...

    startTime := time.Now()
    span.AddEvent("pipeline.started")

    // Run pipeline
    var resultEnv *envelope.Envelope
    var err error
    switch opts.Mode {
    case RunModeParallel:
        resultEnv, err = r.runParallelCore(ctx, env, opts, outputChan)
    default:
        resultEnv, err = r.runSequentialCore(ctx, env, opts, outputChan)
    }

    // Record result in span
    if err != nil {
        span.RecordError(err)
        span.SetStatus(codes.Error, err.Error())
    } else if resultEnv.Terminated {
        span.SetStatus(codes.Ok, "terminated")
        span.SetAttributes(attribute.String("termination.reason", *resultEnv.TerminationReason))
    } else {
        span.SetStatus(codes.Ok, "success")
    }

    span.AddEvent("pipeline.completed")

    // ... existing metrics and logging code ...
}
```

#### Task 1.4: Instrument Agent Execution

**File:** `coreengine/agents/agent.go`

```go
var tracer = otel.Tracer("jeeves-core/agents")

func (a *Agent) Process(ctx context.Context, env *envelope.Envelope) (*envelope.Envelope, error) {
    // Create span
    ctx, span := tracer.Start(ctx, "agent.process",
        trace.WithAttributes(
            attribute.String("jeeves.agent.name", a.Name),
            attribute.String("jeeves.agent.type", a.getAgentType()),
            attribute.String("jeeves.request.id", env.RequestID),
        ),
    )
    defer span.End()

    startTime := time.Now()
    llmCalls := 0

    // ... existing code ...

    defer func() {
        durationMS := int(time.Since(startTime).Milliseconds())
        if err != nil {
            span.RecordError(err)
            span.SetStatus(codes.Error, err.Error())
            // ... existing metrics ...
        } else {
            span.SetStatus(codes.Ok, "success")
            span.SetAttributes(
                attribute.Int("jeeves.llm.calls", llmCalls),
                attribute.Int("duration_ms", durationMS),
            )
            // ... existing metrics ...
        }
    }()

    // ... rest of implementation ...
}

func (a *Agent) getAgentType() string {
    if a.Config.HasLLM {
        return "llm"
    } else if a.Config.HasTools {
        return "tool"
    }
    return "service"
}
```

#### Task 1.5: Add gRPC Trace Interceptors

**File:** `coreengine/grpc/interceptors.go`

```go
import (
    "go.opentelemetry.io/contrib/instrumentation/google.golang.org/grpc/otelgrpc"
)

// Add to ServerOptions:
func ServerOptions(logger Logger) []grpc.ServerOption {
    return []grpc.ServerOption{
        grpc.UnaryInterceptor(ChainUnaryInterceptors(
            RecoveryInterceptor(logger, nil),
            MetricsInterceptor(),
            LoggingInterceptor(logger),
        )),
        grpc.StreamInterceptor(ChainStreamInterceptors(
            StreamRecoveryInterceptor(logger, nil),
            StreamMetricsInterceptor(),
            StreamLoggingInterceptor(logger),
        )),
        // OpenTelemetry interceptors (handles trace context automatically)
        grpc.StatsHandler(otelgrpc.NewServerHandler()),
    }
}
```

---

### Day 2: Python Tracing Setup

#### Task 2.1: Create Tracing Module

**File:** `jeeves_avionics/observability/tracing.py` (NEW)

```python
"""OpenTelemetry tracing configuration for jeeves_avionics."""

from opentelemetry import trace
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.sdk.resources import Resource, SERVICE_NAME, SERVICE_VERSION, DEPLOYMENT_ENVIRONMENT
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
from opentelemetry.instrumentation.grpc import GrpcInstrumentorClient

_tracer_provider: TracerProvider | None = None


def init_tracing(service_name: str, jaeger_endpoint: str = "jaeger:4317") -> None:
    """Initialize OpenTelemetry tracing.

    Args:
        service_name: Service name for traces
        jaeger_endpoint: Jaeger OTLP endpoint (default: jaeger:4317)
    """
    global _tracer_provider

    # Create resource
    resource = Resource.create({
        SERVICE_NAME: service_name,
        SERVICE_VERSION: "4.0.0",
        DEPLOYMENT_ENVIRONMENT: "development",
    })

    # Create tracer provider
    _tracer_provider = TracerProvider(resource=resource)

    # Add OTLP exporter
    otlp_exporter = OTLPSpanExporter(
        endpoint=jaeger_endpoint,
        insecure=True,  # Use TLS in production
    )
    _tracer_provider.add_span_processor(BatchSpanProcessor(otlp_exporter))

    # Set global tracer provider
    trace.set_tracer_provider(_tracer_provider)


def instrument_fastapi(app) -> None:
    """Instrument FastAPI app with OpenTelemetry.

    Args:
        app: FastAPI application instance
    """
    FastAPIInstrumentor.instrument_app(app)


def instrument_grpc_client() -> None:
    """Instrument gRPC clients with OpenTelemetry."""
    GrpcInstrumentorClient().instrument()


def get_tracer(name: str) -> trace.Tracer:
    """Get a tracer for the given name.

    Args:
        name: Tracer name (usually module path)

    Returns:
        OpenTelemetry tracer
    """
    return trace.get_tracer(name)


def shutdown_tracing() -> None:
    """Shutdown tracer provider and flush pending spans."""
    global _tracer_provider
    if _tracer_provider:
        _tracer_provider.shutdown()
```

#### Task 2.2: Instrument LLM Gateway

**File:** `jeeves_avionics/llm/gateway.py`

```python
from jeeves_avionics.observability.tracing import get_tracer
from opentelemetry import trace
from opentelemetry.trace import Status, StatusCode

# At module level
tracer = get_tracer(__name__)

async def _call_provider(...) -> LLMResponse:
    """Call a specific provider and return standardized response."""

    # Create span
    with tracer.start_as_current_span(
        "llm.provider.call",
        attributes={
            "jeeves.llm.provider": provider_name,
            "jeeves.llm.model": model_name,
            "jeeves.agent.name": agent_name,
            "jeeves.request.id": request_id,
        }
    ) as span:
        try:
            # Call provider
            llm_result = await provider.generate(...)

            # Add token metrics to span
            span.set_attributes({
                "jeeves.llm.tokens.prompt": prompt_tokens,
                "jeeves.llm.tokens.completion": completion_tokens,
                "jeeves.llm.tokens.total": total_tokens,
                "jeeves.llm.cost_usd": cost_metrics.cost_usd,
            })

            span.set_status(Status(StatusCode.OK))

        except Exception as e:
            span.record_exception(e)
            span.set_status(Status(StatusCode.ERROR, str(e)))
            raise

        # Record metrics (existing code)
        record_llm_call(...)
        record_llm_tokens(...)

        return LLMResponse(...)
```

#### Task 2.3: Update FastAPI Gateway

**File:** `jeeves_avionics/gateway/main.py`

```python
from jeeves_avionics.observability.tracing import init_tracing, instrument_fastapi, instrument_grpc_client

@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """Lifespan context manager for startup/shutdown."""

    # Initialize tracing
    jaeger_endpoint = os.getenv("JAEGER_ENDPOINT", "jaeger:4317")
    init_tracing("jeeves-gateway", jaeger_endpoint)

    # Instrument FastAPI
    instrument_fastapi(app)

    # Instrument gRPC client
    instrument_grpc_client()

    # ... existing startup code ...

    yield

    # ... existing shutdown code ...
```

---

### Day 3: Infrastructure Setup

#### Task 3.1: Add Jaeger to docker-compose.yml

**File:** `docker/docker-compose.yml`

```yaml
  jaeger:
    image: jaegertracing/all-in-one:latest
    container_name: jeeves-jaeger
    environment:
      - COLLECTOR_OTLP_ENABLED=true
      - SPAN_STORAGE_TYPE=badger
      - BADGER_EPHEMERAL=false
      - BADGER_DIRECTORY_VALUE=/badger/data
      - BADGER_DIRECTORY_KEY=/badger/key
    ports:
      - "16686:16686"  # Jaeger UI
      - "4317:4317"    # OTLP gRPC receiver
      - "4318:4318"    # OTLP HTTP receiver
    volumes:
      - jaeger_data:/badger
    healthcheck:
      test: ["CMD", "wget", "--quiet", "--tries=1", "--spider", "http://localhost:16686/"]
      interval: 30s
      timeout: 10s
      retries: 3
    restart: unless-stopped
    networks:
      - jeeves-network

volumes:
  # ... existing ...
  jaeger_data:
    driver: local
```

#### Task 3.2: Update Service Configurations

**Orchestrator environment:**
```yaml
  orchestrator:
    environment:
      # ... existing ...
      JAEGER_ENDPOINT: jaeger:4317
      OTEL_SERVICE_NAME: jeeves-orchestrator
```

**Gateway environment:**
```yaml
  gateway:
    environment:
      # ... existing ...
      JAEGER_ENDPOINT: jaeger:4317
      OTEL_SERVICE_NAME: jeeves-gateway
```

---

### Day 4: Testing & Validation

#### Task 4.1: Integration Test

**Test Scenario:**
```bash
# Start services
docker-compose up -d postgres llama-server prometheus jaeger gateway orchestrator

# Make test request
curl -X POST http://localhost:8000/api/v1/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello", "session_id": "trace-test-123"}'

# Open Jaeger UI
open http://localhost:16686

# Search for traces:
# - Service: jeeves-gateway
# - Operation: POST /api/v1/chat
```

**Expected Result:**
- See complete trace with spans:
  - `http.request` (FastAPI)
  - `grpc.ExecutePipeline` (gRPC client)
  - `pipeline.execute` (Go runtime)
  - `agent.process` (multiple agents)
  - `llm.provider.call` (LLM gateway)

#### Task 4.2: Performance Validation

**Metrics to Check:**
```promql
# Trace overhead (should be <5%)
rate(jeeves_pipeline_duration_seconds_sum[5m]) / rate(jeeves_pipeline_duration_seconds_count[5m])

# Before vs after tracing enabled
```

#### Task 4.3: Sampling Configuration

**Production Configuration:**

```go
// Go: Use ratio-based sampling (10%)
trace.WithSampler(trace.ParentBased(trace.TraceIDRatioBased(0.1)))
```

```python
# Python: Use ratio-based sampling
from opentelemetry.sdk.trace.sampling import TraceIdRatioBased
tracer_provider = TracerProvider(
    sampler=TraceIdRatioBased(0.1),  # 10% sampling
    resource=resource
)
```

---

### Day 5: Documentation & Handoff

#### Task 5.1: Update METRICS_README.md

Add tracing section with:
- Getting started guide
- Jaeger UI walkthrough
- Trace query examples
- Troubleshooting

#### Task 5.2: Create TRACING_QUERIES.md

Document common trace queries:
- Find slow requests
- Debug errors
- Analyze agent performance
- Identify bottlenecks

#### Task 5.3: Update README.md

Add tracing to "What's Been Achieved" section.

---

## Success Criteria

### Phase 2 Complete When:

- ✅ All services export traces to Jaeger
- ✅ Traces span Go/Python boundary correctly
- ✅ Can trace individual requests end-to-end
- ✅ <5% performance overhead
- ✅ Sampling works correctly
- ✅ Documentation complete
- ✅ Team trained on Jaeger UI

### Key Metrics:

1. **Trace Completeness**: >95% of requests have complete traces
2. **Trace Latency**: <100ms from span creation to export
3. **Performance Overhead**: <5% increase in P95 latency
4. **Sampling Rate**: 10% in production, 100% in development

---

## Common Trace Queries

### Find Slow Requests

In Jaeger UI:
- Service: jeeves-gateway
- Min Duration: 5s
- Limit: 20

### Debug Specific Request

Search by tag:
- `jeeves.request.id = "abc-123"`

### Analyze Agent Performance

- Service: jeeves-orchestrator
- Operation: agent.process
- Tag: `jeeves.agent.name = "planner"`

### Find LLM Bottlenecks

- Service: jeeves-gateway
- Operation: llm.provider.call
- Min Duration: 2s

---

## Risks & Mitigation

### Risk 1: Performance Overhead

**Mitigation:**
- Use sampling (10% in production)
- Async trace export
- Monitor overhead with Prometheus

### Risk 2: Trace Context Loss

**Mitigation:**
- Test trace propagation across all boundaries
- Add integration tests
- Monitor trace completion rate

### Risk 3: Storage Growth

**Mitigation:**
- Use Badger with retention policy (7 days)
- Or use Elasticsearch with lifecycle management
- Monitor storage usage

---

## Next Steps After Phase 2

1. **Phase 3: Alerting** - Add Alertmanager, define SLOs
2. **Phase 4: Dashboards** - Grafana with trace integration
3. **Phase 5: Advanced** - Tail-based sampling, trace analysis automation

---

**Status:** Ready for implementation
**Estimated Effort:** 4-5 days (1 engineer)
**Dependencies:** Phase 1 complete ✅
**Risk Level:** Low (additive, non-breaking changes)
