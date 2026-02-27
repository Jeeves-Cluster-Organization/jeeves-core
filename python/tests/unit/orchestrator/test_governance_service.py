"""Tests for GovernanceService / HealthServicer."""

import pytest
from unittest.mock import AsyncMock, MagicMock

from jeeves_core.orchestrator.governance_service import (
    HealthServicer,
    ServiceError,
    get_agent_definitions,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_tool_health_service(summary: dict | None = None):
    """Build a mock ToolHealthService."""
    svc = AsyncMock()
    if summary is None:
        summary = {
            "tools": {
                "search": {"status": "healthy", "success_rate": 0.99, "last_used": None},
                "write": {"status": "degraded", "success_rate": 0.75, "last_used": None},
            }
        }
    svc.get_health_summary = AsyncMock(return_value=summary)
    svc.get_tool_health = AsyncMock(return_value={
        "tool_name": "search",
        "status": "healthy",
        "success_rate": 0.99,
        "total_calls": 100,
        "failed_calls": 1,
    })
    return svc


def _make_kernel_client(status_result=None):
    """Build a mock kernel client."""
    client = AsyncMock()
    if status_result is None:
        status_result = MagicMock(
            processes_total=10,
            processes_by_state={"RUNNING": 5, "READY": 3, "TERMINATED": 2},
            services_healthy=3,
            services_degraded=0,
            services_unhealthy=0,
            active_orchestration_sessions=2,
            commbus_events_published=100,
            commbus_commands_sent=50,
            commbus_queries_executed=20,
            commbus_active_subscribers=5,
        )
    client.get_system_status = AsyncMock(return_value=status_result)
    return client


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestGetHealthSummary:
    async def test_returns_health_with_tool_breakdown(self):
        svc = HealthServicer(
            tool_health_service=_make_tool_health_service(),
        )
        result = await svc.get_health_summary()
        assert result["total_tools"] == 2
        assert result["healthy_tools"] == 1
        assert result["degraded_tools"] == 1
        assert result["overall_status"] == "degraded"

    async def test_all_healthy(self):
        svc = HealthServicer(
            tool_health_service=_make_tool_health_service({
                "tools": {
                    "a": {"status": "healthy", "success_rate": 1.0},
                    "b": {"status": "healthy", "success_rate": 0.98},
                }
            }),
        )
        result = await svc.get_health_summary()
        assert result["overall_status"] == "healthy"

    async def test_unhealthy_takes_precedence(self):
        svc = HealthServicer(
            tool_health_service=_make_tool_health_service({
                "tools": {
                    "a": {"status": "unhealthy", "success_rate": 0.0},
                    "b": {"status": "degraded", "success_rate": 0.5},
                }
            }),
        )
        result = await svc.get_health_summary()
        assert result["overall_status"] == "unhealthy"

    async def test_error_raises_service_error(self):
        tool_svc = AsyncMock()
        tool_svc.get_health_summary = AsyncMock(side_effect=RuntimeError("db down"))
        svc = HealthServicer(tool_health_service=tool_svc)
        with pytest.raises(ServiceError) as exc_info:
            await svc.get_health_summary()
        assert exc_info.value.code == "INTERNAL"


class TestGetSystemStatus:
    async def test_returns_kernel_status(self):
        svc = HealthServicer(
            tool_health_service=_make_tool_health_service(),
            kernel_client=_make_kernel_client(),
        )
        result = await svc.get_system_status()
        assert result["processes"]["total"] == 10
        assert result["services"]["healthy"] == 3
        assert result["orchestration"]["active_sessions"] == 2
        assert result["commbus"]["events_published"] == 100

    async def test_no_kernel_raises_service_error(self):
        svc = HealthServicer(
            tool_health_service=_make_tool_health_service(),
            kernel_client=None,
        )
        with pytest.raises(ServiceError) as exc_info:
            await svc.get_system_status()
        assert exc_info.value.code == "UNAVAILABLE"

    async def test_kernel_failure_raises_service_error(self):
        client = AsyncMock()
        client.get_system_status = AsyncMock(side_effect=ConnectionError("refused"))
        svc = HealthServicer(
            tool_health_service=_make_tool_health_service(),
            kernel_client=client,
        )
        with pytest.raises(ServiceError) as exc_info:
            await svc.get_system_status()
        assert exc_info.value.code == "INTERNAL"
