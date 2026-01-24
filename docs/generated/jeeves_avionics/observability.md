# Observability Module

Logging, metrics, and OpenTelemetry integration for distributed tracing.

## Navigation

- [README](./README.md) - Overview
- [Settings](./settings.md)
- [Gateway](./gateway.md)
- [Database](./database.md)
- [LLM](./llm.md)
- **Observability** (this file)
- [Tools](./tools.md)
- [Infrastructure](./infrastructure.md)

---

## logging/__init__.py

Centralized logging infrastructure.

### Configuration Functions

```python
def configure_logging(
    level: str = "INFO",
    *,
    json_output: bool = True,
    enable_otel: bool = False,
    component_levels: Optional[Dict[str, str]] = None,
) -> None:
    """Configure logging for the Jeeves runtime.
    
    Should be called ONCE at application startup.
    """

def configure_from_flags(flags: FeatureFlags) -> None:
    """Configure logging from feature flags."""

def is_otel_enabled() -> bool:
    """Check if OTEL tracing is enabled."""
```

### Logger Creation Functions

```python
def create_logger(name: str, **context) -> LoggerProtocol:
    """Create a logger with bound context."""

def create_agent_logger(agent_name: str, **context) -> LoggerProtocol:
    """Create a logger for an agent."""

def create_capability_logger(capability: str, **context) -> LoggerProtocol:
    """Create a logger for a capability."""

def create_tool_logger(tool_name: str, **context) -> LoggerProtocol:
    """Create a logger for a tool."""

def get_component_logger(component: str, parent: Optional[LoggerProtocol] = None) -> LoggerProtocol:
    """Get a logger for a component."""

def get_current_logger() -> LoggerProtocol:
    """Get the current context logger."""

def set_current_logger(logger: LoggerProtocol) -> None:
    """Set the current context logger."""
```

### Context Management

```python
@contextmanager
def request_scope(context: RequestContext, logger: LoggerProtocol):
    """Context manager for request-scoped logging."""

def get_request_context() -> Optional[RequestContext]:
    """Get the current request context."""

def set_request_context(context: RequestContext) -> None:
    """Set the current request context."""
```

---

## logging/adapter.py

StructlogAdapter for dependency injection.

### Class: StructlogAdapter

```python
class StructlogAdapter(LoggerProtocol):
    """Adapts structlog to LoggerProtocol for dependency injection."""
    
    def __init__(self, logger: Optional[structlog.BoundLogger] = None):
        ...
    
    def debug(self, msg: str, **kwargs: Any) -> None: ...
    def info(self, msg: str, **kwargs: Any) -> None: ...
    def warning(self, msg: str, **kwargs: Any) -> None: ...
    def error(self, msg: str, **kwargs: Any) -> None: ...
    def exception(self, msg: str, **kwargs: Any) -> None: ...
    
    def bind(self, **kwargs: Any) -> StructlogAdapter:
        """Create child logger with bound context."""
    
    def unbind(self, *keys: str) -> StructlogAdapter:
        """Create child logger without specified context keys."""
    
    def new(self, **kwargs: Any) -> StructlogAdapter:
        """Create new logger with only the specified context."""
```

### Factory Function

```python
def create_structlog_adapter(**context: Any) -> StructlogAdapter:
    """Create a StructlogAdapter with optional initial context."""
```

---

## logging/context.py

Context propagation for logging and request tracing.

### Context Managers

```python
@contextmanager
def bind_logger_context(**kwargs) -> Generator[LoggerProtocol, None, None]:
    """Temporarily bind additional context to the current logger."""

@contextmanager
def request_context(request_id: str, user_id: str, **extra_context) -> Iterator[None]:
    """Bind request context to all log messages within the context."""
```

---

## observability/metrics.py

Prometheus metrics instrumentation.

### Counters

```python
ORCHESTRATOR_REQUESTS = Counter(
    "orchestrator_requests_total",
    "Total orchestrator requests by outcome.",
    labelnames=("outcome",),
)

META_VALIDATION_OUTCOMES = Counter(
    "meta_validator_reports_total",
    "Total meta-validation reports by status.",
    labelnames=("status",),
)

WORKFLOW_RETRY_ATTEMPTS = Counter(
    "workflow_retry_attempts_total",
    "Total retry attempts by type.",
    labelnames=("retry_type",),
)

CRITIC_DECISIONS = Counter(
    "critic_decisions_total",
    "Critic decisions by action.",
    labelnames=("action",),
)
```

### Gauges

```python
ORCHESTRATOR_INFLIGHT = Gauge(
    "orchestrator_inflight_requests",
    "Number of requests currently being processed.",
)
```

### Histograms

```python
ORCHESTRATOR_LATENCY = Histogram(
    "orchestrator_request_latency_seconds",
    "End-to-end orchestration latency.",
    buckets=(0.1, 0.25, 0.5, 1, 2, 3, 5, 7, 10, 15),
)

CRITIC_CONFIDENCE = Histogram(
    "critic_decision_confidence",
    "Distribution of critic confidence scores by action.",
    labelnames=("action",),
    buckets=(0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0),
)
```

### Recording Functions

```python
def orchestrator_started() -> None:
    """Increment in-flight gauge when orchestration begins."""

def orchestrator_completed(outcome: str, duration_ms: float) -> None:
    """Record orchestrator outcome and duration."""

def orchestrator_failed(duration_ms: float) -> None:
    """Record orchestrator failure metrics."""

def record_meta_validation(report: VerificationReport) -> None:
    """Emit Prometheus metrics for a meta-validation report."""

def record_retry_attempt(retry_type: str, issue_type: str = "unknown") -> None:
    """Record a workflow retry attempt."""

def record_critic_decision(action: str, confidence: float) -> None:
    """Record a critic decision with confidence."""

def start_metrics_server(port: int = 9000) -> None:
    """Start a background HTTP server exposing Prometheus metrics."""
```

---

## observability/otel_adapter.py

OpenTelemetry adapter for distributed tracing.

### Dataclass: TracingContext

```python
@dataclass
class TracingContext:
    """Context for trace propagation across process boundaries."""
    trace_id: str
    span_id: str
    trace_flags: int = 1
    trace_state: str = ""
    baggage: Dict[str, str] = field(default_factory=dict)
    
    def to_dict(self) -> Dict[str, Any]: ...
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> TracingContext: ...
```

### Functions

```python
def create_tracer(
    service_name: str = "jeeves",
    service_version: str = "1.0.0",
    exporter: Optional[Any] = None,
) -> Optional[Tracer]:
    """Create an OpenTelemetry tracer."""

def get_current_span() -> Optional[Span]:
    """Get the current active span."""

def inject_trace_context(carrier: Dict[str, str]) -> None:
    """Inject trace context into a carrier dict for propagation."""

def extract_trace_context(carrier: Dict[str, str]) -> Optional[Any]:
    """Extract trace context from a carrier dict."""
```

### Class: OpenTelemetryAdapter

```python
class OpenTelemetryAdapter:
    """Adapter that bridges EventEmitter events to OpenTelemetry spans.
    
    Event to Span Mapping:
    - agent_started -> Start span with agent.name attribute
    - agent_completed -> End span with status
    - tool_executed -> Child span under current agent
    """
    
    def __init__(
        self,
        tracer: Optional[Tracer] = None,
        logger: Optional[LoggerProtocol] = None,
        service_name: str = "jeeves",
    ):
        ...
    
    @property
    def enabled(self) -> bool:
        """Check if OpenTelemetry tracing is enabled."""
```

#### Span Management

```python
@contextmanager
def start_span(
    self,
    name: str,
    kind: Optional[Any] = None,
    attributes: Optional[Dict[str, Any]] = None,
    context_key: str = "default",
) -> Iterator[Optional[Span]]:
    """Start a new span as a context manager."""
```

#### Event Handlers

```python
async def on_agent_started(self, agent_name: str, request_id: str, stage_order: int = 0, **kwargs) -> Optional[str]:
    """Handle agent_started event - creates a new span."""

async def on_agent_completed(self, agent_name: str, request_id: str, status: str = "success", error: Optional[str] = None, **kwargs) -> None:
    """Handle agent_completed event - ends the span."""

async def on_tool_executed(self, tool_name: str, request_id: str, status: str = "success", execution_time_ms: Optional[int] = None, **kwargs) -> None:
    """Handle tool_executed event - creates a child span."""

async def on_llm_call(self, request_id: str, provider: str, model: str, agent_name: str, tokens_in: int = 0, tokens_out: int = 0, **kwargs) -> None:
    """Handle LLM call event - creates a child span."""
```

#### Context Management

```python
def record_event(self, name: str, request_id: str, attributes: Optional[Dict] = None) -> None:
    """Record an event on the current span."""

def set_attribute(self, key: str, value: Any, request_id: str) -> None:
    """Set an attribute on the current span."""

def get_trace_context(self, request_id: str) -> Optional[TracingContext]:
    """Get the current trace context for propagation."""

def cleanup_context(self, request_id: str) -> None:
    """Clean up span stack for a completed request."""
```

### Global Adapter Functions

```python
def get_global_otel_adapter() -> Optional[OpenTelemetryAdapter]:
    """Get the global OpenTelemetry adapter."""

def set_global_otel_adapter(adapter: OpenTelemetryAdapter) -> None:
    """Set the global OpenTelemetry adapter."""

def init_global_otel(
    service_name: str = "jeeves",
    service_version: str = "1.0.0",
    exporter: Optional[Any] = None,
) -> Optional[OpenTelemetryAdapter]:
    """Initialize global OpenTelemetry adapter."""
```

---

## observability/tracing_middleware.py

Tracing decorators and middleware.

### Class: TracingMiddleware

```python
class TracingMiddleware:
    """Middleware for adding tracing to request handlers."""
    
    def __init__(
        self,
        adapter: Optional[OpenTelemetryAdapter] = None,
        default_kind: Optional[Any] = None,
    ):
        ...
    
    def trace(
        self,
        name: str,
        kind: Optional[Any] = None,
        attributes: Optional[Dict[str, Any]] = None,
    ) -> Callable[[F], F]:
        """Decorator for tracing a function."""
    
    def wrap(
        self,
        handler: Callable,
        name: str,
        kind: Optional[Any] = None,
    ) -> Callable:
        """Wrap an existing handler with tracing."""
```

### Decorators

```python
def trace_function(
    name: str,
    kind: Optional[Any] = None,
    attributes: Optional[Dict[str, Any]] = None,
) -> Callable[[F], F]:
    """Decorator for tracing a function using the global adapter."""
```

### Context Managers

```python
@asynccontextmanager
async def trace_block(
    name: str,
    request_id: str = "default",
    attributes: Optional[Dict[str, Any]] = None,
):
    """Async context manager for tracing a code block."""
```

### Class: SpanContextPropagator

```python
class SpanContextPropagator:
    """Helper for propagating trace context across boundaries."""
    
    def inject(self, request_id: str) -> Dict[str, str]:
        """Inject trace context into headers."""
    
    def inject_to_envelope(self, envelope: Any, request_id: Optional[str] = None) -> None:
        """Inject trace context into a Envelope."""
    
    def extract_from_envelope(self, envelope: Any) -> Optional[Dict[str, str]]:
        """Extract trace context from a Envelope."""
```

---

## Usage Examples

### Logging Setup

```python
from jeeves_avionics.logging import configure_logging, create_logger

configure_logging(level="INFO", json_output=True, enable_otel=True)

logger = create_logger("my_component", request_id="req-123")
logger.info("processing_request", user_id="user-456")
```

### Request Scoped Logging

```python
from jeeves_avionics.logging.context import request_context

with request_context("req-123", "user-456"):
    logger.info("processing_request")  # Automatically includes request_id, user_id
```

### Tracing with Decorators

```python
from jeeves_avionics.observability import trace_function

@trace_function("process_data")
async def process_data(data):
    # Automatically traced
    pass
```

### Manual Span Management

```python
from jeeves_avionics.observability import get_global_otel_adapter

adapter = get_global_otel_adapter()
if adapter and adapter.enabled:
    with adapter.start_span("my_operation", attributes={"key": "value"}) as span:
        # Do work
        span.set_attribute("result", "success")
```

### Prometheus Metrics

```python
from jeeves_avionics.observability.metrics import (
    orchestrator_started,
    orchestrator_completed,
    start_metrics_server,
)

# Start metrics server
start_metrics_server(port=9000)

# Record metrics
orchestrator_started()
try:
    result = await process_request()
    orchestrator_completed("success", duration_ms=150.0)
except Exception:
    orchestrator_completed("error", duration_ms=50.0)
```
