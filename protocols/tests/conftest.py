"""Pytest configuration for protocols tests.

This conftest.py provides fixtures and configuration for the Protocols
test suite. Protocol tests validate type definitions and utility functions.

Key Principles:
- Test pure Python type definitions
- Test utility functions (JSON repair, truncation, etc.)
- Test envelope serialization/deserialization
- Test memory operations (focus, entities, etc.)

Constitutional Compliance:
- Protocols is the foundation layer - no external dependencies
- Tests should be fast and require no external services
"""

import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

import pytest

# Add the project root to the path for imports
project_root = Path(__file__).parent.parent.parent
if str(project_root) not in sys.path:
    sys.path.insert(0, str(project_root))


# =============================================================================
# PYTEST MARKERS
# =============================================================================

def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "unit: Unit tests (fast, no external dependencies)"
    )
    config.addinivalue_line(
        "markers", "protocols: Protocol type definition tests"
    )


# =============================================================================
# ENVELOPE FIXTURES
# =============================================================================

@pytest.fixture
def sample_request_context():
    """Create a sample RequestContext for testing."""
    from jeeves_core import RequestContext

    return RequestContext(
        request_id="req-456",
        capability="test_capability",
        user_id="user-789",
        session_id="sess-abc",
    )


@pytest.fixture
def sample_envelope(sample_request_context):
    """Create a sample Envelope for testing."""
    from jeeves_core.types import Envelope

    return Envelope(
        request_context=sample_request_context,
        envelope_id="env-123",
        request_id="req-456",
        user_id="user-789",
        session_id="sess-abc",
        raw_input="Test input message",
        received_at=datetime.now(timezone.utc),
        current_stage="perception",
        stage_order=["perception", "intent", "plan", "execute", "synthesize", "critic"],
        iteration=1,
        max_iterations=3,
        llm_call_count=0,
        max_llm_calls=10,
        agent_hop_count=0,
        max_agent_hops=21,
    )


@pytest.fixture
def sample_envelope_dict():
    """Create a sample envelope dictionary for testing from_dict."""
    return {
        "request_context": {
            "request_id": "req-456",
            "capability": "test_capability",
            "user_id": "user-789",
            "session_id": "sess-abc",
            "agent_role": None,
            "trace_id": None,
            "span_id": None,
            "tags": {},
        },
        "envelope_id": "env-123",
        "request_id": "req-456",
        "user_id": "user-789",
        "session_id": "sess-abc",
        "raw_input": "Test input message",
        "current_stage": "perception",
        "stage_order": ["perception", "intent", "plan", "execute", "synthesize", "critic"],
        "iteration": 1,
        "max_iterations": 3,
        "llm_call_count": 2,
        "max_llm_calls": 10,
        "agent_hop_count": 5,
        "max_agent_hops": 21,
        "outputs": {
            "perception": {"entities": ["file.py"], "context": "Test context"}
        },
    }


@pytest.fixture
def sample_processing_record():
    """Create a sample ProcessingRecord for testing."""
    from jeeves_core.types import ProcessingRecord

    return ProcessingRecord(
        agent="perception",
        stage_order=1,
        started_at=datetime.now(timezone.utc),
        completed_at=datetime.now(timezone.utc),
        duration_ms=150,
        status="success",
        llm_calls=1,
    )


# =============================================================================
# MEMORY FIXTURES
# =============================================================================

@pytest.fixture
def sample_working_memory():
    """Create a sample WorkingMemory for testing."""
    from protocols import create_working_memory

    return create_working_memory(
        session_id="sess-abc",
        user_id="user-789",
    )


@pytest.fixture
def sample_focus_state():
    """Create a sample FocusState for testing."""
    from protocols import FocusState, FocusType

    return FocusState(
        focus_type=FocusType.TASK,
        focus_id="task-123",
        context={"name": "Test Task", "status": "in_progress"},
    )


@pytest.fixture
def sample_entity_ref():
    """Create a sample EntityRef for testing."""
    from protocols import EntityRef

    return EntityRef(
        entity_type="file",
        entity_id="file-123",
        display_name="test_file.py",
        metadata={"path": "/src/test_file.py"},
    )


# =============================================================================
# CONFIG FIXTURES
# =============================================================================

@pytest.fixture
def sample_agent_config():
    """Create a sample AgentConfig for testing."""
    from jeeves_core.types import AgentConfig, ToolAccess

    return AgentConfig(
        name="perception",
        stage_order=1,
        has_llm=True,
        has_tools=True,
        tool_access=ToolAccess.READ,
        model_role="analyzer",
        temperature=0.1,
        max_tokens=2000,
        timeout_seconds=60,
    )


@pytest.fixture
def sample_context_bounds():
    """Create a sample ContextBounds for testing."""
    from jeeves_core.types import ContextBounds

    return ContextBounds(
        max_input_tokens=4096,
        max_output_tokens=2048,
        max_context_tokens=16384,
        reserved_tokens=512,
    )


# =============================================================================
# UTILITY TEST DATA
# =============================================================================

@pytest.fixture
def sample_broken_json():
    """Create sample broken JSON strings for repair testing."""
    return [
        '{"key": "value"',  # Missing closing brace
        "{'key': 'value'}",  # Single quotes
        '{"key": value}',  # Unquoted value
        '{"items": [1, 2, 3,]}',  # Trailing comma
    ]


@pytest.fixture
def sample_long_string():
    """Create a sample long string for truncation testing."""
    return "a" * 10000


# Note: anyio_backend fixture is centralized in root conftest.py

# Re-export for pytest discovery
__all__ = [
    # Envelope
    "sample_request_context",
    "sample_envelope",
    "sample_envelope_dict",
    "sample_processing_record",
    # Memory
    "sample_working_memory",
    "sample_focus_state",
    "sample_entity_ref",
    # Config
    "sample_agent_config",
    "sample_context_bounds",
    # Utilities
    "sample_broken_json",
    "sample_long_string",
]
