"""Jeeves Infrastructure Runtime - Agent and pipeline execution.

This module contains the runtime components for executing agents
and pipelines.

Usage:
    from jeeves_infra.runtime import Agent, PipelineRunner
    from jeeves_infra.runtime import create_pipeline_runner, create_envelope
"""

from jeeves_infra.runtime.agents import (
    # Protocols
    ToolExecutor,
    Logger,
    Persistence,
    PromptRegistry,
    EventContext,
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

__all__ = [
    # Protocols
    "ToolExecutor",
    "Logger",
    "Persistence",
    "PromptRegistry",
    "EventContext",
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
    # Factories
    "create_pipeline_runner",
    "create_envelope",
]
