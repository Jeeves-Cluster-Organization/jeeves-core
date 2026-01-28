"""Configuration types - mirrors Go coreengine/config.

Source of truth: coreengine/config/pipeline.go, coreengine/config/execution_config.go

Pure data types for configuration. No global state.
Configuration is injected via AppContext.
"""

from dataclasses import dataclass, field, asdict
from enum import Enum
from typing import Any, Callable, Dict, List, Optional, Set

from jeeves_core.types.enums import ToolAccess


class RunMode(str, Enum):
    """How the pipeline runs stages - mirrors Go RunMode."""
    SEQUENTIAL = "sequential"  # One stage at a time
    PARALLEL = "parallel"      # Independent stages run concurrently


class JoinStrategy(str, Enum):
    """How to handle multiple prerequisites - mirrors Go JoinStrategy."""
    ALL = "all"  # Wait for ALL prerequisites (default)
    ANY = "any"  # Proceed when ANY prerequisite completes


class AgentOutputMode(str, Enum):
    """What is the canonical output?"""
    STRUCTURED = "structured"  # Parsed JSON/tool-call objects
    TEXT = "text"              # Concatenated text from tokens


class TokenStreamMode(str, Enum):
    """Should agent emit tokens during execution?"""
    OFF = "off"              # No token emission
    DEBUG = "debug"          # Emit debug tokens (non-authoritative)
    AUTHORITATIVE = "authoritative"  # Emit authoritative tokens


class AgentCapability(str, Enum):
    """Agent capability flags - mirrors Go AgentCapability."""
    LLM = "llm"           # Agent uses LLM for reasoning
    TOOLS = "tools"       # Agent executes tools
    POLICIES = "policies" # Agent applies policy rules
    SERVICE = "service"   # Agent calls internal services


@dataclass
class RoutingRule:
    """Routing rule for conditional transitions - mirrors Go RoutingRule."""
    condition: str
    value: Any
    target: str


@dataclass
class EdgeLimit:
    """Per-edge transition limit - mirrors Go EdgeLimit."""
    from_stage: str
    to_stage: str
    max_count: int


@dataclass
class GenerationParams:
    """Generation control parameters (K8s-style spec).

    Separates execution policy from content (prompts).
    """
    stop: Optional[List[str]] = None         # Stop sequences
    repeat_penalty: Optional[float] = None   # >= 1.0, penalize repetition
    top_p: Optional[float] = None            # (0, 1], nucleus sampling
    top_k: Optional[int] = None              # >= 0, top-k sampling
    seed: Optional[int] = None               # For reproducibility

    def __post_init__(self):
        """Validate generation parameters."""
        if self.top_p is not None and not (0 < self.top_p <= 1):
            raise ValueError(f"top_p must be in (0, 1], got {self.top_p}")
        if self.top_k is not None and self.top_k < 0:
            raise ValueError(f"top_k must be >= 0, got {self.top_k}")
        if self.repeat_penalty is not None and self.repeat_penalty < 1.0:
            raise ValueError(f"repeat_penalty must be >= 1.0, got {self.repeat_penalty}")
        if self.stop and any(s == "" for s in self.stop):
            raise ValueError("Stop sequences cannot contain empty strings")

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dict, excluding None values."""
        return {k: v for k, v in asdict(self).items() if v is not None}


@dataclass
class AgentConfig:
    """Declarative agent configuration - mirrors Go AgentConfig."""
    name: str
    stage_order: int = 0

    # Dependencies (for parallel execution)
    requires: List[str] = field(default_factory=list)  # Hard dependencies
    after: List[str] = field(default_factory=list)     # Soft ordering
    join_strategy: JoinStrategy = JoinStrategy.ALL

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
    generation: Optional[GenerationParams] = None

    # Output
    output_key: str = ""
    required_output_fields: List[str] = field(default_factory=list)

    # Streaming configuration
    output_mode: AgentOutputMode = AgentOutputMode.STRUCTURED
    token_stream: TokenStreamMode = TokenStreamMode.OFF
    streaming_prompt_key: Optional[str] = None

    # Routing
    routing_rules: List[RoutingRule] = field(default_factory=list)
    default_next: Optional[str] = None
    error_next: Optional[str] = None

    # Bounds
    timeout_seconds: Optional[int] = None
    max_retries: int = 0

    # Processing hooks (Python-only, not in Go)
    pre_process: Optional[Callable] = None
    post_process: Optional[Callable] = None
    mock_handler: Optional[Callable] = None


@dataclass
class PipelineConfig:
    """Pipeline configuration - mirrors Go PipelineConfig."""
    name: str
    agents: List[AgentConfig] = field(default_factory=list)

    # Run mode: sequential or parallel
    default_run_mode: RunMode = RunMode.SEQUENTIAL

    # Bounds - Go enforces these authoritatively
    max_iterations: int = 3
    max_llm_calls: int = 10
    max_agent_hops: int = 21
    default_timeout_seconds: int = 300

    # Edge limits for cyclic routing control
    edge_limits: List[EdgeLimit] = field(default_factory=list)

    # Interrupt resume stages (capability-defined)
    clarification_resume_stage: Optional[str] = None
    confirmation_resume_stage: Optional[str] = None
    agent_review_resume_stage: Optional[str] = None

    def get_stage_order(self) -> List[str]:
        """Get ordered list of agent names."""
        return [a.name for a in sorted(self.agents, key=lambda x: x.stage_order)]

    def get_edge_limit(self, from_stage: str, to_stage: str) -> int:
        """Get cycle limit for an edge, or 0 if no limit."""
        for limit in self.edge_limits:
            if limit.from_stage == from_stage and limit.to_stage == to_stage:
                return limit.max_count
        return 0

    def get_ready_stages(self, completed: Dict[str, bool]) -> List[str]:
        """Get stages ready to execute given completed stages."""
        ready = []
        for agent in self.agents:
            if completed.get(agent.name):
                continue
            # Check requires (hard dependencies)
            requires_ok = all(completed.get(r) for r in agent.requires)
            # Check after (soft ordering)
            after_ok = all(completed.get(a) for a in agent.after)
            # For JoinAny, only need one require satisfied
            if agent.join_strategy == JoinStrategy.ANY and agent.requires:
                requires_ok = any(completed.get(r) for r in agent.requires)
            if requires_ok and after_ok:
                ready.append(agent.name)
        return ready


@dataclass
class ContextBounds:
    """Context window bounds."""
    max_input_tokens: int = 4096
    max_output_tokens: int = 2048
    max_context_tokens: int = 16384
    reserved_tokens: int = 512


@dataclass
class ExecutionConfig:
    """Core runtime configuration - mirrors Go ExecutionConfig."""
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
