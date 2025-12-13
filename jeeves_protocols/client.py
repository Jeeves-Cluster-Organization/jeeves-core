"""Go Client - JSON-over-stdio interface to Go runtime.

This is the bridge between Python and Go. All envelope operations
go through Go via subprocess with JSON stdin/stdout.

Usage:
    from jeeves_protocols import GoClient, create_envelope

    # Using module-level functions (recommended)
    envelope = create_envelope("Hello", "user1", "session1")
    can_continue = check_bounds(envelope)
    result = get_result(envelope)

    # Or using client instance
    client = GoClient()
    envelope = client.create("Hello", "user1", "session1")
"""

import json
import os
import shutil
import subprocess
from dataclasses import dataclass
from typing import Any, Dict, List, Optional

from jeeves_protocols.envelope import GenericEnvelope


class GoClientError(Exception):
    """Error from Go client."""
    def __init__(self, message: str, code: Optional[str] = None):
        super().__init__(message)
        self.code = code


class GoNotFoundError(GoClientError):
    """Go binary not found."""
    pass


@dataclass
class BoundsResult:
    """Result from bounds check."""
    can_continue: bool
    terminal_reason: Optional[str]
    iteration: int
    llm_call_count: int
    agent_hop_count: int


class GoClient:
    """Client for calling Go runtime via subprocess.

    The Go binary (go-envelope) is called with JSON on stdin
    and returns JSON on stdout. This is the Unix philosophy
    approach - simple, debuggable, composable.

    Binary search order:
    1. GO_BIN_PATH environment variable
    2. PATH lookup
    3. /app/bin (Docker)
    4. ./go/bin (local dev)
    """

    BINARY_NAME = "go-envelope"
    DEFAULT_PATHS = ["/app/bin", "/usr/local/bin", "./go/bin", "./bin"]

    def __init__(self, binary_path: Optional[str] = None, timeout: float = 30.0):
        self._binary_path = binary_path
        self._timeout = timeout
        self._resolved_path: Optional[str] = None

    @property
    def binary_path(self) -> str:
        """Get resolved binary path."""
        if self._resolved_path is None:
            self._resolved_path = self._find_binary()
        return self._resolved_path

    def _find_binary(self) -> str:
        """Find the Go binary."""
        # Explicit path
        if self._binary_path:
            if os.path.isfile(self._binary_path) and os.access(self._binary_path, os.X_OK):
                return self._binary_path
            raise GoNotFoundError(f"Binary not found at: {self._binary_path}")

        # Environment variable
        env_path = os.getenv("GO_BIN_PATH")
        if env_path:
            full_path = os.path.join(env_path, self.BINARY_NAME)
            if os.path.isfile(full_path) and os.access(full_path, os.X_OK):
                return full_path

        # PATH lookup
        which_result = shutil.which(self.BINARY_NAME)
        if which_result:
            return which_result

        # Default paths
        for bin_dir in self.DEFAULT_PATHS:
            full_path = os.path.join(bin_dir, self.BINARY_NAME)
            if os.path.isfile(full_path) and os.access(full_path, os.X_OK):
                return full_path

        raise GoNotFoundError(
            f"Go binary '{self.BINARY_NAME}' not found. "
            f"Set GO_BIN_PATH or ensure it's in PATH."
        )

    def is_available(self) -> bool:
        """Check if Go binary is available."""
        try:
            _ = self.binary_path
            return True
        except GoNotFoundError:
            return False

    def _call(self, command: str, input_data: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """Call Go binary with command and input."""
        binary = self.binary_path
        input_json = json.dumps(input_data) if input_data else "{}"

        try:
            result = subprocess.run(
                [binary, command],
                input=input_json,
                capture_output=True,
                text=True,
                timeout=self._timeout,
            )

            if result.returncode != 0:
                # Try to parse error from stdout
                try:
                    error_data = json.loads(result.stdout)
                    if error_data.get("error"):
                        raise GoClientError(
                            error_data.get("message", "Unknown error"),
                            code=error_data.get("code"),
                        )
                except json.JSONDecodeError:
                    pass
                raise GoClientError(
                    f"Go binary exited with code {result.returncode}: {result.stderr}",
                    code="exit_error",
                )

            output = json.loads(result.stdout)

            if output.get("error"):
                raise GoClientError(
                    output.get("message", "Unknown error"),
                    code=output.get("code"),
                )

            return output

        except subprocess.TimeoutExpired:
            raise GoClientError(f"Go binary timed out after {self._timeout}s", code="timeout")
        except json.JSONDecodeError as e:
            raise GoClientError(f"Invalid JSON from Go: {e}", code="json_error")
        except FileNotFoundError:
            raise GoNotFoundError(f"Go binary not found: {binary}")

    def create(
        self,
        raw_input: str,
        user_id: str,
        session_id: str,
        request_id: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
        stage_order: Optional[List[str]] = None,
    ) -> GenericEnvelope:
        """Create a new envelope via Go."""
        input_data = {
            "raw_input": raw_input,
            "user_id": user_id,
            "session_id": session_id,
        }
        if request_id:
            input_data["request_id"] = request_id
        if metadata:
            input_data["metadata"] = metadata
        if stage_order:
            input_data["stage_order"] = stage_order

        result = self._call("create", input_data)
        return GenericEnvelope.from_dict(result)

    def can_continue(self, envelope: GenericEnvelope) -> BoundsResult:
        """Check if envelope can continue processing."""
        result = self._call("can-continue", envelope.to_dict())
        return BoundsResult(
            can_continue=result.get("can_continue", False),
            terminal_reason=result.get("terminal_reason"),
            iteration=result.get("iteration", 0),
            llm_call_count=result.get("llm_call_count", 0),
            agent_hop_count=result.get("agent_hop_count", 0),
        )

    def get_result(self, envelope: GenericEnvelope) -> Dict[str, Any]:
        """Get envelope result dictionary."""
        return self._call("result", envelope.to_dict())

    def validate(self, envelope: GenericEnvelope) -> Dict[str, Any]:
        """Validate envelope structure."""
        return self._call("validate", envelope.to_dict())

    def process(self, envelope: GenericEnvelope) -> GenericEnvelope:
        """Process envelope through Go (validate and transform)."""
        result = self._call("process", envelope.to_dict())
        return GenericEnvelope.from_dict(result)

    def version(self) -> Dict[str, str]:
        """Get Go binary version info."""
        return self._call("version")


# =============================================================================
# MODULE-LEVEL CONVENIENCE FUNCTIONS
# =============================================================================

_default_client: Optional[GoClient] = None


def _get_client() -> GoClient:
    """Get default client instance."""
    global _default_client
    if _default_client is None:
        _default_client = GoClient()
    return _default_client


def create_envelope(
    raw_input: str,
    user_id: str,
    session_id: str,
    request_id: Optional[str] = None,
    metadata: Optional[Dict[str, Any]] = None,
    stage_order: Optional[List[str]] = None,
) -> GenericEnvelope:
    """Create envelope via Go runtime."""
    return _get_client().create(
        raw_input, user_id, session_id, request_id, metadata, stage_order
    )


def check_bounds(envelope: GenericEnvelope) -> BoundsResult:
    """Check envelope bounds via Go runtime."""
    return _get_client().can_continue(envelope)


def get_result(envelope: GenericEnvelope) -> Dict[str, Any]:
    """Get envelope result via Go runtime."""
    return _get_client().get_result(envelope)


def is_go_available() -> bool:
    """Check if Go runtime is available."""
    return _get_client().is_available()
