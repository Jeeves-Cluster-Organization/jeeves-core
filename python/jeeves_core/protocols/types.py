"""Python type definitions for the Jeeves protocol layer.

These dataclasses and enums define the contract between Python and Rust.
No proto dependency — code is the contract.

Enums matching Rust definitions are auto-generated in _generated.py
(run: cd jeeves-core && python codegen/generate_python_types.py).
Python-only enums (no Rust equivalent) are defined here.
"""

from dataclasses import dataclass, field, replace
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Callable, Dict, List, Optional, Set, TYPE_CHECKING

from jeeves_core.protocols.interfaces import RequestContext


# =============================================================================
# LLM RESULT TYPES (typed boundary — replaces Dict[str, Any] returns)
# =============================================================================

@dataclass(frozen=True)
class LLMToolCall:
    """A tool call from an LLM response."""
    id: str
    name: str
    arguments: Dict[str, Any]


@dataclass(frozen=True)
class LLMUsage:
    """Token usage from an LLM call."""
    prompt_tokens: int = 0
    completion_tokens: int = 0
    total_tokens: int = 0


@dataclass(frozen=True)
class LLMResult:
    """Typed result from an LLM provider."""
    content: str = ""
    tool_calls: List[LLMToolCall] = field(default_factory=list)
    usage: LLMUsage = field(default_factory=LLMUsage)
    raw: Dict[str, Any] = field(default_factory=dict)

# =============================================================================
# RUST-GENERATED ENUMS (canonical source: Rust serde output)
# =============================================================================

from jeeves_core.protocols._generated import (  # noqa: E402
    TerminalReason,
    InterruptKind,
    InterruptStatus,
    RiskSemantic,
    RiskSeverity,
    ToolCategory,
    HealthStatus,
    LoopVerdict,
    RiskApproval,
    OperationStatus,
)

# =============================================================================
# PYTHON-ONLY ENUMS (no Rust equivalent — never cross IPC)
# =============================================================================


class RunMode(str, Enum):
    """Pipeline run mode."""
    SEQUENTIAL = "sequential"
    PARALLEL = "parallel"


class JoinStrategy(str, Enum):
    """Join strategy for dependencies."""
    ALL = "all"
    ANY = "any"


class TokenStreamMode(str, Enum):
    """Token streaming mode."""
    OFF = "off"
    DEBUG = "debug"
    AUTHORITATIVE = "authoritative"


# =============================================================================
# OPERATION RESULT
# =============================================================================

@dataclass
class OperationResult:
    """Result of an operation."""
    status: OperationStatus = OperationStatus.SUCCESS
    data: Optional[Dict[str, Any]] = None
    error: Optional[str] = None
    error_type: Optional[str] = None
    suggestions: List[str] = field(default_factory=list)


# =============================================================================
# INTERRUPT TYPES
# =============================================================================

@dataclass
class InterruptResponse:
    """Response to an interrupt."""
    text: str = ""
    approved: bool = False
    decision: str = ""
    data: Optional[Dict[str, Any]] = None
    received_at: Optional[datetime] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "text": self.text,
            "approved": self.approved,
            "decision": self.decision,
            "data": self.data,
            "received_at": self.received_at.isoformat() if self.received_at else None,
        }


@dataclass
class FlowInterrupt:
    """Flow interrupt for user interaction."""
    id: str = ""
    kind: Optional[InterruptKind] = None
    request_id: str = ""
    user_id: str = ""
    session_id: str = ""
    envelope_id: str = ""
    question: str = ""
    message: str = ""
    data: Optional[Dict[str, Any]] = None
    response: Optional[InterruptResponse] = None
    status: InterruptStatus = InterruptStatus.PENDING
    created_at: Optional[datetime] = None
    expires_at: Optional[datetime] = None
    resolved_at: Optional[datetime] = None
    trace_id: str = ""
    span_id: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "id": self.id,
            "kind": self.kind.value if isinstance(self.kind, Enum) else self.kind,
            "request_id": self.request_id,
            "user_id": self.user_id,
            "session_id": self.session_id,
            "envelope_id": self.envelope_id,
            "question": self.question,
            "message": self.message,
            "data": self.data,
            "response": self.response.to_dict() if self.response else None,
            "status": self.status.value if isinstance(self.status, Enum) else self.status,
            "created_at": self.created_at.isoformat() if self.created_at else None,
            "expires_at": self.expires_at.isoformat() if self.expires_at else None,
            "resolved_at": self.resolved_at.isoformat() if self.resolved_at else None,
            "trace_id": self.trace_id,
            "span_id": self.span_id,
        }

    @classmethod
    def from_db_row(cls, row: Dict[str, Any]) -> "FlowInterrupt":
        """Create FlowInterrupt from database row."""
        kind_value = row.get("kind")
        kind = None
        if isinstance(kind_value, str):
            try:
                kind = InterruptKind(kind_value)
            except ValueError:
                pass  # Unknown kind stays None
        elif isinstance(kind_value, InterruptKind):
            kind = kind_value

        status_value = row.get("status", "pending")
        if isinstance(status_value, str):
            try:
                status = InterruptStatus(status_value)
            except ValueError:
                status = InterruptStatus.PENDING
        else:
            status = status_value

        response_data = row.get("response")
        response = None
        if response_data and isinstance(response_data, dict):
            response = InterruptResponse(**response_data)

        return cls(
            id=row.get("id", ""),
            kind=kind,
            request_id=row.get("request_id", ""),
            user_id=row.get("user_id", ""),
            session_id=row.get("session_id", ""),
            envelope_id=row.get("envelope_id", ""),
            question=row.get("question", ""),
            message=row.get("message", ""),
            data=row.get("data"),
            response=response,
            status=status,
            trace_id=row.get("trace_id", ""),
            span_id=row.get("span_id", ""),
        )


@dataclass
class KernelInterrupt:
    """Kernel-level interrupt for system scope operations.

    Unlike FlowInterrupt (capability/pipeline scope), KernelInterrupt
    represents system-level interrupts managed by the Rust kernel.
    """
    id: str = ""
    kind: Optional[InterruptKind] = None
    process_id: str = ""
    question: str = ""
    message: str = ""
    data: Optional[Dict[str, Any]] = None
    status: InterruptStatus = InterruptStatus.PENDING
    created_at: Optional[datetime] = None
    expires_at: Optional[datetime] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "id": self.id,
            "kind": self.kind.value if isinstance(self.kind, Enum) else self.kind,
            "process_id": self.process_id,
            "question": self.question,
            "message": self.message,
            "data": self.data,
            "status": self.status.value if isinstance(self.status, Enum) else self.status,
            "created_at": self.created_at.isoformat() if self.created_at else None,
            "expires_at": self.expires_at.isoformat() if self.expires_at else None,
        }


# =============================================================================
# RATE LIMITING
# =============================================================================

@dataclass
class RateLimitConfig:
    """Rate limit configuration."""
    requests_per_minute: int = 60
    requests_per_hour: int = 1000
    requests_per_day: int = 10000
    burst_size: int = 10


@dataclass
class RateLimitResult:
    """Result of rate limit check."""
    allowed: bool = True
    exceeded: bool = False
    reason: str = ""
    limit_type: str = ""
    current_count: int = 0
    limit: int = 0
    retry_after_seconds: float = 0.0
    remaining: int = 0


# =============================================================================
# PROCESSING RECORD
# =============================================================================

@dataclass
class ProcessingRecord:
    """Record of agent processing step."""
    agent: str
    stage_order: int
    started_at: datetime
    completed_at: Optional[datetime] = None
    duration_ms: int = 0
    status: str = "running"
    error: Optional[str] = None
    llm_calls: int = 0


# =============================================================================
# PIPELINE EVENT
# =============================================================================

@dataclass
class PipelineEvent:
    """Event from pipeline execution."""
    type: str
    stage: str
    data: Dict[str, Any]
    debug: bool = False


# =============================================================================
# CONFIG TYPES
# =============================================================================

@dataclass
class RoutingRule:
    """Routing rule: expression tree + target stage.

    expr: A RoutingExpr dict (built via jeeves_core.protocols.routing builders).
    target: Name of the target stage to route to when expr evaluates to true.
    """
    expr: Dict[str, Any]
    target: str

    def to_kernel_dict(self) -> Dict[str, Any]:
        return {"expr": self.expr, "target": self.target}


@dataclass
class EdgeLimit:
    """Per-edge transition limit."""
    from_stage: str
    to_stage: str
    max_count: int


@dataclass
class GenerationParams:
    """Generation control parameters."""
    stop: Optional[List[str]] = None
    repeat_penalty: Optional[float] = None
    top_p: Optional[float] = None
    top_k: Optional[int] = None
    seed: Optional[int] = None

    def __post_init__(self):
        if self.top_p is not None and not (0 < self.top_p <= 1):
            raise ValueError(f"top_p must be in (0, 1], got {self.top_p}")
        if self.top_k is not None and self.top_k < 0:
            raise ValueError(f"top_k must be >= 0, got {self.top_k}")
        if self.repeat_penalty is not None and self.repeat_penalty < 1.0:
            raise ValueError(f"repeat_penalty must be >= 1.0, got {self.repeat_penalty}")

    def to_dict(self) -> Dict[str, Any]:
        return {k: v for k, v in {
            "stop": self.stop,
            "repeat_penalty": self.repeat_penalty,
            "top_p": self.top_p,
            "top_k": self.top_k,
            "seed": self.seed,
        }.items() if v is not None}


@dataclass(frozen=True)
class RetrievedContext:
    """Standardized shape for retrieved content."""
    content: str
    source: str = ""
    score: float = 0.0
    metadata: Dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class ClassificationResult:
    """Result from an embedding-based classifier."""
    label: str
    score: float
    all_scores: Dict[str, float] = field(default_factory=dict)


@dataclass
class RetrievalConfig:
    """Retrieval hints for an agent stage. Python-only, not serialized to kernel."""
    retriever_key: str = ""
    limit: int = 10
    inject_as: str = "retrieved_context"
    filters: Dict[str, Any] = field(default_factory=dict)


@dataclass
class AgentConfig:
    """Declarative agent configuration."""
    name: str
    stage_order: int = 0
    requires: List[str] = field(default_factory=list)
    join_strategy: JoinStrategy = JoinStrategy.ALL
    has_llm: bool = False
    has_tools: bool = False
    has_policies: bool = False
    allowed_tools: Optional[Set[str]] = None
    output_schema: Optional[Dict[str, Any]] = None
    model_role: Optional[str] = None
    prompt_key: Optional[str] = None
    temperature: Optional[float] = None
    max_tokens: Optional[int] = None
    generation: Optional[GenerationParams] = None
    output_key: str = ""
    token_stream: TokenStreamMode = TokenStreamMode.OFF
    streaming_prompt_key: Optional[str] = None
    routing_rules: List[RoutingRule] = field(default_factory=list)
    default_next: Optional[str] = None
    error_next: Optional[str] = None
    parallel_group: Optional[str] = None
    max_visits: Optional[int] = None
    pre_process: Optional[Callable] = None
    post_process: Optional[Callable] = None
    mock_handler: Optional[Callable] = None
    # Retrieval hints (Python-only, not serialized to kernel)
    retrieval: Optional[RetrievalConfig] = None
    # Tool dispatch mode (deterministic, no LLM) — Python-only, not serialized to kernel
    tool_dispatch: Optional[str] = None        # "auto" = framework handles dispatch
    tool_source_agent: Optional[str] = None    # output_key of agent with tool selection
    tool_name_field: str = "tool"              # field in source output with tool name
    tool_params_field: str = "params"          # field in source output with params dict

    def __post_init__(self):
        if not self.output_key:
            self.output_key = self.name
        if self.token_stream == TokenStreamMode.AUTHORITATIVE and self.output_schema is not None:
            raise ValueError(
                f"Agent '{self.name}': AUTHORITATIVE streaming + output_schema is forbidden. "
                "Cannot authoritatively stream structured output."
            )

    def to_kernel_dict(self) -> Dict[str, Any]:
        """Serialize to the dict shape the Rust kernel expects for PipelineStage."""
        d: Dict[str, Any] = {
            "name": self.name,
            "agent": self.name,
            "routing": [r.to_kernel_dict() for r in self.routing_rules],
        }
        if self.default_next is not None:
            d["default_next"] = self.default_next
        if self.error_next is not None:
            d["error_next"] = self.error_next
        if self.parallel_group is not None:
            d["parallel_group"] = self.parallel_group
            d["join_strategy"] = "WaitAll" if self.join_strategy == JoinStrategy.ALL else "WaitFirst"
        if self.max_visits is not None:
            d["max_visits"] = self.max_visits
        if self.output_schema is not None:
            d["output_schema"] = self.output_schema
        if self.allowed_tools:
            d["allowed_tools"] = sorted(self.allowed_tools)
        return d


def stage(
    name: str,
    *,
    prompt_key: str | None = None,
    output_key: str | None = None,
    output_schema: Dict[str, Any] | None = None,
    model_role: str | None = None,
    temperature: float | None = None,
    max_tokens: int | None = None,
    generation: GenerationParams | None = None,
    tools: bool = False,
    tool_dispatch: str | None = None,
    tool_source_agent: str | None = None,
    tool_name_field: str = "tool",
    tool_params_field: str = "params",
    routing_rules: list[RoutingRule] | None = None,
    default_next: str | None = None,
    error_next: str | None = None,
    parallel_group: str | None = None,
    join_strategy: JoinStrategy = JoinStrategy.ALL,
    max_visits: int | None = None,
    pre_process: Callable | None = None,
    post_process: Callable | None = None,
    mock_handler: Callable | None = None,
    token_stream: TokenStreamMode = TokenStreamMode.OFF,
    streaming_prompt_key: str | None = None,
    allowed_tools: Set[str] | None = None,
    retrieval: RetrievalConfig | None = None,
) -> AgentConfig:
    """Shorthand for AgentConfig with inference.

    Infers:
    - has_llm=True if prompt_key is set
    - has_tools=True if tools=True or tool_dispatch is set
    - output_key defaults to name
    - model_role defaults to name when has_llm and no explicit model_role
    """
    has_llm = prompt_key is not None
    has_tools = tools or tool_dispatch is not None

    return AgentConfig(
        name=name,
        has_llm=has_llm,
        has_tools=has_tools,
        model_role=model_role or (name if has_llm else None),
        prompt_key=prompt_key,
        output_key=output_key or name,
        output_schema=output_schema,
        temperature=temperature,
        max_tokens=max_tokens,
        generation=generation,
        routing_rules=routing_rules or [],
        default_next=default_next,
        error_next=error_next,
        parallel_group=parallel_group,
        join_strategy=join_strategy,
        max_visits=max_visits,
        pre_process=pre_process,
        post_process=post_process,
        mock_handler=mock_handler,
        token_stream=token_stream,
        streaming_prompt_key=streaming_prompt_key,
        tool_dispatch=tool_dispatch,
        tool_source_agent=tool_source_agent,
        tool_name_field=tool_name_field,
        tool_params_field=tool_params_field,
        allowed_tools=allowed_tools,
        retrieval=retrieval,
    )


@dataclass
class Edge:
    """Directed edge in a pipeline graph."""
    source: str
    target: str
    when: Optional[Dict[str, Any]] = None  # RoutingExpr dict, None = default/unconditional


@dataclass
class PipelineConfig:
    """Pipeline configuration."""
    name: str
    agents: List[AgentConfig] = field(default_factory=list)
    default_run_mode: RunMode = RunMode.SEQUENTIAL
    max_iterations: int = 3
    max_llm_calls: int = 10
    max_agent_hops: int = 21
    default_timeout_seconds: int = 300
    edge_limits: List[EdgeLimit] = field(default_factory=list)
    step_limit: Optional[int] = None
    clarification_resume_stage: Optional[str] = None
    confirmation_resume_stage: Optional[str] = None
    agent_review_resume_stage: Optional[str] = None

    def to_kernel_dict(self) -> Dict[str, Any]:
        """Serialize to the dict shape the Rust kernel expects for PipelineConfig."""
        d: Dict[str, Any] = {
            "name": self.name,
            "max_iterations": self.max_iterations,
            "max_llm_calls": self.max_llm_calls,
            "max_agent_hops": self.max_agent_hops,
            "edge_limits": [
                {"from_stage": el.from_stage, "to_stage": el.to_stage, "max_count": el.max_count}
                for el in self.edge_limits
            ],
            "stages": [agent.to_kernel_dict() for agent in self.agents],
        }
        if self.step_limit is not None:
            d["step_limit"] = self.step_limit
        return d

    def get_stage_order(self) -> List[str]:
        return [a.name for a in sorted(self.agents, key=lambda x: x.stage_order)]

    @classmethod
    def chain(
        cls,
        name: str,
        agents: List["AgentConfig"],
        *,
        max_iterations: int = 3,
        max_llm_calls: int = 10,
        max_agent_hops: int = 21,
        error_next: Optional[str] = None,
        **kwargs,
    ) -> "PipelineConfig":
        """Build sequential pipeline with auto-wired routing.

        Auto-wires:
        - stage_order from list position
        - default_next chains to next element, last gets None
        - error_next global applied to all non-terminal stages
          (only where agent.error_next is None)

        Args:
            name: Pipeline name.
            agents: Ordered list of AgentConfig (stage_order will be overwritten).
            max_iterations: Pipeline iteration limit.
            max_llm_calls: LLM call limit.
            max_agent_hops: Agent hop limit.
            error_next: Global error_next stage (applied to all non-terminal stages
                       where error_next is not already set).
            **kwargs: Additional PipelineConfig fields (edge_limits, etc.).

        Returns:
            Fully wired PipelineConfig.
        """
        wired = []
        for i, agent in enumerate(agents):
            is_last = (i == len(agents) - 1)
            next_name = None if is_last else agents[i + 1].name

            wired.append(replace(agent,
                stage_order=i,
                default_next=agent.default_next if agent.default_next is not None else next_name,
                error_next=(
                    agent.error_next if agent.error_next is not None
                    else (error_next if not is_last else None)
                ),
            ))

        return cls(
            name=name,
            agents=wired,
            max_iterations=max_iterations,
            max_llm_calls=max_llm_calls,
            max_agent_hops=max_agent_hops,
            **kwargs,
        )

    @classmethod
    def graph(
        cls,
        name: str,
        stages: Dict[str, "AgentConfig"],
        edges: List["Edge"],
        *,
        error_next: Optional[str] = None,
        max_iterations: int = 3,
        max_llm_calls: int = 10,
        max_agent_hops: int = 21,
        **kwargs,
    ) -> "PipelineConfig":
        """Build routed pipeline from stages and edges.

        Auto-wires:
        - stage_order from dict insertion order
        - routing_rules from conditional edges (when != None)
        - default_next from unconditional edges (first match)
        - error_next applied globally unless stage overrides
        """
        stage_names = set(stages.keys())

        # Validate edge targets
        for edge in edges:
            if edge.source not in stage_names:
                raise ValueError(f"Edge source '{edge.source}' not in stages")
            if edge.target not in stage_names:
                raise ValueError(f"Edge target '{edge.target}' not in stages")

        # Collect edges per source
        conditional: Dict[str, List[RoutingRule]] = {s: [] for s in stages}
        unconditional: Dict[str, Optional[str]] = {s: None for s in stages}

        for edge in edges:
            if edge.when is not None:
                conditional[edge.source].append(
                    RoutingRule(expr=edge.when, target=edge.target)
                )
            else:
                if unconditional[edge.source] is not None:
                    raise ValueError(
                        f"Multiple unconditional edges from '{edge.source}': "
                        f"'{unconditional[edge.source]}' and '{edge.target}'"
                    )
                unconditional[edge.source] = edge.target

        wired = []
        for i, (stage_name, agent_config) in enumerate(stages.items()):
            wired.append(replace(agent_config,
                stage_order=i,
                routing_rules=conditional[stage_name],
                default_next=unconditional[stage_name],
                error_next=(
                    agent_config.error_next if agent_config.error_next is not None
                    else error_next
                ),
            ))

        return cls(
            name=name,
            agents=wired,
            max_iterations=max_iterations,
            max_llm_calls=max_llm_calls,
            max_agent_hops=max_agent_hops,
            **kwargs,
        )



@dataclass
class ContextBounds:
    """Context window bounds."""
    max_input_tokens: int = 4096
    max_output_tokens: int = 2048
    max_context_tokens: int = 16384
    reserved_tokens: int = 512


@dataclass
class ExecutionConfig:
    """Core runtime configuration."""
    max_iterations: int = 3
    max_llm_calls: int = 10
    max_agent_hops: int = 21
    context_bounds: ContextBounds = field(default_factory=ContextBounds)


@dataclass
class OrchestrationFlags:
    """Runtime orchestration flags."""
    max_concurrent_agents: int = 4


# =============================================================================
# INSTRUCTION CONFIG (Typed view of agent_config from kernel)
# =============================================================================

@dataclass(frozen=True)
class InstructionContext:
    """Typed view of agent_config.context from enriched instructions."""
    envelope_id: str = ""
    request_id: str = ""
    user_id: str = ""
    session_id: str = ""
    raw_input: str = ""
    outputs: Dict[str, Dict[str, Any]] = field(default_factory=dict)
    metadata: Dict[str, Any] = field(default_factory=dict)
    prompt_context: Dict[str, Any] = field(default_factory=dict)
    llm_call_count: int = 0
    agent_hop_count: int = 0
    tokens_in: int = 0
    tokens_out: int = 0

    @classmethod
    def from_dict(cls, d: Dict[str, Any]) -> "InstructionContext":
        return cls(
            envelope_id=d.get("envelope_id", ""),
            request_id=d.get("request_id", ""),
            user_id=d.get("user_id", ""),
            session_id=d.get("session_id", ""),
            raw_input=d.get("raw_input", ""),
            outputs=d.get("outputs", {}),
            metadata=dict(d.get("metadata", {})),
            prompt_context=d.get("prompt_context", {}),
            llm_call_count=d.get("llm_call_count", 0),
            agent_hop_count=d.get("agent_hop_count", 0),
            tokens_in=d.get("tokens_in", 0),
            tokens_out=d.get("tokens_out", 0),
        )


@dataclass(frozen=True)
class InstructionConfig:
    """Typed view of agent_config from enriched instructions."""
    context: Optional[InstructionContext] = None
    output_schema: Optional[Dict[str, Any]] = None
    allowed_tools: Optional[List[str]] = None

    @classmethod
    def from_dict(cls, d: Dict[str, Any]) -> "InstructionConfig":
        ctx_data = d.get("context")
        ctx = InstructionContext.from_dict(ctx_data) if ctx_data else None
        return cls(
            context=ctx,
            output_schema=d.get("output_schema"),
            allowed_tools=d.get("allowed_tools"),
        )


# =============================================================================
# AGENT CONTEXT (Thin View of Kernel State)
# =============================================================================

@dataclass(frozen=True)
class AgentContext:
    """Read-only view of kernel state for agent execution.

    Built from enriched Instruction. Not a parallel envelope —
    just a structured view of what the kernel sent.
    Replaces the Python Envelope for agent consumption.
    """
    # Identity
    envelope_id: str = ""
    request_id: str = ""
    user_id: str = ""
    session_id: str = ""

    # Input
    raw_input: str = ""

    # Prior agent outputs (read-only snapshot)
    outputs: Dict[str, Dict[str, Any]] = field(default_factory=dict)

    # Capability-provided context (mutable dict — sent back as metadata_updates)
    metadata: Dict[str, Any] = field(default_factory=dict)

    # Prompt context (kernel-rendered from outputs + metadata)
    prompt_context: Dict[str, Any] = field(default_factory=dict)

    # Bounds/metrics (read-only, kernel-accumulated)
    llm_call_count: int = 0
    agent_hop_count: int = 0
    tokens_in: int = 0
    tokens_out: int = 0

    @classmethod
    def from_instruction(cls, instruction) -> "AgentContext":
        """Build from enriched kernel instruction.

        Args:
            instruction: OrchestratorInstruction with typed agent_config
        """
        config = instruction.agent_config
        ctx = config.context if config.context else InstructionContext()
        return cls(
            envelope_id=ctx.envelope_id,
            request_id=ctx.request_id,
            user_id=ctx.user_id,
            session_id=ctx.session_id,
            raw_input=ctx.raw_input,
            outputs=ctx.outputs,
            metadata=dict(ctx.metadata),
            prompt_context=ctx.prompt_context,
            llm_call_count=ctx.llm_call_count,
            agent_hop_count=ctx.agent_hop_count,
            tokens_in=ctx.tokens_in,
            tokens_out=ctx.tokens_out,
        )


# =============================================================================
# PROTOCOL FOR INTERRUPT SERVICE (from jeeves_core.types.interrupts)
# =============================================================================

from typing import Protocol, runtime_checkable


@runtime_checkable
class InterruptServiceProtocol(Protocol):
    """Interrupt service interface.

    Method signatures match the gateway interrupts router expectations.
    """

    async def create_interrupt(
        self,
        kind: InterruptKind,
        envelope_id: str,
        question: str = "",
        message: str = "",
        data: Optional[Dict[str, Any]] = None,
        request_id: str = "",
        user_id: str = "",
        session_id: str = "",
    ) -> FlowInterrupt: ...

    async def respond(
        self,
        interrupt_id: str,
        response: InterruptResponse,
        user_id: str,
    ) -> Optional[FlowInterrupt]: ...

    async def get_interrupt(self, interrupt_id: str) -> Optional[FlowInterrupt]: ...

    async def get_pending_for_session(
        self,
        session_id: str,
        kinds: Optional[List[InterruptKind]] = None,
    ) -> List[FlowInterrupt]: ...

    async def cancel(
        self,
        interrupt_id: str,
        reason: Optional[str] = None,
    ) -> bool: ...


# =============================================================================
# EXPORTS
# =============================================================================

__all__ = [
    # LLM result types
    "LLMToolCall",
    "LLMUsage",
    "LLMResult",
    # Enums (from _generated.py — Rust canonical)
    "TerminalReason",
    "InterruptKind",
    "InterruptStatus",
    "RiskSemantic",
    "RiskSeverity",
    "ToolCategory",
    "HealthStatus",
    "LoopVerdict",
    "RiskApproval",
    "OperationStatus",
    # Enums (Python-only)
    "RunMode",
    "JoinStrategy",
    "TokenStreamMode",
    # Operation result
    "OperationResult",
    # Interrupt types
    "InterruptResponse",
    "FlowInterrupt",
    "KernelInterrupt",
    "InterruptServiceProtocol",
    # Rate limiting
    "RateLimitConfig",
    "RateLimitResult",
    # Processing
    "ProcessingRecord",
    "PipelineEvent",
    # Config types
    "RoutingRule",
    "EdgeLimit",
    "GenerationParams",
    "AgentConfig",
    "stage",
    "Edge",
    "PipelineConfig",
    "ContextBounds",
    "ExecutionConfig",
    "OrchestrationFlags",
    # Instruction config
    "InstructionContext",
    "InstructionConfig",
    # Agent context
    "AgentContext",
    # Retrieval types
    "RetrievedContext",
    "ClassificationResult",
    "RetrievalConfig",
]
