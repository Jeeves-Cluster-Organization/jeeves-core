"""Health check endpoints for production monitoring.

Provides /health and /ready endpoints following Kubernetes conventions:
- /health: Liveness check (is the service running?)
- /ready: Readiness check (can the service accept requests?)
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from datetime import datetime, timezone
from enum import Enum
from time import monotonic
from typing import Any, Dict, Optional

from jeeves_infra.protocols import DatabaseClientProtocol, HealthStatus, LLMProviderProtocol
from jeeves_infra.config.constants import PLATFORM_VERSION


class ComponentStatus(str, Enum):
    """Individual component status values."""

    UP = "up"
    DOWN = "down"
    DEGRADED = "degraded"


@dataclass
class ComponentHealth:
    """Health status for an individual component."""

    status: ComponentStatus
    message: Optional[str] = None
    latency_ms: Optional[float] = None
    last_check: Optional[datetime] = None


@dataclass
class HealthCheckResult:
    """Complete health check result."""

    status: HealthStatus
    timestamp: datetime
    uptime_seconds: float
    components: Dict[str, ComponentHealth]
    version: str = PLATFORM_VERSION


class HealthChecker:
    """Performs health checks on system components."""

    def __init__(
        self,
        db: DatabaseClientProtocol,
        llm_provider: LLMProviderProtocol,
        *,
        database_timeout: float = 2.0,
        model_timeout: float = 5.0,
    ) -> None:
        """Initialize health checker.

        Args:
            db: Database client for persistence checks
            llm_provider: Active LLM provider instance
            database_timeout: Timeout for database health checks (seconds)
            model_timeout: Timeout for model health checks (seconds)
        """
        self.db = db
        self.llm_provider = llm_provider
        self.database_timeout = database_timeout
        self.model_timeout = model_timeout
        self._start_time = monotonic()

    async def check_liveness(self) -> HealthCheckResult:
        """Perform liveness check (basic service availability).

        This is a lightweight check that only verifies the service is running.
        Kubernetes uses this to determine if the pod should be restarted.
        """
        now = datetime.now(timezone.utc)
        uptime = monotonic() - self._start_time

        return HealthCheckResult(
            status=HealthStatus.HEALTHY,
            timestamp=now,
            uptime_seconds=uptime,
            components={
                "api": ComponentHealth(
                    status=ComponentStatus.UP,
                    message="Service is running",
                    last_check=now,
                )
            },
        )

    async def check_readiness(self) -> HealthCheckResult:
        """Perform readiness check (can accept traffic).

        This checks all critical dependencies (database, models, etc.).
        Kubernetes uses this to determine if the pod should receive traffic.
        """
        now = datetime.now(timezone.utc)
        uptime = monotonic() - self._start_time

        components: Dict[str, ComponentHealth] = {}

        # Check database connectivity
        db_health = await self._check_database()
        components["database"] = db_health

        # Check LLM model availability
        model_health = await self._check_models()
        components["models"] = model_health

        # Determine overall status
        statuses = [comp.status for comp in components.values()]
        if any(s == ComponentStatus.DOWN for s in statuses):
            overall_status = HealthStatus.UNHEALTHY
        elif any(s == ComponentStatus.DEGRADED for s in statuses):
            overall_status = HealthStatus.DEGRADED
        else:
            overall_status = HealthStatus.HEALTHY

        return HealthCheckResult(
            status=overall_status,
            timestamp=now,
            uptime_seconds=uptime,
            components=components,
        )

    async def _check_database(self) -> ComponentHealth:
        """Check database connectivity and responsiveness."""
        start = monotonic()

        try:
            # Simple query to verify database is responsive
            async with asyncio.timeout(self.database_timeout):
                # Backend-agnostic: query a known table directly
                # If the table doesn't exist, fetch_one raises — schema missing
                # If the table is empty, fetch_one returns None — that's fine
                try:
                    await self.db.fetch_one(
                        "SELECT 1 FROM requests LIMIT 1"
                    )
                except Exception as schema_exc:
                    # Check if this is a missing-table error vs connection error
                    err = str(schema_exc).lower()
                    if any(p in err for p in ("no such table", "does not exist", "relation")):
                        return ComponentHealth(
                            status=ComponentStatus.DOWN,
                            message="Database schema not initialized",
                            latency_ms=(monotonic() - start) * 1000,
                            last_check=datetime.now(timezone.utc),
                        )
                    raise  # Re-raise connection errors for outer handler

                latency_ms = (monotonic() - start) * 1000

                # Check if latency is concerning
                if latency_ms > 1000:  # >1s is degraded
                    status = ComponentStatus.DEGRADED
                    message = f"Database responding slowly ({latency_ms:.1f}ms)"
                else:
                    status = ComponentStatus.UP
                    message = "Database operational"

                return ComponentHealth(
                    status=status,
                    message=message,
                    latency_ms=latency_ms,
                    last_check=datetime.now(timezone.utc),
                )

        except asyncio.TimeoutError:
            return ComponentHealth(
                status=ComponentStatus.DOWN,
                message=f"Database timeout (>{self.database_timeout}s)",
                latency_ms=(monotonic() - start) * 1000,
                last_check=datetime.now(timezone.utc),
            )
        except Exception as exc:
            return ComponentHealth(
                status=ComponentStatus.DOWN,
                message=f"Database error: {str(exc)}",
                latency_ms=(monotonic() - start) * 1000,
                last_check=datetime.now(timezone.utc),
            )

    async def _check_models(self) -> ComponentHealth:
        """Check LLM provider availability via provider-owned health_check()."""
        start = monotonic()

        try:
            async with asyncio.timeout(self.model_timeout):
                is_healthy = await self.llm_provider.health_check()

            latency_ms = (monotonic() - start) * 1000
            if is_healthy:
                return ComponentHealth(
                    status=ComponentStatus.UP,
                    message="LLM provider operational",
                    latency_ms=latency_ms,
                    last_check=datetime.now(timezone.utc),
                )

            return ComponentHealth(
                status=ComponentStatus.DOWN,
                message="LLM provider health check failed",
                latency_ms=latency_ms,
                last_check=datetime.now(timezone.utc),
            )

        except asyncio.TimeoutError:
            return ComponentHealth(
                status=ComponentStatus.DOWN,
                message=f"LLM provider timeout (>{self.model_timeout}s)",
                latency_ms=(monotonic() - start) * 1000,
                last_check=datetime.now(timezone.utc),
            )
        except Exception as exc:
            return ComponentHealth(
                status=ComponentStatus.DOWN,
                message=f"LLM provider error: {str(exc)[:80]}",
                latency_ms=(monotonic() - start) * 1000,
                last_check=datetime.now(timezone.utc),
            )


def health_check_to_dict(result: HealthCheckResult) -> Dict[str, Any]:
    """Convert HealthCheckResult to dictionary for API response."""
    return {
        "status": result.status.value,
        "timestamp": result.timestamp.isoformat(),
        "uptime_seconds": result.uptime_seconds,
        "version": result.version,
        "components": {
            name: {
                "status": comp.status.value,
                "message": comp.message,
                "latency_ms": comp.latency_ms,
                "last_check": comp.last_check.isoformat() if comp.last_check else None,
            }
            for name, comp in result.components.items()
        },
    }
