"""Jeeves Infrastructure Runtime - Agent and pipeline execution.

This module contains the runtime components for executing agents
and pipelines.

Usage:
    from jeeves_core.runtime import Agent, PipelineRunner
    from jeeves_core.runtime import create_pipeline_runner, create_envelope
"""

from jeeves_core.runtime.agents import (
    # Agent-scoped protocols
    AgentToolExecutor,
    AgentLogger,
    AgentPersistence,
    AgentPromptRegistry,
    AgentEventContext,
    # Type aliases
    LLMProviderFactory,
    PreProcessHook,
    PostProcessHook,
    MockHandler,
    # Classes
    AgentFeatures,
    Agent,
    PipelineRunner,
    OptionalCheckpoint,
    # Factories
    create_pipeline_runner,
    create_envelope,
)

from jeeves_core.runtime.capability_service import (
    CapabilityService,
    CapabilityResult,
)

__all__ = [
    # Agent-scoped protocols
    "AgentToolExecutor",
    "AgentLogger",
    "AgentPersistence",
    "AgentPromptRegistry",
    "AgentEventContext",
    # Type aliases
    "LLMProviderFactory",
    "PreProcessHook",
    "PostProcessHook",
    "MockHandler",
    # Classes
    "AgentFeatures",
    "Agent",
    "PipelineRunner",
    "OptionalCheckpoint",
    # Capability service base
    "CapabilityService",
    "CapabilityResult",
    # Factories
    "create_pipeline_runner",
    "create_envelope",
]
