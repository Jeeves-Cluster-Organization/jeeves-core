"""Mock implementations for infrastructure dependencies.

These mocks allow mission system tests to run without requiring
actual database connections, LLM providers, or memory services.
"""

import json
import pytest
from typing import Any, Callable, Dict, List, Optional
from unittest.mock import AsyncMock, MagicMock


class MockLLMAdapter:
    """Mock LLM adapter for mission system tests.

    Provides canned responses based on prompt patterns.
    """

    def __init__(self, responses: Optional[Dict[str, str]] = None):
        self.responses = responses or {}
        self.call_count = 0
        self.calls: List[Dict[str, Any]] = []

    async def chat(
        self,
        model: str,
        messages: List[Dict[str, Any]],
        options: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        self.call_count += 1
        # Extract prompt text from messages for pattern matching
        prompt = " ".join(
            m.get("content", "") for m in messages if isinstance(m.get("content"), str)
        )
        self.calls.append({
            "messages": messages,
            "model": model,
            "options": options,
        })

        # Check for pattern matches
        for pattern, response in self.responses.items():
            if pattern.lower() in prompt.lower():
                return {"content": response, "tool_calls": []}

        # Default responses based on prompt content
        if "plan" in prompt.lower():
            return {"content": json.dumps({
                "steps": [{"tool": "grep_search", "params": {"pattern": "test"}}],
                "confidence": 0.9,
            }), "tool_calls": []}
        elif "intent" in prompt.lower():
            return {"content": json.dumps({
                "intent": "analyze",
                "goals": ["understand code"],
                "confidence": 0.9,
            }), "tool_calls": []}
        return {"content": '{"result": "mock response"}', "tool_calls": []}

    async def chat_with_usage(
        self,
        model: str,
        messages: List[Dict[str, Any]],
        options: Optional[Dict[str, Any]] = None,
    ) -> tuple:
        result = await self.chat(model, messages, options)
        return result, {"prompt_tokens": 10, "completion_tokens": 5}

    async def health_check(self) -> bool:
        return True


class MockDatabaseClient:
    """Mock database client for mission system tests.

    Provides in-memory storage for test data.
    """

    def __init__(self):
        self._data: Dict[str, List[Dict[str, Any]]] = {}

    async def connect(self) -> None:
        """Mock connect - no-op."""
        pass

    async def disconnect(self) -> None:
        """Mock disconnect - no-op."""
        pass

    async def insert(self, table: str, data: Dict[str, Any]) -> str:
        """Insert data into mock storage."""
        if table not in self._data:
            self._data[table] = []
        self._data[table].append(data)
        return data.get("id", data.get("session_id", data.get("request_id", "mock-id")))

    async def fetch_one(self, query: str, params: tuple = ()) -> Optional[Dict[str, Any]]:
        """Fetch one record from mock storage."""
        # Simple mock - just return first record from relevant table
        for table_name, records in self._data.items():
            if records:
                return records[0]
        return None

    async def fetch_all(self, query: str, params: tuple = ()) -> List[Dict[str, Any]]:
        """Fetch all records from mock storage."""
        all_records = []
        for records in self._data.values():
            all_records.extend(records)
        return all_records

    async def update(self, table: str, data: Dict[str, Any], condition: str, params: tuple = ()) -> int:
        """Update records in mock storage."""
        return 1  # Mock success


class MockMemoryService:
    """Mock memory service for mission system tests.

    Provides mock implementations of memory operations.
    """

    def __init__(self):
        self._context: Dict[str, Any] = {}
        self._events: List[Dict[str, Any]] = []

    async def get_context(self, session_id: str) -> Dict[str, Any]:
        """Get mock context for a session."""
        return self._context.get(session_id, {
            "history": [],
            "facts": [],
            "recent_events": [],
        })

    async def save_context(self, session_id: str, context: Dict[str, Any]) -> None:
        """Save context for a session."""
        self._context[session_id] = context

    async def log_event(self, event: Dict[str, Any]) -> None:
        """Log an event."""
        self._events.append(event)

    async def get_events(self, session_id: str, limit: int = 10) -> List[Dict[str, Any]]:
        """Get events for a session."""
        return [e for e in self._events if e.get("session_id") == session_id][:limit]


class MockEventBus:
    """Mock event bus for mission system tests."""

    def __init__(self):
        self.events: List[Dict[str, Any]] = []
        self._handlers: Dict[str, List[Callable]] = {}

    async def emit(self, event: Any) -> None:
        """Emit an event."""
        self.events.append(event if isinstance(event, dict) else {"event": event})

    def subscribe(self, pattern: str, handler: Callable) -> Callable[[], None]:
        """Subscribe to events matching pattern."""
        if pattern not in self._handlers:
            self._handlers[pattern] = []
        self._handlers[pattern].append(handler)
        return lambda: self._handlers[pattern].remove(handler)

    async def emit_agent_started(self, agent_name: str, request_context: Any, **payload) -> None:
        self.events.append({
            "type": "agent.started",
            "agent": agent_name,
            "request_context": getattr(request_context, "to_dict", lambda: request_context)(),
            **payload,
        })

    async def emit_agent_completed(self, agent_name: str, request_context: Any, status: str, error: Optional[str] = None, **payload) -> None:
        self.events.append({
            "type": "agent.completed",
            "agent": agent_name,
            "status": status,
            "request_context": getattr(request_context, "to_dict", lambda: request_context)(),
            **payload,
        })

    async def emit_tool_started(self, tool_name: str, request_context: Any, step: int, total: int) -> None:
        self.events.append({
            "type": "tool.started",
            "tool": tool_name,
            "request_context": getattr(request_context, "to_dict", lambda: request_context)(),
        })

    async def emit_tool_completed(self, tool_name: str, request_context: Any, status: str, execution_time_ms: int, error: Optional[str] = None) -> None:
        self.events.append({
            "type": "tool.completed",
            "tool": tool_name,
            "status": status,
            "request_context": getattr(request_context, "to_dict", lambda: request_context)(),
        })


# =============================================================================
# PYTEST FIXTURES
# =============================================================================

@pytest.fixture
def mock_llm_adapter():
    """Create a mock LLM adapter."""
    return MockLLMAdapter()


@pytest.fixture
def mock_llm_adapter_factory():
    """Factory for creating mock LLM adapters with custom responses."""
    def _create(responses: Optional[Dict[str, str]] = None) -> MockLLMAdapter:
        return MockLLMAdapter(responses)
    return _create


@pytest.fixture
def mock_database_client():
    """Create a mock database client."""
    return MockDatabaseClient()


@pytest.fixture
def mock_memory_service():
    """Create a mock memory service."""
    return MockMemoryService()


@pytest.fixture
def mock_event_bus():
    """Create a mock event bus."""
    return MockEventBus()
