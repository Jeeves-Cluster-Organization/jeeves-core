"""DeterministicAgent — base class for non-LLM stage executors.

Subclass and implement execute(). Register via stage(agent_class=MyAgent).
The framework bridges into Agent.process() via mock_handler — no Worker/IPC changes needed.
"""

from abc import ABC, abstractmethod
from typing import Any, Dict

from jeeves_core.protocols.types import AgentContext


class DeterministicAgent(ABC):
    """Base class for non-LLM stage executors.

    Subclass and implement execute(). Register via stage(agent_class=MyAgent).
    """

    @abstractmethod
    async def execute(self, context: AgentContext) -> Dict[str, Any]:
        """Produce this stage's output.

        Returns:
            Dict for envelope.outputs[stage_name].
        """
        ...
