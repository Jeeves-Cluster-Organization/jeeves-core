"""Test that jeeves_core.api re-exports work (Phase 4a)."""


def test_api_imports():
    """All curated api.py exports are importable."""
    from jeeves_core.api import (
        # Stage/pipeline
        PipelineConfig, AgentConfig, stage, Edge, RoutingRule,
        RunMode, JoinStrategy, TokenStreamMode, GenerationParams,
        DomainServiceConfig, EdgeLimit,
        # Wiring infrastructure
        DomainModeConfig, DomainAgentConfig,
        CapabilityToolsConfig, CapabilityOrchestratorConfig,
        AgentLLMConfig,
        # Context types
        RequestContext, AgentContext,
        # Routing builders
        eq, neq, gt, lt, gte, lte, contains, exists, not_exists,
        and_, or_, not_, always, agent, meta, state, interrupt, current,
        # Registration
        register_capability,
        # Runtime
        DeterministicAgent,
        # Testing
        make_agent_context, make_agent_config,
    )
    # Smoke: construct a stage
    s = stage("test", prompt_key="p.test", default_next=None)
    assert s.name == "test"
    assert s.has_llm is True
