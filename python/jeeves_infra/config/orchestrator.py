"""Orchestrator configuration — cyclic flow and context bounds.

Per Amendment V: No modes. The cyclic flow is the only flow.
"""

from dataclasses import dataclass


@dataclass
class CyclicConfig:
    """Configuration for the cyclic orchestrator.

    - max_llm_calls: Hard limit on LLM invocations per request
    - max_iterations: Hard limit on plan-execute-draft-critique cycles
    - critic_confidence_threshold: Minimum confidence for Critic to honor retry/clarify actions
    - max_validator_retries: Max Critic → Validator retries per request
    - max_planner_retries: Max Critic → Planner retries per request

    No mode enum. The cyclic flow is the only flow.
    """
    max_llm_calls: int = 10
    max_iterations: int = 3
    critic_confidence_threshold: float = 0.6
    max_validator_retries: int = 1
    max_planner_retries: int = 1


@dataclass
class ContextBoundsConfig:
    """Configuration for context window bounds.

    These bounds prevent context explosion by limiting how much
    data from each memory layer can be included in LLM prompts.

    Designed for local models with ~4K-16K context windows.
    """
    # L1: Task Context
    max_task_context_chars: int = 2000

    # L3: Semantic Memory
    max_semantic_snippets: int = 5
    max_semantic_chars_per_snippet: int = 400
    max_semantic_chars_total: int = 1500

    # L4: Working Memory (conversation history)
    max_open_loops: int = 5
    max_conversation_turns: int = 10
    max_conversation_chars: int = 3000

    # L5: Graph Context
    max_graph_relationships: int = 4

    # Execution History
    max_prior_plans: int = 2
    max_prior_tool_results: int = 10

    # Total Budget
    max_total_context_chars: int = 10000


# Global instances
cyclic_config = CyclicConfig()
context_bounds = ContextBoundsConfig()


def get_context_bounds() -> ContextBoundsConfig:
    """Get the global context bounds configuration."""
    return context_bounds
