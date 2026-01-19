"""Tests for Go bridge module.

These tests verify the Python-Go interoperability layer works correctly.
The bridge uses EXPLICIT mode - caller must choose Go or Python mode.
No auto-detection. No silent fallbacks.
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

    def test_require_available_raises_when_not_found(self):
        """Test require_available raises when binary not found."""
        bridge = GoBridge("nonexistent-binary")
        with pytest.raises(GoNotAvailableError) as exc_info:
            bridge.require_available()
        assert "not found in PATH" in str(exc_info.value)

    @patch("shutil.which")
    def test_require_available_succeeds_when_found(self, mock_which):
        """Test require_available succeeds when binary found."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        bridge = GoBridge("go-envelope")
        bridge.require_available()  # Should not raise

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

    def test_call_raises_when_not_available(self):
        """Test call raises GoNotAvailableError when binary not found."""
        bridge = GoBridge("nonexistent-binary")
        with pytest.raises(GoNotAvailableError):
            bridge.call("process", {})


class TestGoEnvelopeBridge:
    """Tests for GoEnvelopeBridge class."""

    def test_init_python_mode(self):
        """Test initialization in Python mode."""
        bridge = GoEnvelopeBridge(use_go=False)
        assert bridge._use_go is False
        assert bridge.using_go is False

    @patch("shutil.which")
    def test_init_go_mode_raises_when_not_available(self, mock_which):
        """Test initialization in Go mode raises when binary not available."""
        mock_which.return_value = None
        with pytest.raises(GoNotAvailableError):
            GoEnvelopeBridge(use_go=True)

    @patch("shutil.which")
    def test_init_go_mode_succeeds_when_available(self, mock_which):
        """Test initialization in Go mode succeeds when binary available."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        bridge = GoEnvelopeBridge(use_go=True)
        assert bridge._use_go is True
        assert bridge.using_go is True

    def test_create_envelope_python_mode(self):
        """Test create_envelope in Python mode."""
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

    def test_can_continue_python_mode(self):
        """Test can_continue in Python mode."""
        bridge = GoEnvelopeBridge(use_go=False)

        envelope = bridge.create_envelope(
            raw_input="Test",
            user_id="u1",
            session_id="s1",
        )

        result = bridge.can_continue(envelope)

        assert result["can_continue"] is True
        assert result["iteration"] == 0
        assert result["llm_call_count"] == 0

    def test_get_result_python_mode(self):
        """Test get_result in Python mode."""
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

    def test_validate_python_mode(self):
        """Test validate in Python mode."""
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
    def test_create_envelope_go_mode_success(self, mock_which, mock_run):
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

    @patch("subprocess.run")
    @patch("shutil.which")
    def test_create_envelope_go_mode_error_raises(self, mock_which, mock_run):
        """Test create_envelope in Go mode raises on error."""
        mock_which.return_value = "/usr/local/bin/go-envelope"
        mock_run.return_value = MagicMock(
            returncode=0,
            stdout='{"error": true, "code": "validation", "message": "Invalid input"}',
            stderr="",
        )

        bridge = GoEnvelopeBridge(use_go=True)
        with pytest.raises(GoExecutionError) as exc_info:
            bridge.create_envelope(
                raw_input="Hello",
                user_id="u1",
                session_id="s1",
            )
        assert "Invalid input" in str(exc_info.value)


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


class TestPythonModeIntegration:
    """Integration tests using Python mode (always available)."""

    @pytest.fixture
    def bridge(self):
        """Create bridge in Python mode for testing."""
        return GoEnvelopeBridge(use_go=False)

    def test_roundtrip_envelope(self, bridge):
        """Test full envelope roundtrip in Python mode."""
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

        # Validate
        validation = bridge.validate(envelope)
        assert validation["valid"] is True
