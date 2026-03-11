"""A2A Client — Invoke remote A2A agents as tool executors.

Pattern A (Temporal coherence): Remote agent invocation is wrapped as a
tool executor, NOT as a new kernel instruction type. Local kernel remains
sole termination authority. Remote agent runs its own orchestration with
its own bounds. Results serialize back as tool output → envelope.outputs
→ routable.

Usage:
    client = A2AClient(base_url="http://remote-agent:8000/a2a")
    result = await client.send_task("Hello, analyze this code", skill="code_analysis")

    # As a tool executor (Pattern A)
    async def invoke_remote_agent(agent_url: str, task: str, skill: str = "") -> dict:
        client = A2AClient(base_url=agent_url)
        return await client.send_task(task, skill=skill)
"""

from __future__ import annotations

import logging
import uuid
from typing import Any, Dict, List, Optional

import httpx

logger = logging.getLogger(__name__)


class A2AClient:
    """Client for invoking remote A2A agents.

    Wraps HTTP calls to A2A-compatible endpoints.
    Designed to be used as a tool executor backend (Pattern A).
    """

    def __init__(
        self,
        base_url: str,
        timeout: float = 120.0,
        headers: Optional[Dict[str, str]] = None,
    ):
        self._base_url = base_url.rstrip("/")
        self._timeout = timeout
        self._headers = headers or {}
        self._agent_card: Optional[Dict[str, Any]] = None

    async def get_agent_card(self) -> Dict[str, Any]:
        """Fetch the remote agent's Agent Card.

        Returns the A2A agent card from /.well-known/agent.json.
        """
        if self._agent_card:
            return self._agent_card

        async with httpx.AsyncClient(timeout=self._timeout) as client:
            # Agent card is at the root, not under /a2a
            base = self._base_url.replace("/a2a", "")
            response = await client.get(
                f"{base}/.well-known/agent.json",
                headers=self._headers,
            )
            response.raise_for_status()
            self._agent_card = response.json()
            return self._agent_card

    async def send_task(
        self,
        input_text: str,
        *,
        task_id: Optional[str] = None,
        session_id: Optional[str] = None,
        skill: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Send a task to the remote A2A agent.

        This is the primary method for Pattern A (A2A as tool).
        Returns the task result as a dict suitable for envelope.outputs.

        Args:
            input_text: The task input text
            task_id: Optional task ID (generated if not provided)
            session_id: Optional session ID
            skill: Optional skill/pipeline name
            metadata: Optional additional metadata

        Returns:
            {"task_id": str, "status": str, "artifacts": [...], "output": str}
        """
        task_id = task_id or str(uuid.uuid4())

        payload: Dict[str, Any] = {
            "id": task_id,
            "message": {
                "role": "user",
                "parts": [{"type": "text", "text": input_text}],
            },
        }

        if session_id:
            payload["sessionId"] = session_id
        if skill:
            payload["skill"] = skill
        if metadata:
            payload["metadata"] = metadata

        async with httpx.AsyncClient(timeout=self._timeout) as client:
            response = await client.post(
                f"{self._base_url}/tasks/send",
                json=payload,
                headers={**self._headers, "Content-Type": "application/json"},
            )
            response.raise_for_status()
            result = response.json()

        # Extract output text from artifacts for easy consumption
        output_text = ""
        artifacts = result.get("artifacts", [])
        for artifact in artifacts:
            for part in artifact.get("parts", []):
                if part.get("type") == "text":
                    output_text += part.get("text", "")

        return {
            "task_id": result.get("id", task_id),
            "status": result.get("status", {}).get("state", "unknown"),
            "artifacts": artifacts,
            "output": output_text,
        }

    async def get_task(self, task_id: str) -> Dict[str, Any]:
        """Get status of a remote task.

        Args:
            task_id: The task ID to query

        Returns:
            A2A task status response
        """
        async with httpx.AsyncClient(timeout=self._timeout) as client:
            response = await client.get(
                f"{self._base_url}/tasks/{task_id}",
                headers=self._headers,
            )
            response.raise_for_status()
            return response.json()

    async def cancel_task(self, task_id: str) -> Dict[str, Any]:
        """Cancel a remote task.

        Args:
            task_id: The task ID to cancel

        Returns:
            A2A cancellation response
        """
        async with httpx.AsyncClient(timeout=self._timeout) as client:
            response = await client.post(
                f"{self._base_url}/tasks/{task_id}/cancel",
                headers=self._headers,
            )
            response.raise_for_status()
            return response.json()

    async def list_skills(self) -> List[Dict[str, Any]]:
        """List available skills from the remote agent's Agent Card."""
        card = await self.get_agent_card()
        return card.get("skills", [])
