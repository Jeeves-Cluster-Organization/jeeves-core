"""LLM fixtures for avionics tests.

Provides mock LLM providers for testing without network calls.
"""

import json
import pytest
from typing import Any, Dict, List, Optional
from unittest.mock import AsyncMock


class MockLLMProvider:
    """Mock LLM provider for testing.

    Provides canned responses based on prompt content patterns.
    """

    def __init__(self, responses: Optional[Dict[str, str]] = None):
        self.responses = responses or {}
        self.call_count = 0
        self.calls: List[Dict[str, Any]] = []

    async def generate(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> str:
        """Generate a mock response."""
        self.call_count += 1
        self.calls.append({
            "model": model,
            "prompt": prompt,
            "options": options,
        })

        # Check for pattern matches in responses
        for pattern, response in self.responses.items():
            if pattern.lower() in prompt.lower():
                return response

        # Default response based on prompt content
        if "plan" in prompt.lower():
            return json.dumps({
                "steps": [
                    {"tool": "grep_search", "params": {"pattern": "test"}},
                ],
                "confidence": 0.9,
            })
        elif "intent" in prompt.lower():
            return json.dumps({
                "intent": "analyze",
                "goals": ["understand code"],
                "confidence": 0.9,
            })
        else:
            return json.dumps({"result": "mock response"})

    async def generate_structured(
        self,
        model: str,
        prompt: str,
        schema: Dict[str, Any],
        options: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Generate a structured mock response."""
        self.call_count += 1
        return {"structured": "response"}

    async def health_check(self) -> bool:
        """Always returns True for mock."""
        return True


@pytest.fixture
def mock_llm_provider():
    """Create a mock LLM provider."""
    return MockLLMProvider()


@pytest.fixture
def mock_llm_provider_factory():
    """Factory for creating mock LLM providers with custom responses.

    Usage:
        def test_something(mock_llm_provider_factory):
            provider = mock_llm_provider_factory({
                "find auth": '{"result": "AuthHandler in auth.py"}'
            })
    """
    def _create(responses: Optional[Dict[str, str]] = None) -> MockLLMProvider:
        return MockLLMProvider(responses)
    return _create
