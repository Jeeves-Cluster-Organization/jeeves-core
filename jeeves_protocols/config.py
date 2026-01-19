"""Configuration types - mirrors Go coreengine/config.

Pure data types for configuration. No global state.
Configuration is injected via AppContext (see jeeves_avionics/context.py).
"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Callable, Dict, List, Optional, Set

from jeeves_protocols.core import ToolAccess


class RunMode(str, Enum):
    """How the pipeline runs stages."""
    SEQUENTIAL = "sequential"  # One stage at a time
    PARALLEL = "parallel"      # Independent stages run concurrently


@dataclass
class RoutingRule:
    """Routing rule for conditional transitions."""
    condition: str
    value: Any
    target: str


@dataclass
class AgentConfig:
    """Declarative agent configuration."""
    name: str
    stage_order: int = 0

    # Capabilities
    has_llm: bool = False
    has_tools: bool = False
    has_policies: bool = False

    # Tool access
    tool_access: ToolAccess = ToolAccess.NONE
    allowed_tools: Optional[Set[str]] = None

    # LLM config
    model_role: Optional[str] = None
    prompt_key: Optional[str] = None
    temperature: Optional[float] = None
    max_tokens: Optional[int] = None

    # Output
    output_key: str = ""
    required_output_fields: List[str] = field(default_factory=list)

    # Routing
    routing_rules: List[RoutingRule] = field(default_factory=list)
    default_next: Optional[str] = None
    error_next: Optional[str] = None

    # Bounds
    timeout_seconds: Optional[int] = None
    max_retries: int = 0

    # Processing hooks (optional callables for custom behavior)
    pre_process: Optional[Callable] = None
    post_process: Optional[Callable] = None
    mock_handler: Optional[Callable] = None


@dataclass
class PipelineConfig:
    """Pipeline configuration."""
    name: str
    agents: List[AgentConfig] = field(default_factory=list)

    # Run mode: sequential or parallel
    default_run_mode: RunMode = RunMode.SEQUENTIAL

    # Global config
    max_iterations: int = 3
    max_llm_calls: int = 10
    max_agent_hops: int = 21
    default_timeout_seconds: int = 300

    # Feature flags
    enable_arbiter: bool = True
    skip_arbiter_for_read_only: bool = True

    # Interrupt resume stages (capability-defined, not hardcoded in runtime)
    # If None, defaults to second stage in pipeline for clarification,
    # or stage with has_tools=True for confirmation
    clarification_resume_stage: Optional[str] = None
    confirmation_resume_stage: Optional[str] = None

    def get_stage_order(self) -> List[str]:
        """Get ordered list of agent names."""
        return [a.name for a in sorted(self.agents, key=lambda x: x.stage_order)]

    def get_clarification_resume_stage(self) -> str:
        """Get stage to resume after clarification interrupt."""
        if self.clarification_resume_stage:
            return self.clarification_resume_stage
        # Default: second stage in pipeline (index 1)
        stages = self.get_stage_order()
        return stages[1] if len(stages) > 1 else stages[0] if stages else "end"

    def get_confirmation_resume_stage(self) -> str:
        """Get stage to resume after confirmation interrupt."""
        if self.confirmation_resume_stage:
            return self.confirmation_resume_stage
        # Default: first stage with has_tools=True
        for agent in self.agents:
            if agent.has_tools:
                return agent.name
        # Fallback: third stage (typical executor position)
        stages = self.get_stage_order()
        return stages[3] if len(stages) > 3 else stages[-1] if stages else "end"


@dataclass
class ContextBounds:
    """Context window bounds."""
    max_input_tokens: int = 4096
    max_output_tokens: int = 2048
    max_context_tokens: int = 16384
    reserved_tokens: int = 512


@dataclass
class CoreConfig:
    """Core runtime configuration."""
    # Bounds
    max_iterations: int = 3
    max_llm_calls: int = 10
    max_agent_hops: int = 21

    # Context
    context_bounds: ContextBounds = field(default_factory=ContextBounds)

    # Feature flags
    enable_telemetry: bool = True
    enable_checkpoints: bool = False
    debug_mode: bool = False


@dataclass
class OrchestrationFlags:
    """Runtime orchestration flags."""
    enable_parallel_agents: bool = False
    enable_checkpoints: bool = False
    enable_distributed: bool = False
    enable_telemetry: bool = True
    max_concurrent_agents: int = 4
    checkpoint_interval_seconds: int = 30
