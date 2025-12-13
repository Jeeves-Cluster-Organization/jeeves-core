"""Unit tests for ToolHealthService.

Tests for L7 System Introspection tool health monitoring including:
- Metric recording (success, error, timeout)
- Health status determination
- Performance degradation detection
- Circuit breaker logic
- Error pattern analysis
- Dashboard summaries
"""

import pytest
from datetime import datetime, timezone, timedelta
from unittest.mock import AsyncMock, MagicMock

from jeeves_memory_module.services.tool_health_service import (
    ToolHealthService,
    ToolHealthReport,
    SystemHealthReport,
    HealthStatus
)
from jeeves_memory_module.repositories.tool_metrics_repository import (
    ToolMetricsRepository,
    ToolMetric
)

# Requires PostgreSQL database for metrics repository
pytestmark = pytest.mark.requires_postgres


class TestService:
    """Tests for ToolHealthService main functionality."""

    @pytest.fixture
    def mock_db(self):
        """Create mock database client."""
        db = AsyncMock()
        db.execute = AsyncMock()
        db.fetch_one = AsyncMock()
        db.fetch_all = AsyncMock()
        return db

    @pytest.fixture
    def mock_repository(self):
        """Create mock tool metrics repository."""
        repo = AsyncMock(spec=ToolMetricsRepository)
        repo.ensure_table = AsyncMock()
        repo.record = AsyncMock()
        repo.get_tool_stats = AsyncMock()
        repo.get_all_tool_names = AsyncMock()
        repo.get_recent_errors = AsyncMock()
        repo.get_slow_executions = AsyncMock()
        return repo

    @pytest.fixture
    def service(self, mock_db, mock_repository):
        """Create service with mocked dependencies."""
        return ToolHealthService(db=mock_db, repository=mock_repository)

    @pytest.fixture
    def healthy_stats(self):
        """Stats for a healthy tool (>95% success, <2s latency)."""
        return {
            "tool_name": "add_task",
            "total_calls": 100,
            "success_count": 98,
            "error_count": 2,
            "timeout_count": 0,
            "success_rate": 0.98,
            "avg_time_ms": 500
        }

    @pytest.fixture
    def degraded_stats(self):
        """Stats for a degraded tool (80-95% success or 2-5s latency)."""
        return {
            "tool_name": "search_tasks",
            "total_calls": 100,
            "success_count": 85,
            "error_count": 15,
            "timeout_count": 0,
            "success_rate": 0.85,
            "avg_time_ms": 3500
        }

    @pytest.fixture
    def unhealthy_stats(self):
        """Stats for an unhealthy tool (<80% success or >5s latency)."""
        return {
            "tool_name": "memory_search",
            "total_calls": 50,
            "success_count": 30,
            "error_count": 15,
            "timeout_count": 5,
            "success_rate": 0.60,
            "avg_time_ms": 6000
        }

    @pytest.fixture
    def insufficient_data_stats(self):
        """Stats with insufficient data for assessment."""
        return {
            "tool_name": "rare_tool",
            "total_calls": 3,
            "success_count": 3,
            "error_count": 0,
            "timeout_count": 0,
            "success_rate": 1.0,
            "avg_time_ms": 100
        }

    # ========== Initialization Tests ==========

    @pytest.mark.asyncio
    async def test_ensure_initialized_calls_repository(self, service, mock_repository):
        """Test that ensure_initialized creates the table."""
        await service.ensure_initialized()
        mock_repository.ensure_table.assert_called_once()

    # ========== Record Execution Tests ==========

    @pytest.mark.asyncio
    async def test_record_execution_success(self, service, mock_repository):
        """Test recording a successful tool execution."""
        expected_metric = ToolMetric(
            metric_id="m-1",
            tool_name="add_task",
            user_id="user-123",
            status="success",
            execution_time_ms=250
        )
        mock_repository.record.return_value = expected_metric

        result = await service.record_execution(
            tool_name="add_task",
            user_id="user-123",
            status="success",
            execution_time_ms=250
        )

        mock_repository.record.assert_called_once()
        recorded = mock_repository.record.call_args[0][0]
        assert recorded.tool_name == "add_task"
        assert recorded.status == "success"
        assert recorded.execution_time_ms == 250

    @pytest.mark.asyncio
    async def test_record_execution_error(self, service, mock_repository):
        """Test recording a tool error."""
        expected_metric = ToolMetric(
            metric_id="m-1",
            tool_name="search_tasks",
            user_id="user-123",
            status="error",
            error_type="DatabaseError",
            error_message="Connection failed"
        )
        mock_repository.record.return_value = expected_metric

        result = await service.record_execution(
            tool_name="search_tasks",
            user_id="user-123",
            status="error",
            execution_time_ms=1000,
            error_type="DatabaseError",
            error_message="Connection failed"
        )

        recorded = mock_repository.record.call_args[0][0]
        assert recorded.status == "error"
        assert recorded.error_type == "DatabaseError"
        assert recorded.error_message == "Connection failed"

    @pytest.mark.asyncio
    async def test_record_execution_timeout(self, service, mock_repository):
        """Test recording a tool timeout."""
        mock_repository.record.return_value = ToolMetric(
            tool_name="slow_tool",
            user_id="user-123",
            status="timeout",
            execution_time_ms=30000
        )

        await service.record_execution(
            tool_name="slow_tool",
            user_id="user-123",
            status="timeout",
            execution_time_ms=30000
        )

        recorded = mock_repository.record.call_args[0][0]
        assert recorded.status == "timeout"

    @pytest.mark.asyncio
    async def test_record_execution_with_all_fields(self, service, mock_repository):
        """Test recording with all optional fields."""
        mock_repository.record.return_value = ToolMetric(
            tool_name="add_task",
            user_id="user-123",
            session_id="sess-1",
            request_id="req-1",
            status="success",
            execution_time_ms=200,
            metadata={"items_created": 1}
        )

        await service.record_execution(
            tool_name="add_task",
            user_id="user-123",
            session_id="sess-1",
            request_id="req-1",
            status="success",
            execution_time_ms=200,
            metadata={"items_created": 1}
        )

        recorded = mock_repository.record.call_args[0][0]
        assert recorded.session_id == "sess-1"
        assert recorded.request_id == "req-1"
        assert recorded.metadata == {"items_created": 1}

    # ========== Health Check Tests ==========

    @pytest.mark.asyncio
    async def test_check_tool_health_healthy(
        self, service, mock_repository, healthy_stats
    ):
        """Test health check returns HEALTHY for good tools."""
        mock_repository.get_tool_stats.return_value = healthy_stats

        report = await service.check_tool_health("add_task")

        assert report.status == HealthStatus.HEALTHY
        assert report.tool_name == "add_task"
        assert report.success_rate == 0.98
        assert len(report.issues) == 0
        assert len(report.recommendations) == 0

    @pytest.mark.asyncio
    async def test_check_tool_health_degraded_by_success_rate(
        self, service, mock_repository, degraded_stats
    ):
        """Test health check returns DEGRADED for moderate issues."""
        mock_repository.get_tool_stats.return_value = degraded_stats

        report = await service.check_tool_health("search_tasks")

        assert report.status == HealthStatus.DEGRADED
        assert len(report.issues) > 0
        assert any("success rate" in issue.lower() for issue in report.issues)
        assert len(report.recommendations) > 0

    @pytest.mark.asyncio
    async def test_check_tool_health_degraded_by_latency(
        self, service, mock_repository
    ):
        """Test health check returns DEGRADED for high latency."""
        stats = {
            "tool_name": "slow_tool",
            "total_calls": 100,
            "success_count": 100,
            "error_count": 0,
            "success_rate": 1.0,
            "avg_time_ms": 3500  # Between 2000ms and 5000ms
        }
        mock_repository.get_tool_stats.return_value = stats

        report = await service.check_tool_health("slow_tool")

        assert report.status == HealthStatus.DEGRADED
        assert any("latency" in issue.lower() for issue in report.issues)

    @pytest.mark.asyncio
    async def test_check_tool_health_unhealthy(
        self, service, mock_repository, unhealthy_stats
    ):
        """Test health check returns UNHEALTHY for critical issues."""
        mock_repository.get_tool_stats.return_value = unhealthy_stats

        report = await service.check_tool_health("memory_search")

        assert report.status == HealthStatus.UNHEALTHY
        assert any("critical" in issue.lower() for issue in report.issues)
        assert any("urgent" in rec.lower() for rec in report.recommendations)

    @pytest.mark.asyncio
    async def test_check_tool_health_unknown_insufficient_data(
        self, service, mock_repository, insufficient_data_stats
    ):
        """Test health check returns UNKNOWN with insufficient data."""
        mock_repository.get_tool_stats.return_value = insufficient_data_stats

        report = await service.check_tool_health("rare_tool")

        assert report.status == HealthStatus.UNKNOWN
        assert any("insufficient" in issue.lower() for issue in report.issues)

    @pytest.mark.asyncio
    async def test_check_tool_health_custom_period(self, service, mock_repository):
        """Test health check with custom time period."""
        mock_repository.get_tool_stats.return_value = {
            "total_calls": 50,
            "success_count": 48,
            "error_count": 2,
            "success_rate": 0.96,
            "avg_time_ms": 300
        }

        await service.check_tool_health("add_task", period_hours=24)

        # Verify stats were requested with correct time window
        mock_repository.get_tool_stats.assert_called_once()
        call_args = mock_repository.get_tool_stats.call_args
        since_arg = call_args.kwargs['since']  # Using keyword args, not positional
        # Should be approximately 24 hours ago
        expected_since = datetime.now(timezone.utc) - timedelta(hours=24)
        assert abs((since_arg - expected_since).total_seconds()) < 60

    # ========== System Health Check Tests ==========

    @pytest.mark.asyncio
    async def test_check_all_tools_health(
        self, service, mock_repository, healthy_stats, degraded_stats
    ):
        """Test checking health of all tools."""
        mock_repository.get_all_tool_names.return_value = ["add_task", "search_tasks"]
        mock_repository.get_tool_stats.side_effect = [healthy_stats, degraded_stats]

        report = await service.check_all_tools_health()

        assert isinstance(report, SystemHealthReport)
        assert len(report.tool_reports) == 2
        assert report.summary["healthy"] == 1
        assert report.summary["degraded"] == 1

    @pytest.mark.asyncio
    async def test_check_all_tools_overall_status_unhealthy(
        self, service, mock_repository, healthy_stats, unhealthy_stats
    ):
        """Test overall status is UNHEALTHY if any tool is unhealthy."""
        mock_repository.get_all_tool_names.return_value = ["add_task", "memory_search"]
        mock_repository.get_tool_stats.side_effect = [healthy_stats, unhealthy_stats]

        report = await service.check_all_tools_health()

        assert report.status == HealthStatus.UNHEALTHY

    @pytest.mark.asyncio
    async def test_check_all_tools_overall_status_degraded(
        self, service, mock_repository, healthy_stats, degraded_stats
    ):
        """Test overall status is DEGRADED if any tool is degraded."""
        mock_repository.get_all_tool_names.return_value = ["add_task", "search_tasks"]
        mock_repository.get_tool_stats.side_effect = [healthy_stats, degraded_stats]

        report = await service.check_all_tools_health()

        assert report.status == HealthStatus.DEGRADED

    @pytest.mark.asyncio
    async def test_check_all_tools_overall_status_healthy(
        self, service, mock_repository, healthy_stats
    ):
        """Test overall status is HEALTHY if all tools are healthy."""
        mock_repository.get_all_tool_names.return_value = ["add_task", "get_tasks"]
        mock_repository.get_tool_stats.side_effect = [healthy_stats, healthy_stats]

        report = await service.check_all_tools_health()

        assert report.status == HealthStatus.HEALTHY

    @pytest.mark.asyncio
    async def test_check_all_tools_no_tools(self, service, mock_repository):
        """Test system health with no tools."""
        mock_repository.get_all_tool_names.return_value = []

        report = await service.check_all_tools_health()

        assert report.status == HealthStatus.UNKNOWN
        assert len(report.tool_reports) == 0

    # ========== Circuit Breaker Tests ==========

    @pytest.mark.asyncio
    async def test_should_circuit_break_true_when_errors_exceed_threshold(
        self, service, mock_repository
    ):
        """Test circuit breaker triggers when errors exceed threshold."""
        mock_repository.get_tool_stats.return_value = {
            "error_count": 5,
            "timeout_count": 2
        }

        result = await service.should_circuit_break(
            "failing_tool",
            error_threshold=5
        )

        assert result is True

    @pytest.mark.asyncio
    async def test_should_circuit_break_false_below_threshold(
        self, service, mock_repository
    ):
        """Test circuit breaker doesn't trigger below threshold."""
        mock_repository.get_tool_stats.return_value = {
            "error_count": 2,
            "timeout_count": 1
        }

        result = await service.should_circuit_break(
            "mostly_ok_tool",
            error_threshold=5
        )

        assert result is False

    @pytest.mark.asyncio
    async def test_should_circuit_break_counts_timeouts(
        self, service, mock_repository
    ):
        """Test circuit breaker counts timeouts as errors."""
        mock_repository.get_tool_stats.return_value = {
            "error_count": 2,
            "timeout_count": 4  # 2 + 4 = 6 >= 5
        }

        result = await service.should_circuit_break(
            "timeout_tool",
            error_threshold=5
        )

        assert result is True

    @pytest.mark.asyncio
    async def test_should_circuit_break_custom_window(
        self, service, mock_repository
    ):
        """Test circuit breaker with custom time window."""
        mock_repository.get_tool_stats.return_value = {
            "error_count": 10,
            "timeout_count": 0
        }

        await service.should_circuit_break(
            "tool",
            error_threshold=5,
            window_minutes=10
        )

        # Verify time window was respected
        call_args = mock_repository.get_tool_stats.call_args
        since_arg = call_args.kwargs['since']  # Using keyword args, not positional
        expected_since = datetime.now(timezone.utc) - timedelta(minutes=10)
        assert abs((since_arg - expected_since).total_seconds()) < 60

    @pytest.mark.asyncio
    async def test_should_circuit_break_handles_none_values(
        self, service, mock_repository
    ):
        """Test circuit breaker handles None values in stats."""
        mock_repository.get_tool_stats.return_value = {
            "error_count": None,
            "timeout_count": None
        }

        result = await service.should_circuit_break("tool")

        # Should not crash, should return False (0 + 0 < threshold)
        assert result is False

    # ========== Error Pattern Tests ==========

    @pytest.mark.asyncio
    async def test_get_error_patterns_groups_by_type(
        self, service, mock_repository
    ):
        """Test error pattern analysis groups errors by type."""
        mock_repository.get_recent_errors.return_value = [
            ToolMetric(tool_name="tool", error_type="DatabaseError", error_message="msg1"),
            ToolMetric(tool_name="tool", error_type="DatabaseError", error_message="msg2"),
            ToolMetric(tool_name="tool", error_type="TimeoutError", error_message="msg3"),
        ]

        result = await service.get_error_patterns("tool")

        assert result["tool_name"] == "tool"
        assert result["total_errors"] == 3
        assert len(result["patterns"]) == 2
        # DatabaseError should be first (most frequent)
        assert result["patterns"][0]["error_type"] == "DatabaseError"
        assert result["patterns"][0]["count"] == 2

    @pytest.mark.asyncio
    async def test_get_error_patterns_empty_when_no_errors(
        self, service, mock_repository
    ):
        """Test error patterns returns empty when no errors."""
        mock_repository.get_recent_errors.return_value = []

        result = await service.get_error_patterns("tool")

        assert result["total_errors"] == 0
        assert result["patterns"] == []

    @pytest.mark.asyncio
    async def test_get_error_patterns_includes_recent_messages(
        self, service, mock_repository
    ):
        """Test error patterns includes recent error messages."""
        mock_repository.get_recent_errors.return_value = [
            ToolMetric(tool_name="tool", error_type="Error", error_message="First error"),
            ToolMetric(tool_name="tool", error_type="Error", error_message="Second error"),
        ]

        result = await service.get_error_patterns("tool")

        assert "recent_messages" in result
        assert "First error" in result["recent_messages"]

    # ========== Dashboard Summary Tests ==========

    @pytest.mark.asyncio
    async def test_get_dashboard_summary(
        self, service, mock_repository, healthy_stats, degraded_stats
    ):
        """Test dashboard summary generation."""
        mock_repository.get_all_tool_names.return_value = ["add_task", "search_tasks"]
        mock_repository.get_tool_stats.side_effect = [healthy_stats, degraded_stats]
        mock_repository.get_slow_executions.return_value = []

        result = await service.get_dashboard_summary()

        assert "overall_status" in result
        assert "summary" in result
        assert "tools_needing_attention" in result
        assert "slow_executions" in result
        assert "checked_at" in result

    @pytest.mark.asyncio
    async def test_get_dashboard_summary_includes_degraded_tools(
        self, service, mock_repository, healthy_stats, degraded_stats
    ):
        """Test dashboard includes tools needing attention."""
        mock_repository.get_all_tool_names.return_value = ["add_task", "search_tasks"]
        mock_repository.get_tool_stats.side_effect = [healthy_stats, degraded_stats]
        mock_repository.get_slow_executions.return_value = []

        result = await service.get_dashboard_summary()

        # Only degraded tool should be in attention list
        assert len(result["tools_needing_attention"]) == 1
        assert result["tools_needing_attention"][0]["tool_name"] == "search_tasks"

    @pytest.mark.asyncio
    async def test_get_dashboard_summary_includes_slow_executions(
        self, service, mock_repository, healthy_stats
    ):
        """Test dashboard includes slow executions."""
        mock_repository.get_all_tool_names.return_value = ["add_task"]
        mock_repository.get_tool_stats.return_value = healthy_stats
        mock_repository.get_slow_executions.return_value = [
            ToolMetric(
                tool_name="slow_tool",
                execution_time_ms=6000,
                recorded_at=datetime.now(timezone.utc)
            )
        ]

        result = await service.get_dashboard_summary()

        assert len(result["slow_executions"]) == 1
        assert result["slow_executions"][0]["tool_name"] == "slow_tool"
        assert result["slow_executions"][0]["execution_time_ms"] == 6000


class TestStatusEnum:
    """Tests for HealthStatus enum values."""

    def test_all_statuses_defined(self):
        """Test all expected statuses exist."""
        assert HealthStatus.HEALTHY.value == "healthy"
        assert HealthStatus.DEGRADED.value == "degraded"
        assert HealthStatus.UNHEALTHY.value == "unhealthy"
        assert HealthStatus.UNKNOWN.value == "unknown"


class TestHealthReport:
    """Tests for ToolHealthReport serialization."""

    def test_to_dict_serialization(self):
        """Test ToolHealthReport serialization."""
        now = datetime.now(timezone.utc)
        report = ToolHealthReport(
            tool_name="add_task",
            status=HealthStatus.HEALTHY,
            success_rate=0.98,
            avg_latency_ms=250.5,
            total_calls=100,
            recent_errors=2,
            issues=[],
            recommendations=[],
            checked_at=now
        )

        result = report.to_dict()

        assert result["tool_name"] == "add_task"
        assert result["status"] == "healthy"
        assert result["success_rate"] == 0.98
        assert result["avg_latency_ms"] == 250.5
        assert result["total_calls"] == 100
        assert result["checked_at"] == now.isoformat()

    def test_to_dict_includes_issues(self):
        """Test ToolHealthReport includes issues in serialization."""
        report = ToolHealthReport(
            tool_name="failing_tool",
            status=HealthStatus.UNHEALTHY,
            success_rate=0.5,
            avg_latency_ms=1000,
            total_calls=20,
            recent_errors=10,
            issues=["Success rate critically low"],
            recommendations=["Investigate failures"],
            checked_at=datetime.now(timezone.utc)
        )

        result = report.to_dict()

        assert result["issues"] == ["Success rate critically low"]
        assert result["recommendations"] == ["Investigate failures"]


class TestSystemReport:
    """Tests for SystemHealthReport serialization."""

    def test_to_dict_serialization(self):
        """Test SystemHealthReport serialization."""
        now = datetime.now(timezone.utc)
        tool_report = ToolHealthReport(
            tool_name="add_task",
            status=HealthStatus.HEALTHY,
            success_rate=0.98,
            avg_latency_ms=250,
            total_calls=100,
            recent_errors=2,
            issues=[],
            recommendations=[],
            checked_at=now
        )
        report = SystemHealthReport(
            status=HealthStatus.HEALTHY,
            tool_reports=[tool_report],
            summary={"healthy": 1, "degraded": 0, "unhealthy": 0, "unknown": 0},
            checked_at=now
        )

        result = report.to_dict()

        assert result["status"] == "healthy"
        assert len(result["tool_reports"]) == 1
        assert result["summary"]["healthy"] == 1


class TestMetric:
    """Tests for ToolMetric dataclass."""

    def test_to_dict_serialization(self):
        """Test ToolMetric serialization."""
        now = datetime.now(timezone.utc)
        metric = ToolMetric(
            metric_id="m-1",
            tool_name="add_task",
            user_id="user-123",
            session_id="sess-1",
            request_id="req-1",
            status="success",
            execution_time_ms=250,
            recorded_at=now
        )

        result = metric.to_dict()

        assert result["metric_id"] == "m-1"
        assert result["tool_name"] == "add_task"
        assert result["status"] == "success"
        assert result["recorded_at"] == now.isoformat()

    def test_from_dict_deserialization(self):
        """Test ToolMetric deserialization."""
        now = datetime.now(timezone.utc)
        data = {
            "metric_id": "m-1",
            "tool_name": "search_tasks",
            "user_id": "user-1",
            "status": "error",
            "execution_time_ms": 1000,
            "error_type": "DatabaseError",
            "error_message": "Connection timeout",
            "recorded_at": now.isoformat()
        }

        metric = ToolMetric.from_dict(data)

        assert metric.metric_id == "m-1"
        assert metric.status == "error"
        assert metric.error_type == "DatabaseError"
        assert metric.recorded_at == now


class TestThresholds:
    """Tests for health threshold constants."""

    def test_success_rate_thresholds(self):
        """Test success rate threshold values."""
        assert ToolHealthService.SUCCESS_RATE_HEALTHY == 0.95
        assert ToolHealthService.SUCCESS_RATE_DEGRADED == 0.80

    def test_latency_thresholds(self):
        """Test latency threshold values."""
        assert ToolHealthService.LATENCY_HEALTHY_MS == 2000
        assert ToolHealthService.LATENCY_DEGRADED_MS == 5000

    def test_minimum_calls_for_assessment(self):
        """Test minimum calls threshold."""
        assert ToolHealthService.MIN_CALLS_FOR_ASSESSMENT == 5


class TestIntegration:
    """Integration-style tests for service behavior."""

    @pytest.fixture
    def mock_repository(self):
        """Create mock repository."""
        return AsyncMock(spec=ToolMetricsRepository)

    @pytest.fixture
    def service(self, mock_repository):
        """Create service."""
        return ToolHealthService(db=AsyncMock(), repository=mock_repository)

    @pytest.mark.asyncio
    async def test_health_report_provides_actionable_recommendations(
        self, service, mock_repository
    ):
        """Test health reports provide actionable recommendations."""
        mock_repository.get_tool_stats.return_value = {
            "total_calls": 100,
            "success_count": 70,
            "error_count": 30,
            "timeout_count": 0,
            "success_rate": 0.70,
            "avg_time_ms": 1000
        }

        report = await service.check_tool_health("failing_tool")

        # Should have at least one actionable recommendation
        assert len(report.recommendations) > 0
        # Recommendations should be actionable (contain verbs)
        action_words = ["investigate", "check", "consider", "review", "urgent"]
        has_action = any(
            any(word in rec.lower() for word in action_words)
            for rec in report.recommendations
        )
        assert has_action, "Recommendations should be actionable"

    @pytest.mark.asyncio
    async def test_combined_health_assessment_worst_wins(
        self, service, mock_repository
    ):
        """Test that combined health assessment uses worst status."""
        # Good success rate but bad latency
        mock_repository.get_tool_stats.return_value = {
            "total_calls": 100,
            "success_count": 98,
            "error_count": 2,
            "success_rate": 0.98,  # HEALTHY
            "avg_time_ms": 6000    # UNHEALTHY (>5000ms)
        }

        report = await service.check_tool_health("slow_but_reliable")

        # Should be UNHEALTHY (worst of HEALTHY and UNHEALTHY)
        assert report.status == HealthStatus.UNHEALTHY
