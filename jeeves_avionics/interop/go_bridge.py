"""Go bridge module for Python-Go interoperability.

This module provides a bridge to Go binaries for performance-critical
operations. When Go binaries are not available, it falls back to Python
implementations.
"""

import json
import shutil
import subprocess
import uuid
from dataclasses import dataclass, field
from typing import Any, Dict, Optional


class GoBridgeError(Exception):
    """Base exception for Go bridge errors."""
    pass


class GoNotAvailableError(GoBridgeError):
    """Raised when Go binary is not available."""
    pass


class GoExecutionError(GoBridgeError):
    """Raised when Go binary execution fails."""

    def __init__(self, message: str, code: str = "unknown"):
        super().__init__(message)
        self.code = code


@dataclass
class GoResult:
    """Result from a Go binary call."""
    success: bool
    data: Dict[str, Any] = field(default_factory=dict)
    error_code: Optional[str] = None
    error_message: Optional[str] = None


class GoBridge:
    """Base class for Go binary bridges."""

    def __init__(self, binary_name: str, timeout_seconds: float = 30.0):
        """Initialize Go bridge.

        Args:
            binary_name: Name of the Go binary
            timeout_seconds: Timeout for binary execution
        """
        self._binary_name = binary_name
        self._timeout = timeout_seconds
        self._binary_path: Optional[str] = None

    def is_available(self) -> bool:
        """Check if the Go binary is available.

        Returns:
            True if binary is found in PATH
        """
        return shutil.which(self._binary_name) is not None

    def call(self, command: str, args: Dict[str, Any]) -> GoResult:
        """Call the Go binary with a command.

        Args:
            command: Command to execute
            args: Arguments as a dictionary

        Returns:
            GoResult with success status and data/error

        Raises:
            GoNotAvailableError: If binary not found
            GoExecutionError: If execution fails
        """
        if not self.is_available():
            raise GoNotAvailableError(f"Binary {self._binary_name} not found")

        try:
            # Prepare input
            input_data = json.dumps({"command": command, **args})

            # Execute binary
            result = subprocess.run(
                [self._binary_name],
                input=input_data,
                capture_output=True,
                text=True,
                timeout=self._timeout,
            )

            if result.returncode != 0:
                raise GoExecutionError(
                    f"Binary exited with code {result.returncode}: {result.stderr}",
                    code="exit_error",
                )

            # Parse output
            output = json.loads(result.stdout)

            if output.get("error"):
                return GoResult(
                    success=False,
                    data=output,
                    error_code=output.get("code", "unknown"),
                    error_message=output.get("message", "Unknown error"),
                )

            return GoResult(success=True, data=output)

        except subprocess.TimeoutExpired:
            raise GoExecutionError(
                f"Binary {self._binary_name} timed out after {self._timeout}s",
                code="timeout",
            )
        except json.JSONDecodeError as e:
            raise GoExecutionError(f"Invalid JSON from binary: {e}", code="parse_error")


class GoEnvelopeBridge(GoBridge):
    """Bridge to Go envelope operations."""

    def __init__(self, use_go: Optional[bool] = None):
        """Initialize envelope bridge.

        Args:
            use_go: Force Go usage (True), Python fallback (False),
                   or auto-detect (None)
        """
        super().__init__("go-envelope")
        self._use_go = use_go
        self._initialized = False

    @property
    def using_go(self) -> bool:
        """Check if using Go implementation.

        Returns:
            True if using Go binary, False if using Python fallback
        """
        if not self._initialized:
            if self._use_go is None:
                self._use_go = self.is_available()
            self._initialized = True
        return self._use_go

    def create_envelope(
        self,
        raw_input: str,
        user_id: str,
        session_id: str,
        **kwargs: Any,
    ) -> Dict[str, Any]:
        """Create a new envelope.

        Args:
            raw_input: Raw user input
            user_id: User identifier
            session_id: Session identifier
            **kwargs: Additional envelope fields

        Returns:
            Created envelope as dictionary
        """
        if self.using_go:
            result = self.call(
                "create",
                {
                    "raw_input": raw_input,
                    "user_id": user_id,
                    "session_id": session_id,
                    **kwargs,
                },
            )
            return result.data

        # Python fallback
        return self._python_create_envelope(raw_input, user_id, session_id, **kwargs)

    def can_continue(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
        """Check if envelope processing can continue.

        Args:
            envelope: Current envelope state

        Returns:
            Dictionary with can_continue status and metrics
        """
        if self.using_go:
            result = self.call("can_continue", {"envelope": envelope})
            return result.data

        # Python fallback
        return self._python_can_continue(envelope)

    def get_result(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
        """Get result from envelope.

        Args:
            envelope: Current envelope state

        Returns:
            Result dictionary
        """
        if self.using_go:
            result = self.call("get_result", {"envelope": envelope})
            return result.data

        # Python fallback
        return self._python_get_result(envelope)

    def validate(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
        """Validate an envelope.

        Args:
            envelope: Envelope to validate

        Returns:
            Validation result with valid status and errors
        """
        if self.using_go:
            result = self.call("validate", {"envelope": envelope})
            return result.data

        # Python fallback
        return self._python_validate(envelope)

    # Python fallback implementations

    def _python_create_envelope(
        self,
        raw_input: str,
        user_id: str,
        session_id: str,
        **kwargs: Any,
    ) -> Dict[str, Any]:
        """Python fallback for create_envelope."""
        envelope_id = f"env_{uuid.uuid4().hex[:12]}"
        request_id = f"req_{uuid.uuid4().hex[:12]}"

        return {
            "envelope_id": envelope_id,
            "request_id": request_id,
            "raw_input": raw_input,
            "user_id": user_id,
            "session_id": session_id,
            "iteration": 0,
            "llm_call_count": 0,
            "terminated": False,
            "error": None,
            **kwargs,
        }

    def _python_can_continue(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
        """Python fallback for can_continue."""
        iteration = envelope.get("iteration", 0)
        llm_calls = envelope.get("llm_call_count", 0)
        terminated = envelope.get("terminated", False)

        # Default limits
        max_iterations = 10
        max_llm_calls = 20

        can_continue = (
            not terminated
            and iteration < max_iterations
            and llm_calls < max_llm_calls
        )

        return {
            "can_continue": can_continue,
            "iteration": iteration,
            "llm_call_count": llm_calls,
            "max_iterations": max_iterations,
            "max_llm_calls": max_llm_calls,
            "terminated": terminated,
        }

    def _python_get_result(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
        """Python fallback for get_result."""
        return {
            "envelope_id": envelope.get("envelope_id", ""),
            "request_id": envelope.get("request_id", ""),
            "final_response": envelope.get("final_response"),
            "terminated": envelope.get("terminated", False),
            "error": envelope.get("error"),
            "iteration": envelope.get("iteration", 0),
            "llm_call_count": envelope.get("llm_call_count", 0),
        }

    def _python_validate(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
        """Python fallback for validate."""
        errors = []

        # Required fields
        required = ["envelope_id", "user_id"]
        for field in required:
            if field not in envelope:
                errors.append(f"Missing required field: {field}")

        return {
            "valid": len(errors) == 0,
            "errors": errors,
        }
