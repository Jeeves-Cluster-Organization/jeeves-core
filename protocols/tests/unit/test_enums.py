"""Unit tests for core types and enums.

Tests RiskLevel, ToolCategory, HealthStatus, TerminalReason, and other core types.
"""

import pytest


class TestRiskLevel:
    """Tests for RiskLevel enum."""

    def test_risk_level_values(self):
        """Test RiskLevel enum values exist."""
        from jeeves_core.types import RiskLevel

        # Semantic levels
        assert RiskLevel.READ_ONLY is not None
        assert RiskLevel.WRITE is not None
        assert RiskLevel.DESTRUCTIVE is not None
        # Severity levels
        assert RiskLevel.LOW is not None
        assert RiskLevel.MEDIUM is not None
        assert RiskLevel.HIGH is not None
        assert RiskLevel.CRITICAL is not None

    def test_risk_level_comparison(self):
        """Test RiskLevel enum comparison."""
        from jeeves_core.types import RiskLevel

        # Enums should be comparable
        assert RiskLevel.READ_ONLY != RiskLevel.HIGH
        assert RiskLevel.LOW != RiskLevel.CRITICAL

    def test_requires_confirmation(self):
        """Test requires_confirmation class method."""
        from jeeves_core.types import RiskLevel

        assert RiskLevel.requires_confirmation(RiskLevel.DESTRUCTIVE) is True
        assert RiskLevel.requires_confirmation(RiskLevel.HIGH) is True
        assert RiskLevel.requires_confirmation(RiskLevel.CRITICAL) is True
        assert RiskLevel.requires_confirmation(RiskLevel.LOW) is False


class TestToolCategory:
    """Tests for ToolCategory enum."""

    def test_tool_category_values(self):
        """Test ToolCategory enum values exist."""
        from jeeves_core.types import ToolCategory

        # Operation types
        assert ToolCategory.READ is not None
        assert ToolCategory.WRITE is not None
        assert ToolCategory.EXECUTE is not None
        assert ToolCategory.NETWORK is not None
        assert ToolCategory.SYSTEM is not None
        # Tool organization
        assert ToolCategory.UNIFIED is not None
        assert ToolCategory.COMPOSITE is not None
        assert ToolCategory.STANDALONE is not None


class TestHealthStatus:
    """Tests for HealthStatus enum."""

    def test_health_status_values(self):
        """Test HealthStatus enum values exist."""
        from jeeves_core.types import HealthStatus

        assert HealthStatus.HEALTHY is not None
        assert HealthStatus.DEGRADED is not None
        assert HealthStatus.UNHEALTHY is not None


class TestTerminalReason:
    """Tests for TerminalReason enum."""

    def test_terminal_reason_values(self):
        """Test TerminalReason enum values exist."""
        from jeeves_core.types import TerminalReason

        assert TerminalReason.COMPLETED is not None
        assert TerminalReason.MAX_ITERATIONS_EXCEEDED is not None
        assert TerminalReason.MAX_LLM_CALLS_EXCEEDED is not None
        assert TerminalReason.MAX_AGENT_HOPS_EXCEEDED is not None
        assert TerminalReason.USER_CANCELLED is not None
        assert TerminalReason.TOOL_FAILED_FATALLY is not None
        assert TerminalReason.POLICY_VIOLATION is not None


class TestLoopVerdict:
    """Tests for LoopVerdict enum."""

    def test_loop_verdict_values(self):
        """Test LoopVerdict enum values exist."""
        from jeeves_core.types import LoopVerdict

        assert LoopVerdict.PROCEED is not None
        assert LoopVerdict.LOOP_BACK is not None
        assert LoopVerdict.ADVANCE is not None
        assert LoopVerdict.ESCALATE is not None


class TestRiskApproval:
    """Tests for RiskApproval enum."""

    def test_risk_approval_values(self):
        """Test RiskApproval enum values exist."""
        from jeeves_core.types import RiskApproval

        assert RiskApproval.APPROVED is not None
        assert RiskApproval.DENIED is not None
        assert RiskApproval.PENDING is not None


class TestToolAccess:
    """Tests for ToolAccess enum."""

    def test_tool_access_values(self):
        """Test ToolAccess enum values exist."""
        from jeeves_core.types import ToolAccess

        assert ToolAccess.NONE is not None
        assert ToolAccess.READ is not None
        assert ToolAccess.WRITE is not None
        assert ToolAccess.ALL is not None


class TestOperationStatus:
    """Tests for OperationStatus enum."""

    def test_operation_status_values(self):
        """Test OperationStatus enum values exist."""
        from jeeves_core.types import OperationStatus

        assert OperationStatus.SUCCESS is not None
        assert OperationStatus.ERROR is not None
        assert OperationStatus.NOT_FOUND is not None
        assert OperationStatus.TIMEOUT is not None
        assert OperationStatus.VALIDATION_ERROR is not None


class TestOperationResult:
    """Tests for OperationResult dataclass."""

    def test_operation_result_success(self):
        """Test creating success result."""
        from jeeves_core.types import OperationResult

        result = OperationResult.success({"key": "value"})

        assert result.is_success is True
        assert result.is_error is False
        assert result.data == {"key": "value"}

    def test_operation_result_error(self):
        """Test creating error result."""
        from jeeves_core.types import OperationResult

        result = OperationResult.error("Something went wrong", "validation")

        assert result.is_success is False
        assert result.is_error is True
        assert result.error == "Something went wrong"
        assert result.error_type == "validation"

    def test_operation_result_not_found(self):
        """Test creating not found result."""
        from jeeves_core.types import OperationResult

        result = OperationResult.not_found("File not found")

        assert result.is_success is False
        assert result.error == "File not found"
        assert result.error_type == "not_found"

    def test_operation_result_to_dict(self):
        """Test converting result to dictionary."""
        from jeeves_core.types import OperationResult

        result = OperationResult.success({"items": [1, 2, 3]})
        data = result.to_dict()

        assert data["status"] == "success"
        assert data["data"] == {"items": [1, 2, 3]}
