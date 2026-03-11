"""Prometheus metrics instrumentation for the agent pipeline."""

from __future__ import annotations

try:
    from prometheus_client import Counter, Gauge, Histogram, start_http_server
except ImportError:  # pragma: no cover - fallback when Prometheus is unavailable
    import warnings

    class _NoopMetric:
        def labels(self, **_kwargs):
            return self

        def inc(self, *_args, **_kwargs):
            return None

        def dec(self, *_args, **_kwargs):
            return None

        def observe(self, *_args, **_kwargs):
            return None

    def Counter(*_args, **_kwargs):  # type: ignore
        warnings.warn("prometheus-client not installed; metrics are disabled", RuntimeWarning)
        return _NoopMetric()

    def Gauge(*_args, **_kwargs):  # type: ignore
        warnings.warn("prometheus-client not installed; metrics are disabled", RuntimeWarning)
        return _NoopMetric()

    def Histogram(*_args, **_kwargs):  # type: ignore
        warnings.warn("prometheus-client not installed; metrics are disabled", RuntimeWarning)
        return _NoopMetric()

    def start_http_server(*_args, **_kwargs):  # type: ignore
        warnings.warn("prometheus-client not installed; metrics server not started", RuntimeWarning)
        return None

ORCHESTRATOR_INFLIGHT = Gauge(
    "orchestrator_inflight_requests",
    "Number of requests currently being processed by the orchestrator.",
)

ORCHESTRATOR_REQUESTS = Counter(
    "orchestrator_requests_total",
    "Total orchestrator requests by outcome.",
    labelnames=("outcome",),
)

ORCHESTRATOR_LATENCY = Histogram(
    "orchestrator_request_latency_seconds",
    "End-to-end orchestration latency in seconds.",
    buckets=(0.1, 0.25, 0.5, 1, 2, 3, 5, 7, 10, 15),
)

# ============================================================
# LLM Provider Metrics
# ============================================================

LLM_PROVIDER_CALLS = Counter(
    "airframe_llm_provider_calls_total",
    "LLM provider calls from Python layer",
    labelnames=("provider", "model", "status"),
)

LLM_PROVIDER_LATENCY = Histogram(
    "airframe_llm_provider_duration_seconds",
    "LLM provider call duration in seconds",
    labelnames=("provider", "model"),
    buckets=(0.1, 0.5, 1, 2, 5, 10, 30, 60),
)

LLM_TOKENS_USED = Counter(
    "airframe_llm_tokens_total",
    "Total LLM tokens used",
    labelnames=("provider", "model", "type"),  # type: prompt, completion
)

# ============================================================
# HTTP Gateway Metrics
# ============================================================

HTTP_REQUESTS_TOTAL = Counter(
    "jeeves_http_requests_total",
    "Total HTTP requests to FastAPI gateway",
    labelnames=("method", "path", "status_code"),
)

HTTP_REQUEST_DURATION = Histogram(
    "jeeves_http_request_duration_seconds",
    "HTTP request duration in seconds",
    labelnames=("method", "path"),
    buckets=(0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10),
)

# ============================================================
# Kernel-Unique Metrics (no other framework can emit these)
# ============================================================

PIPELINE_TERMINATIONS = Counter(
    "jeeves_pipeline_terminations_total",
    "Pipeline terminations by terminal reason",
    labelnames=("reason",),
)

KERNEL_INSTRUCTIONS = Counter(
    "jeeves_kernel_instructions_total",
    "Kernel instructions dispatched by type",
    labelnames=("instruction_type",),
)

AGENT_EXECUTION_DURATION = Histogram(
    "jeeves_agent_execution_duration_seconds",
    "Per-agent execution duration",
    labelnames=("agent_name",),
    buckets=(0.1, 0.5, 1, 2, 5, 10, 30, 60),
)

TOOL_EXECUTIONS = Counter(
    "jeeves_tool_executions_total",
    "Tool executions by tool name and status",
    labelnames=("tool_name", "status"),
)


def orchestrator_started() -> None:
    """Increment in-flight gauge when orchestration begins."""

    ORCHESTRATOR_INFLIGHT.inc()


def orchestrator_completed(outcome: str, duration_ms: float) -> None:
    """Record orchestrator outcome, duration, and update counters."""

    ORCHESTRATOR_INFLIGHT.dec()
    ORCHESTRATOR_REQUESTS.labels(outcome=outcome).inc()
    ORCHESTRATOR_LATENCY.observe(max(duration_ms, 0.0) / 1000.0)


def orchestrator_failed(duration_ms: float) -> None:
    """Record orchestrator failure metrics."""

    orchestrator_completed("failed", duration_ms)


def orchestrator_rejected(reason: str) -> None:
    """Record orchestrator rejections that occur before work begins."""

    ORCHESTRATOR_REQUESTS.labels(outcome=reason).inc()


def start_metrics_server(port: int = 9000) -> None:
    """Start a background HTTP server exposing Prometheus metrics."""

    start_http_server(port)


# ============================================================
# LLM Provider Metric Recording Functions
# ============================================================

def record_llm_call(provider: str, model: str, status: str, duration_seconds: float) -> None:
    """Record an LLM provider call with duration.

    Args:
        provider: Provider name (llamaserver, openai, anthropic, etc.)
        model: Model name
        status: Call status (success, error)
        duration_seconds: Call duration in seconds
    """
    LLM_PROVIDER_CALLS.labels(provider=provider, model=model, status=status).inc()
    LLM_PROVIDER_LATENCY.labels(provider=provider, model=model).observe(duration_seconds)


def record_llm_tokens(provider: str, model: str, prompt_tokens: int, completion_tokens: int) -> None:
    """Record LLM token usage.

    Args:
        provider: Provider name
        model: Model name
        prompt_tokens: Number of prompt tokens
        completion_tokens: Number of completion tokens
    """
    if prompt_tokens > 0:
        LLM_TOKENS_USED.labels(provider=provider, model=model, type="prompt").inc(prompt_tokens)
    if completion_tokens > 0:
        LLM_TOKENS_USED.labels(provider=provider, model=model, type="completion").inc(completion_tokens)


# ============================================================
# HTTP Gateway Metric Recording Functions
# ============================================================

def record_http_request(method: str, path: str, status_code: int, duration_seconds: float) -> None:
    """Record an HTTP request with duration.

    Args:
        method: HTTP method (GET, POST, etc.)
        path: Request path
        status_code: HTTP status code
        duration_seconds: Request duration in seconds
    """
    HTTP_REQUESTS_TOTAL.labels(method=method, path=path, status_code=str(status_code)).inc()
    HTTP_REQUEST_DURATION.labels(method=method, path=path).observe(duration_seconds)


# ============================================================
# Kernel-Unique Metric Recording Functions
# ============================================================

def record_pipeline_termination(reason: str) -> None:
    """Record a pipeline termination by terminal reason."""
    PIPELINE_TERMINATIONS.labels(reason=reason).inc()


def record_kernel_instruction(instruction_type: str) -> None:
    """Record a kernel instruction dispatched by type."""
    KERNEL_INSTRUCTIONS.labels(instruction_type=instruction_type).inc()


def record_agent_duration(agent_name: str, duration_seconds: float) -> None:
    """Record per-agent execution duration."""
    AGENT_EXECUTION_DURATION.labels(agent_name=agent_name).observe(duration_seconds)


def record_tool_execution(tool_name: str, status: str) -> None:
    """Record a tool execution by tool name and status."""
    TOOL_EXECUTIONS.labels(tool_name=tool_name, status=status).inc()
