"""Tests for Go bridge module.

Go binary is REQUIRED. These tests mock the binary for unit testing.
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

    def test_init_raises_when_not_found(self):
        """Test initialization raises when binary not found."""
        with pytest.raises(GoNotAvailableError) as exc_info:
            GoBridge("nonexistent-binary")
        assert "not found in PATH" in str(exc_info.value)

    @patch("shutil.which")
    def test_init_succeeds_when_found(self, mock_which):
        """Test initialization succeeds when binary found."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        bridge = GoBridge("go-envelope")
        assert bridge._binary_path == "/usr/local/bin/go-envelope"

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

    @patch("shutil.which")
    def test_init_raises_when_not_available(self, mock_which):
        """Test initialization raises when binary not available."""
        mock_which.return_value = None
        with pytest.raises(GoNotAvailableError):
            GoEnvelopeBridge()

    @patch("shutil.which")
    def test_init_succeeds_when_available(self, mock_which):
        """Test initialization succeeds when binary available."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        bridge = GoEnvelopeBridge()
        assert bridge._binary_path == "/usr/local/bin/go-envelope"

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_create_envelope_success(self, mock_which, mock_run):
        """Test create_envelope success."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout=json.dumps({
                "envelope_id": "env_123",
                "raw_input": "Hello",
                "user_id": "u1",
                "session_id": "s1",
            }),
            stderr="",
        )

        bridge = GoEnvelopeBridge()
        result = bridge.create_envelope(
            raw_input="Hello",
            user_id="u1",
            session_id="s1",
        )

        assert result["envelope_id"] == "env_123"
        assert result["raw_input"] == "Hello"

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_create_envelope_error_raises(self, mock_which, mock_run):
        """Test create_envelope raises on error."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout='{"error": true, "code": "validation", "message": "Invalid input"}',
            stderr="",
        )

        bridge = GoEnvelopeBridge()
        with pytest.raises(GoExecutionError) as exc_info:
            bridge.create_envelope(
                raw_input="Hello",
                user_id="u1",
                session_id="s1",
            )
        assert "Invalid input" in str(exc_info.value)

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_can_continue_success(self, mock_which, mock_run):
        """Test can_continue success."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout=json.dumps({
                "can_continue": True,
                "iteration": 0,
                "llm_call_count": 0,
            }),
            stderr="",
        )

        bridge = GoEnvelopeBridge()
        result = bridge.can_continue({"envelope_id": "env_123"})

        assert result["can_continue"] is True

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_get_result_success(self, mock_which, mock_run):
        """Test get_result success."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout=json.dumps({
                "envelope_id": "env_123",
                "terminated": False,
            }),
            stderr="",
        )

        bridge = GoEnvelopeBridge()
        result = bridge.get_result({"envelope_id": "env_123"})

        assert result["envelope_id"] == "env_123"

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_validate_success(self, mock_which, mock_run):
        """Test validate success."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout=json.dumps({
                "valid": True,
                "errors": [],
            }),
            stderr="",
        )

        bridge = GoEnvelopeBridge()
        result = bridge.validate({"envelope_id": "env_123", "user_id": "u1"})

        assert result["valid"] is True
        assert result["errors"] == []


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
