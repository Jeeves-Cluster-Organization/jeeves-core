"""Test that jeeves_core top-level re-exports work."""


def test_surface_imports():
    """All curated __init__.py exports are importable."""
    from jeeves_core import (
        # Stage/pipeline
        PipelineConfig, AgentConfig, stage, Edge, RoutingRule,
        RunMode, JoinStrategy, TokenStreamMode, GenerationParams,
        EdgeLimit,
        # Wiring (consumer-facing)
        ModeConfig,
        ToolsConfig,
        ToolCatalog,
        # Context types
        RequestContext, AgentContext,
        # Routing builders
        eq, neq, gt, lt, gte, lte, contains, exists, not_exists,
        and_, or_, not_, always, agent, meta, state, interrupt, current,
        # Registration
        register_capability,
        # Runtime
        Agent, DeterministicAgent,
        PipelineRunner, create_pipeline_runner,
        CapabilityService, CapabilityResult,
        # Enums
        TerminalReason,
        # Interfaces
        AppContextProtocol, LLMProviderProtocol, ToolRegistryProtocol,
        # Bootstrap
        create_app_context,
    )
    # Smoke: construct a stage
    s = stage("test", prompt_key="p.test", default_next=None)
    assert s.name == "test"
    assert s.has_llm is True
