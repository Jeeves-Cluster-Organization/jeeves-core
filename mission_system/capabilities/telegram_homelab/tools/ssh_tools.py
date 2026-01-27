"""SSH tools for executing commands on homelab servers."""

import asyncio
import logging
from dataclasses import dataclass
from typing import Any, Dict, List, Optional

from ..config import get_config

logger = logging.getLogger(__name__)


@dataclass
class ToolResult:
    """Result from tool execution."""

    status: str  # success, error, timeout
    data: Dict[str, Any]
    citations: List[Dict[str, str]]
    error_message: Optional[str] = None


class SSHExecutor:
    """SSH command executor with security boundaries."""

    def __init__(self):
        self.config = get_config().ssh

    def _validate_host(self, hostname: str) -> bool:
        """Validate that hostname is in allowed list."""
        if not self.config.hosts:
            logger.warning("No SSH hosts configured - rejecting all SSH requests")
            return False

        # Check exact match or wildcard
        for allowed_host in self.config.hosts:
            if hostname == allowed_host:
                return True
            if allowed_host.endswith("*") and hostname.startswith(
                allowed_host[:-1]
            ):
                return True

        return False

    def _build_ssh_command(
        self, hostname: str, command: str, user: Optional[str] = None
    ) -> List[str]:
        """Build SSH command with security parameters."""
        ssh_user = user or self.config.default_user
        ssh_cmd = ["ssh"]

        # Add identity file if configured
        if self.config.private_key_path:
            ssh_cmd.extend(["-i", self.config.private_key_path])

        # Add known hosts if configured
        if self.config.known_hosts_path:
            ssh_cmd.extend(["-o", f"UserKnownHostsFile={self.config.known_hosts_path}"])

        # Strict host key checking
        if self.config.strict_host_key_checking:
            ssh_cmd.extend(["-o", "StrictHostKeyChecking=yes"])
        else:
            ssh_cmd.extend(["-o", "StrictHostKeyChecking=no"])

        # Disable pseudo-terminal allocation
        ssh_cmd.append("-T")

        # Connection timeout
        ssh_cmd.extend(["-o", f"ConnectTimeout={self.config.timeout_seconds}"])

        # Target
        ssh_cmd.append(f"{ssh_user}@{hostname}")

        # Command
        ssh_cmd.append(command)

        return ssh_cmd

    async def execute_command(
        self, hostname: str, command: str, user: Optional[str] = None, timeout: Optional[int] = None
    ) -> ToolResult:
        """
        Execute SSH command on remote host.

        Args:
            hostname: Target hostname (must be in whitelist)
            command: Shell command to execute
            user: Optional SSH user (defaults to config.default_user)
            timeout: Optional timeout in seconds (defaults to config.timeout_seconds)

        Returns:
            ToolResult with command output and citations
        """
        # Validate hostname
        if not self._validate_host(hostname):
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"Hostname '{hostname}' is not in the allowed SSH hosts list",
            )

        # Build SSH command
        ssh_cmd = self._build_ssh_command(hostname, command, user)
        cmd_timeout = timeout or self.config.timeout_seconds

        logger.info(f"Executing SSH command on {hostname}: {command}")

        try:
            # Execute command with timeout
            process = await asyncio.create_subprocess_exec(
                *ssh_cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )

            try:
                stdout, stderr = await asyncio.wait_for(
                    process.communicate(), timeout=cmd_timeout
                )
            except asyncio.TimeoutError:
                process.kill()
                await process.wait()
                return ToolResult(
                    status="timeout",
                    data={},
                    citations=[],
                    error_message=f"Command timed out after {cmd_timeout} seconds",
                )

            # Decode output
            stdout_text = stdout.decode("utf-8", errors="replace")
            stderr_text = stderr.decode("utf-8", errors="replace")

            # Enforce output size limit
            if len(stdout_text) > self.config.max_output_chars:
                stdout_text = (
                    stdout_text[: self.config.max_output_chars]
                    + f"\n\n... (output truncated at {self.config.max_output_chars} chars)"
                )

            if len(stderr_text) > self.config.max_output_chars:
                stderr_text = (
                    stderr_text[: self.config.max_output_chars]
                    + f"\n\n... (output truncated at {self.config.max_output_chars} chars)"
                )

            # Determine success
            exit_code = process.returncode
            status = "success" if exit_code == 0 else "error"

            return ToolResult(
                status=status,
                data={
                    "hostname": hostname,
                    "command": command,
                    "exit_code": exit_code,
                    "stdout": stdout_text,
                    "stderr": stderr_text,
                },
                citations=[
                    {
                        "type": "ssh_execution",
                        "hostname": hostname,
                        "command": command,
                        "exit_code": str(exit_code),
                    }
                ],
                error_message=stderr_text if status == "error" else None,
            )

        except FileNotFoundError:
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message="SSH client not found. Please install OpenSSH.",
            )
        except Exception as e:
            logger.exception(f"SSH execution failed for {hostname}")
            return ToolResult(
                status="error",
                data={},
                citations=[],
                error_message=f"SSH execution failed: {str(e)}",
            )


# Global executor instance
_ssh_executor: Optional[SSHExecutor] = None


def get_ssh_executor() -> SSHExecutor:
    """Get or create the global SSH executor instance."""
    global _ssh_executor
    if _ssh_executor is None:
        _ssh_executor = SSHExecutor()
    return _ssh_executor


async def ssh_execute(
    hostname: str, command: str, user: Optional[str] = None, timeout: Optional[int] = None
) -> ToolResult:
    """
    Execute SSH command on homelab server.

    This is the main tool function that will be registered with the capability.

    Args:
        hostname: Target hostname (must be in whitelist)
        command: Shell command to execute
        user: Optional SSH user
        timeout: Optional timeout in seconds

    Returns:
        ToolResult with command output
    """
    executor = get_ssh_executor()
    return await executor.execute_command(hostname, command, user, timeout)
