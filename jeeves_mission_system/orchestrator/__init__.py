"""
Code Analysis Orchestrator - gRPC service layer.

This package provides the gRPC server that:
- Wraps the 7-agent code analysis orchestration
- Exposes services for code analysis flow
- Runs as a separate container from the HTTP gateway

The orchestrator owns:
- Agent pipeline (Perception → Intent → Planner → Traverser → Synthesizer → Critic → Integration)
- Database connections
- LLM provider management
- Tool registry

Unified Interrupt System (v4.0):
- ConfirmationOrchestrator has been REMOVED
- All interrupt handling now goes through jeeves_control_tower.services.InterruptService
- EventOrchestrator remains for agent event emission
"""

__version__ = "0.1.0"

# Phase 7: Unified Event Orchestrator (retained)
from jeeves_mission_system.orchestrator.events import (
    EventOrchestrator,
    create_event_orchestrator,
    AgentEventType,
    AgentEvent,
    AgentEventEmitter,
    AgentEventContext,
)

__all__ = [
    # Events (Phase 7)
    "EventOrchestrator",
    "create_event_orchestrator",
    "AgentEventType",
    "AgentEvent",
    "AgentEventEmitter",
    "AgentEventContext",
]
