"""Single-import surface for capability definition and registration."""

# Stage/pipeline definition
from jeeves_core.protocols import (
    PipelineConfig, AgentConfig, stage, Edge, RoutingRule,
    RunMode, JoinStrategy, TokenStreamMode, GenerationParams,
    DomainServiceConfig, EdgeLimit,
    # Wiring infrastructure
    DomainModeConfig, DomainAgentConfig,
    CapabilityToolsConfig, CapabilityOrchestratorConfig,
    AgentLLMConfig,
    # Context types
    RequestContext, AgentContext,
)
# Routing expression builders
from jeeves_core.protocols.routing import (
    eq, neq, gt, lt, gte, lte, contains, exists, not_exists,
    and_, or_, not_, always, agent, meta, state, interrupt, current,
)
# Registration
from jeeves_core.capability_wiring import register_capability
# Runtime base class
from jeeves_core.runtime.deterministic import DeterministicAgent
# Testing helpers
from jeeves_core.testing.helpers import make_agent_context, make_agent_config
