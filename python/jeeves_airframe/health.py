"""Health check types and utilities for production monitoring.

Provides data types and conversion helpers for /health and /ready endpoints.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from enum import Enum
from typing import Any, Dict, Optional

from jeeves_airframe.protocols import HealthStatus
from jeeves_airframe.config.constants import PLATFORM_VERSION


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
