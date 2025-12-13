"""Tests for Go bridge module.

These tests verify the Python-Go interoperability layer works correctly,
including fallback behavior when Go binaries are not available.
"""

import json
import pytest
from unittest.mock import patch, MagicMock
import subprocess

from jeeves_avionics.interop.go_bridge import (
    GoBridge,
    GoEnvelopeBridge,
    GoBridgeError,
    GoNotAvailableError,
    GoExecutionError,
    GoResult,
)


class TestGoBridge:
    """Tests for base GoBridge class."""

    def test_init(self):
        """Test bridge initialization."""
        bridge = GoBridge("go-envelope")
        assert bridge._binary_name == "go-envelope"
        assert bridge._timeout == 30.0

    def test_is_available_when_not_found(self):
        """Test is_available returns False when binary not found."""
        bridge = GoBridge("nonexistent-binary")
        assert bridge.is_available() is False

    @patch("shutil.which")
    def test_is_available_when_found(self, mock_which):
        """Test is_available returns True when binary found."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        bridge = GoBridge("go-envelope")
        assert bridge.is_available() is True

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_call_success(self, mock_which, mock_run):
        """Test successful call to Go binary."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout='{"envelope_id": "env_123", "user_id": "u1"}',
            stderr="",
        )

        bridge = GoBridge("go-envelope")
        result = bridge.call("create", {"raw_input": "hello"})

        assert result.success is True
        assert result.data["envelope_id"] == "env_123"

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_call_error_response(self, mock_which, mock_run):
        """Test handling of error response from Go binary."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout='{"error": true, "code": "parse_error", "message": "Invalid JSON"}',
            stderr="",
        )

        bridge = GoBridge("go-envelope")
        result = bridge.call("process", {})

        assert result.success is False
        assert result.error_code == "parse_error"
        assert result.error_message == "Invalid JSON"

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_call_timeout(self, mock_which, mock_run):
        """Test timeout handling."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.side_effect = subprocess.TimeoutExpired(cmd="go-envelope", timeout=30)

        bridge = GoBridge("go-envelope", timeout_seconds=30)

        with pytest.raises(GoExecutionError) as exc_info:
            bridge.call("process", {})

        assert exc_info.value.code == "timeout"

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_call_exit_error(self, mock_which, mock_run):
        """Test handling of non-zero exit code."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=1,
            stdout="",
            stderr="panic: something went wrong",
        )

        bridge = GoBridge("go-envelope")

        with pytest.raises(GoExecutionError) as exc_info:
            bridge.call("process", {})

        assert exc_info.value.code == "exit_error"


class TestGoEnvelopeBridge:
    """Tests for GoEnvelopeBridge class."""

    def test_init_with_python_fallback(self):
        """Test initialization with Python fallback."""
        bridge = GoEnvelopeBridge(use_go=False)
        assert bridge._use_go is False

    def test_create_envelope_python_fallback(self):
        """Test create_envelope with Python fallback."""
        bridge = GoEnvelopeBridge(use_go=False)

        result = bridge.create_envelope(
            raw_input="Hello world",
            user_id="user123",
            session_id="sess456",
        )

        assert "envelope_id" in result
        assert result["raw_input"] == "Hello world"
        assert result["user_id"] == "user123"
        assert result["session_id"] == "sess456"

    def test_can_continue_python_fallback(self):
        """Test can_continue with Python fallback."""
        bridge = GoEnvelopeBridge(use_go=False)

        # Create envelope first
        envelope = bridge.create_envelope(
            raw_input="Test",
            user_id="u1",
            session_id="s1",
        )

        result = bridge.can_continue(envelope)

        assert result["can_continue"] is True
        assert result["iteration"] == 0
        assert result["llm_call_count"] == 0

    def test_get_result_python_fallback(self):
        """Test get_result with Python fallback."""
        bridge = GoEnvelopeBridge(use_go=False)

        envelope = bridge.create_envelope(
            raw_input="Test",
            user_id="u1",
            session_id="s1",
        )

        result = bridge.get_result(envelope)

        assert "envelope_id" in result
        assert "request_id" in result
        assert result["terminated"] is False

    def test_validate_python_fallback(self):
        """Test validate with Python fallback."""
        bridge = GoEnvelopeBridge(use_go=False)

        result = bridge.validate({"envelope_id": "env_123", "user_id": "u1"})

        assert result["valid"] is True
        assert result["errors"] == []

    def test_validate_missing_fields(self):
        """Test validate with missing fields."""
        bridge = GoEnvelopeBridge(use_go=False)

        result = bridge.validate({})

        assert result["valid"] is False
        assert len(result["errors"]) > 0

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_create_envelope_go_success(self, mock_which, mock_run):
        """Test create_envelope using Go binary."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout=json.dumps({
                "envelope_id": "env_go_123",
                "raw_input": "Hello",
                "user_id": "u1",
                "session_id": "s1",
            }),
            stderr="",
        )

        bridge = GoEnvelopeBridge(use_go=True)
        result = bridge.create_envelope(
            raw_input="Hello",
            user_id="u1",
            session_id="s1",
        )

        assert result["envelope_id"] == "env_go_123"

    @patch("shutil.which")
    def test_auto_detect_go_not_available(self, mock_which):
        """Test auto-detection when Go not available."""
        mock_which.return_value = None

        bridge = GoEnvelopeBridge(use_go=None)

        # Force initialization
        _ = bridge.using_go

        assert bridge._use_go is False

    @patch("shutil.which")
    def test_auto_detect_go_available(self, mock_which):
        """Test auto-detection when Go available."""
        mock_which.return_value = "/usr/local/bin/go-envelope"

        bridge = GoEnvelopeBridge(use_go=None)

        # Force initialization
        _ = bridge.using_go

        assert bridge._use_go is True


class TestGoResult:
    """Tests for GoResult dataclass."""

    def test_success_result(self):
        """Test successful result."""
        result = GoResult(
            success=True,
            data={"envelope_id": "env_123"},
        )
        assert result.success is True
        assert result.error_code is None

    def test_error_result(self):
        """Test error result."""
        result = GoResult(
            success=False,
            data={},
            error_code="parse_error",
            error_message="Invalid JSON",
        )
        assert result.success is False
        assert result.error_code == "parse_error"


class TestIntegration:
    """Integration tests that run if Go binary is available."""

    @pytest.fixture
    def bridge(self):
        """Create bridge for testing."""
        return GoEnvelopeBridge()

    def test_roundtrip_envelope(self, bridge):
        """Test full envelope roundtrip."""
        # This test works whether Go is available or not (uses fallback)
        envelope = bridge.create_envelope(
            raw_input="What is this codebase about?",
            user_id="integration_test_user",
            session_id="integration_test_session",
        )

        assert envelope["raw_input"] == "What is this codebase about?"

        # Check can continue
        can_continue = bridge.can_continue(envelope)
        assert can_continue["can_continue"] is True

        # Get result
        result = bridge.get_result(envelope)
        assert result["envelope_id"] == envelope["envelope_id"]
