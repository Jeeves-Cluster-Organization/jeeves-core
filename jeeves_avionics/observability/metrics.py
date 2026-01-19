"""Prometheus metrics instrumentation for the 7-agent stack."""

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

from jeeves_mission_system.common.models import VerificationReport


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

META_VALIDATION_OUTCOMES = Counter(
    "meta_validator_reports_total",
    "Total meta-validation reports partitioned by approval status.",
    labelnames=("status",),
)

META_VALIDATION_ISSUES = Counter(
    "meta_validator_issue_detections_total",
    "Counts of individual issue detections by type.",
    labelnames=("issue_type",),
)

# ============================================================
# Retry Metrics (Amendment IX: Workflow Observability)
# v1.0 Hardening: Track retry attempts and reasons
# ============================================================

WORKFLOW_RETRY_ATTEMPTS = Counter(
    "workflow_retry_attempts_total",
    "Total retry attempts by type (planner_retry, validator_retry).",
    labelnames=("retry_type",),
)

WORKFLOW_RETRY_REASONS = Counter(
    "workflow_retry_reasons_total",
    "Retry reasons by issue type.",
    labelnames=("retry_type", "issue_type"),
)

CRITIC_DECISIONS = Counter(
    "critic_decisions_total",
    "Critic decisions by action taken.",
    labelnames=("action",),
)

CRITIC_CONFIDENCE = Histogram(
    "critic_decision_confidence",
    "Distribution of critic confidence scores by action.",
    labelnames=("action",),
    buckets=(0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0),
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


def record_meta_validation(report: VerificationReport) -> None:
    """Emit Prometheus metrics for a meta-validation report."""

    status = "approved" if report.approved else "rejected"
    META_VALIDATION_OUTCOMES.labels(status=status).inc()
    for issue in report.issues_found:
        META_VALIDATION_ISSUES.labels(issue_type=issue.type).inc()


def start_metrics_server(port: int = 9000) -> None:
    """Start a background HTTP server exposing Prometheus metrics."""

    start_http_server(port)


# ============================================================
# Retry Metric Recording Functions (Amendment IX)
# ============================================================

def record_retry_attempt(retry_type: str, issue_type: str = "unknown") -> None:
    """Record a workflow retry attempt.

    Args:
        retry_type: Type of retry (planner_retry, validator_retry)
        issue_type: Reason for retry (e.g., tool_count_mismatch, wrong_tool_choice)
    """
    WORKFLOW_RETRY_ATTEMPTS.labels(retry_type=retry_type).inc()
    WORKFLOW_RETRY_REASONS.labels(retry_type=retry_type, issue_type=issue_type).inc()


def record_critic_decision(action: str, confidence: float) -> None:
    """Record a critic decision with confidence.

    Args:
        action: Decision action (accept, clarify, retry_validator, retry_planner)
        confidence: Confidence score (0.0-1.0)
    """
    CRITIC_DECISIONS.labels(action=action).inc()
    CRITIC_CONFIDENCE.labels(action=action).observe(confidence)
