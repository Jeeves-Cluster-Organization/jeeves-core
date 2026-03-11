"""A2A (Agent-to-Agent) Protocol — Server and Client adapters.

Server: Expose jeeves pipelines as A2A-compatible agents.
Client: Invoke remote A2A agents as tool executors (Pattern A).

Temporal coherence: A2A agents are wrapped as tools. Local kernel
remains sole termination authority. Remote agent runs its own
orchestration with its own bounds.
"""

from .server import a2a_router
from .client import A2AClient

__all__ = ["a2a_router", "A2AClient"]
