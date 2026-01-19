"""Go bridge module for Python-Go interoperability.

Go binary is REQUIRED. No Python fallback.
If Go binary is not available, initialization fails immediately.
"""

import json
import shutil
import subprocess
from dataclasses import dataclass, field
from typing import Any, Dict, Optional


class GoBridgeError(Exception):
    """Base exception for Go bridge errors."""
    pass


class GoNotAvailableError(GoBridgeError):
    """Raised when Go binary is required but not available."""
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
    """Bridge to Go binary. Fails if binary not available."""

    def __init__(self, binary_name: str, timeout_seconds: float = 30.0):
        """Initialize Go bridge.

        Args:
            binary_name: Name of the Go binary
            timeout_seconds: Timeout for binary execution
            
        Raises:
            GoNotAvailableError: If binary not found in PATH
        """
        self._binary_name = binary_name
        self._timeout = timeout_seconds
        self._binary_path = shutil.which(binary_name)
        
        if self._binary_path is None:
            raise GoNotAvailableError(
                f"Go binary '{binary_name}' not found in PATH. "
                f"Build with: go build -o {binary_name} cmd/envelope/main.go"
            )

    def call(self, command: str, args: Dict[str, Any]) -> GoResult:
        """Call the Go binary with a command.

        Args:
            command: Command to execute
            args: Arguments as a dictionary

        Returns:
            GoResult with success status and data/error

        Raises:
            GoExecutionError: If execution fails
        """
        try:
            input_data = json.dumps({"command": command, **args})

            result = subprocess.run(
                [self._binary_path],
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
    """Bridge to Go envelope operations. Go binary is REQUIRED."""

    def __init__(self):
        """Initialize envelope bridge.
        
        Raises:
            GoNotAvailableError: If go-envelope binary not available
        """
        super().__init__("go-envelope")

    def create_envelope(
        self,
        raw_input: str,
        user_id: str,
        session_id: str,
        **kwargs: Any,
    ) -> Dict[str, Any]:
        """Create a new envelope via Go.

        Args:
            raw_input: Raw user input
            user_id: User identifier
            session_id: Session identifier
            **kwargs: Additional envelope fields

        Returns:
            Created envelope as dictionary
            
        Raises:
            GoExecutionError: If Go execution fails
        """
        result = self.call(
            "create",
            {
                "raw_input": raw_input,
                "user_id": user_id,
                "session_id": session_id,
                **kwargs,
            },
        )
        if not result.success:
            raise GoExecutionError(
                result.error_message or "create_envelope failed",
                code=result.error_code or "unknown"
            )
        return result.data

    def can_continue(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
        """Check if envelope processing can continue.

        Args:
            envelope: Current envelope state

        Returns:
            Dictionary with can_continue status and metrics
            
        Raises:
            GoExecutionError: If Go execution fails
        """
        result = self.call("can_continue", {"envelope": envelope})
        if not result.success:
            raise GoExecutionError(
                result.error_message or "can_continue failed",
                code=result.error_code or "unknown"
            )
        return result.data

    def get_result(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
        """Get result from envelope.

        Args:
            envelope: Current envelope state

        Returns:
            Result dictionary
            
        Raises:
            GoExecutionError: If Go execution fails
        """
        result = self.call("get_result", {"envelope": envelope})
        if not result.success:
            raise GoExecutionError(
                result.error_message or "get_result failed",
                code=result.error_code or "unknown"
            )
        return result.data

    def validate(self, envelope: Dict[str, Any]) -> Dict[str, Any]:
        """Validate an envelope.

        Args:
            envelope: Envelope to validate

        Returns:
            Validation result with valid status and errors
            
        Raises:
            GoExecutionError: If Go execution fails
        """
        result = self.call("validate", {"envelope": envelope})
        if not result.success:
            raise GoExecutionError(
                result.error_message or "validate failed",
                code=result.error_code or "unknown"
            )
        return result.data
