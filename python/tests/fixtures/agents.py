"""Agent and context fixtures for tests.

Usage:
    @pytest.fixture
    def my_test(agent_context_factory):
        ctx = agent_context_factory(raw_input="test query")
"""

import pytest
from typing import Callable, Dict, Any
from jeeves_core.protocols import AgentContext, RequestContext
from jeeves_core.testing.helpers import make_agent_context
from tests.fixtures.mocks import (
    MockDatabaseClient,
    MockLLMAdapter,
)


# =============================================================================
# FACTORY FIXTURES
# =============================================================================

@pytest.fixture
def agent_context_factory() -> Callable[..., AgentContext]:
    """Factory for creating test AgentContext instances.

    Usage:
        def test_something(agent_context_factory):
            ctx = agent_context_factory(raw_input="custom query")
    """
    def _create(
        raw_input: str = "test query",
        session_id: str = "test-session",
        user_id: str = "test-user",
        outputs: Dict[str, Dict[str, Any]] = None,
        metadata: Dict[str, Any] = None,
    ) -> AgentContext:
        return make_agent_context(
            message=raw_input,
            user_id=user_id,
            session_id=session_id,
            outputs=outputs,
            metadata=metadata,
        )
    return _create


@pytest.fixture
def sample_context(agent_context_factory) -> AgentContext:
    """Basic sample context for tests."""
    return agent_context_factory(raw_input="Where is the main function defined?")


# =============================================================================
# MOCK FIXTURES
# =============================================================================

@pytest.fixture
def mock_db() -> MockDatabaseClient:
    """Mock database client for isolated testing."""
    return MockDatabaseClient()


@pytest.fixture
def mock_tool_executor():
    """Mock tool executor for testing executor/traverser stages."""
    class MockToolExecutor:
        def __init__(self):
            self.executed_tools = []

        async def execute(self, tool_name: str, params: Dict[str, Any]) -> Dict:
            self.executed_tools.append((tool_name, params))
            return {"status": "success", "result": f"Executed {tool_name}"}

    return MockToolExecutor()


@pytest.fixture
def mock_llm_provider() -> MockLLMAdapter:
    """Mock LLM provider for isolated testing."""
    return MockLLMAdapter()


__all__ = [
    # Factory fixtures
    "agent_context_factory",
    "sample_context",
    # Mock fixtures
    "mock_db",
    "mock_tool_executor",
    "mock_llm_provider",
]
