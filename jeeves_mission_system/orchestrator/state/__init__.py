"""
State definitions for Jeeves workflow orchestration.

This package provides JeevesState TypedDict which mirrors GenericEnvelope structure
for workflow state management. The actual orchestration is handled by a pure
Python async runtime (see jeeves-capability-code-analyser/orchestration/runtime.py).

Main exports:
- JeevesState: TypedDict state container (mirrors GenericEnvelope)
- create_initial_state: Factory function to create initial workflow state
- REDUCER_FIELDS: List of fields that require list concatenation during merges
"""

from jeeves_mission_system.orchestrator.state.state import (
    JeevesState,
    create_initial_state,
    REDUCER_FIELDS,
)

__all__ = [
    "JeevesState",
    "create_initial_state",
    "REDUCER_FIELDS",
]
