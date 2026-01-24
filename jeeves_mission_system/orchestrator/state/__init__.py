"""
State definitions for Jeeves workflow orchestration.

This package provides PipelineState TypedDict which mirrors Envelope structure
for workflow state management. The actual orchestration is handled by a pure
Python async runtime (see jeeves-capability-code-analyser/orchestration/runtime.py).

Main exports:
- PipelineState: TypedDict state container (mirrors Envelope)
- create_initial_state: Factory function to create initial workflow state
- REDUCER_FIELDS: List of fields that require list concatenation during merges
"""

from jeeves_mission_system.orchestrator.state.state import (
    PipelineState,
    create_initial_state,
    REDUCER_FIELDS,
)

__all__ = [
    "PipelineState",
    "create_initial_state",
    "REDUCER_FIELDS",
]
