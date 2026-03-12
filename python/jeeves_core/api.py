"""Single-import surface for capability definition.

Usage:
    from jeeves_core.api import PipelineConfig, stage, eq, DeterministicAgent
"""

# Stage/pipeline definition
from jeeves_core.protocols import (
    PipelineConfig, AgentConfig, stage, Edge, RoutingRule,
    RunMode, JoinStrategy, TokenStreamMode, GenerationParams,
    DomainServiceConfig, EdgeLimit,
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
