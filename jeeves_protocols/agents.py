"""Unified Agent Runtime - Go-backed pipeline execution.

Architecture:
    Go (coreengine/)     - Envelope state, bounds checking, DAG structure
    Python (this file)   - Agent execution, LLM calls, tool execution
    Bridge (client.py)   - JSON-over-stdio communication
"""

from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Protocol, AsyncIterator, Tuple

from jeeves_protocols.config import AgentConfig, PipelineConfig
from jeeves_protocols.envelope import GenericEnvelope


# =============================================================================
# PROTOCOLS
# =============================================================================

class LLMProvider(Protocol):
    """Protocol for LLM providers."""
    async def complete(self, messages: List[Dict[str, str]], **kwargs) -> str:
        ...


class ToolExecutor(Protocol):
    """Protocol for tool execution."""
    async def execute(self, tool_name: str, params: Dict[str, Any]) -> Dict[str, Any]:
        ...


class Logger(Protocol):
    """Protocol for structured logging."""
    def info(self, event: str, **kwargs) -> None: ...
    def warn(self, event: str, **kwargs) -> None: ...
    def error(self, event: str, **kwargs) -> None: ...
    def bind(self, **kwargs) -> "Logger": ...


class Persistence(Protocol):
    """Protocol for state persistence."""
    async def save_state(self, thread_id: str, state: Dict[str, Any]) -> None: ...
    async def load_state(self, thread_id: str) -> Optional[Dict[str, Any]]: ...


class PromptRegistry(Protocol):
    """Protocol for prompt retrieval."""
    def get(self, key: str, **kwargs) -> str: ...


class EventContext(Protocol):
    """Protocol for event emission."""
    async def emit_agent_started(self, agent_name: str) -> None: ...
    async def emit_agent_completed(self, agent_name: str, status: str, **kwargs) -> None: ...


# =============================================================================
# TYPE ALIASES
# =============================================================================

LLMProviderFactory = Callable[[str], LLMProvider]
PreProcessHook = Callable[[GenericEnvelope, Optional["UnifiedAgent"]], GenericEnvelope]
PostProcessHook = Callable[[GenericEnvelope, Dict[str, Any], Optional["UnifiedAgent"]], GenericEnvelope]
MockHandler = Callable[[GenericEnvelope], Dict[str, Any]]


# =============================================================================
# AGENT CAPABILITY FLAGS
# =============================================================================

class AgentCapability:
    """Agent capability flags."""
    LLM = "llm"
    TOOLS = "tools"
    POLICIES = "policies"


# =============================================================================
# UNIFIED AGENT
# =============================================================================

@dataclass
class UnifiedAgent:
    """Unified agent driven by configuration.

    Agents are configuration-driven - no subclassing required.
    Behavior determined by config flags and hooks.
    """
    config: AgentConfig
    logger: Logger
    llm: Optional[LLMProvider] = None
    tools: Optional[ToolExecutor] = None
    prompt_registry: Optional[PromptRegistry] = None
    event_context: Optional[EventContext] = None
    use_mock: bool = False

    pre_process: Optional[PreProcessHook] = None
    post_process: Optional[PostProcessHook] = None
    mock_handler: Optional[MockHandler] = None

    @property
    def name(self) -> str:
        return self.config.name

    def set_event_context(self, ctx: EventContext) -> None:
        self.event_context = ctx

    async def process(self, envelope: GenericEnvelope) -> GenericEnvelope:
        """Process envelope through this agent."""
        self.logger.info(f"{self.name}_started", envelope_id=envelope.envelope_id)

        if self.event_context:
            await self.event_context.emit_agent_started(self.name)

        # Pre-process hook
        if self.pre_process:
            envelope = self.pre_process(envelope, self)

        # Get output
        if self.use_mock and self.mock_handler:
            output = self.mock_handler(envelope)
        elif self.config.has_llm and self.llm:
            output = await self._call_llm(envelope)
        else:
            output = envelope.outputs.get(self.config.output_key, {})

        # Tool execution
        if self.config.has_tools and self.tools and output.get("tool_calls"):
            output = await self._execute_tools(envelope, output)

        # Post-process hook
        if self.post_process:
            envelope = self.post_process(envelope, output, self)

        # Store output
        envelope.outputs[self.config.output_key] = output
        envelope.agent_hop_count += 1

        # Route to next stage
        next_stage = self._determine_next_stage(output)
        envelope.current_stage = next_stage

        self.logger.info(f"{self.name}_completed", envelope_id=envelope.envelope_id, next_stage=next_stage)

        if self.event_context:
            await self.event_context.emit_agent_completed(self.name, status="success")

        return envelope

    async def _call_llm(self, envelope: GenericEnvelope) -> Dict[str, Any]:
        """Call LLM with prompt from registry."""
        if not self.prompt_registry:
            raise ValueError(f"Agent {self.name} requires prompt_registry")

        prompt_key = self.config.prompt_key or f"{envelope.metadata.get('pipeline', 'default')}.{self.name}"
        prompt = self.prompt_registry.get(prompt_key, envelope=envelope)
        messages = [{"role": "user", "content": prompt}]

        kwargs = {}
        if self.config.temperature is not None:
            kwargs["temperature"] = self.config.temperature
        if self.config.max_tokens is not None:
            kwargs["max_tokens"] = self.config.max_tokens

        response = await self.llm.complete(messages, **kwargs)
        envelope.llm_call_count += 1

        try:
            import json
            return json.loads(response)
        except (json.JSONDecodeError, TypeError):
            return {"response": response}

    async def _execute_tools(self, envelope: GenericEnvelope, output: Dict[str, Any]) -> Dict[str, Any]:
        """Execute tool calls from LLM output."""
        tool_calls = output.get("tool_calls", [])
        results = []

        for call in tool_calls:
            tool_name = call.get("name")
            params = call.get("params", {})

            if not self._can_access_tool(tool_name):
                results.append({"tool": tool_name, "error": f"Access denied for {self.name}"})
                continue

            try:
                result = await self.tools.execute(tool_name, params)
                results.append({"tool": tool_name, "result": result})
            except Exception as e:
                results.append({"tool": tool_name, "error": str(e)})

        output["tool_results"] = results
        return output

    def _can_access_tool(self, tool_name: str) -> bool:
        """Check tool access based on config."""
        access = self.config.tool_access
        if access == "all":
            return True
        if access == "none":
            return False
        if self.config.allowed_tools:
            return tool_name in self.config.allowed_tools
        return True

    def _determine_next_stage(self, output: Dict[str, Any]) -> str:
        """Determine next stage based on routing rules."""
        for rule in self.config.routing_rules:
            if output.get(rule.condition) == rule.value:
                return rule.target

        if output.get("error") and self.config.error_next:
            return self.config.error_next

        return self.config.default_next or "end"


# =============================================================================
# UNIFIED RUNTIME
# =============================================================================

@dataclass
class UnifiedRuntime:
    """Pipeline runtime - orchestrates agent execution.

    Uses Go for envelope state/bounds when available.
    Uses Python for agent execution, LLM, tools.
    """
    config: PipelineConfig
    llm_factory: Optional[LLMProviderFactory] = None
    tool_executor: Optional[ToolExecutor] = None
    logger: Optional[Logger] = None
    persistence: Optional[Persistence] = None
    prompt_registry: Optional[PromptRegistry] = None
    use_mock: bool = False

    agents: Dict[str, UnifiedAgent] = field(default_factory=dict)
    event_context: Optional[EventContext] = None
    _go_client: Optional[Any] = field(default=None)
    _initialized: bool = field(default=False)

    def __post_init__(self):
        if not self._initialized:
            self._build_agents()
            self._init_go_client()
            self._initialized = True

    def _init_go_client(self):
        """Initialize Go client if available."""
        try:
            from jeeves_protocols.client import GoClient
            client = GoClient()
            if client.is_available():
                self._go_client = client
                if self.logger:
                    self.logger.info("go_client_initialized")
        except Exception:
            self._go_client = None

    def _build_agents(self):
        """Build agents from pipeline config."""
        for agent_config in self.config.agents:
            llm = None
            if agent_config.has_llm and self.llm_factory and agent_config.model_role:
                llm = self.llm_factory(agent_config.model_role)

            tools = self.tool_executor if agent_config.has_tools else None

            agent = UnifiedAgent(
                config=agent_config,
                logger=self.logger.bind(agent=agent_config.name) if self.logger else _NullLogger(),
                llm=llm,
                tools=tools,
                prompt_registry=self.prompt_registry,
                use_mock=self.use_mock,
                pre_process=getattr(agent_config, 'pre_process', None),
                post_process=getattr(agent_config, 'post_process', None),
                mock_handler=getattr(agent_config, 'mock_handler', None),
            )

            self.agents[agent_config.name] = agent

        if self.logger:
            self.logger.info("runtime_initialized", pipeline=self.config.name, agents=list(self.agents.keys()))

    def set_event_context(self, ctx: EventContext) -> None:
        self.event_context = ctx
        for agent in self.agents.values():
            agent.set_event_context(ctx)

    async def run(self, envelope: GenericEnvelope, thread_id: str = "") -> GenericEnvelope:
        """Execute pipeline on envelope."""
        envelope.max_iterations = self.config.max_iterations
        envelope.max_llm_calls = self.config.max_llm_calls
        envelope.max_agent_hops = self.config.max_agent_hops

        stage_order = [a.name for a in self.config.agents]
        envelope.stage_order = stage_order
        envelope.current_stage = stage_order[0] if stage_order else "end"

        if self.logger:
            self.logger.info("pipeline_started", envelope_id=envelope.envelope_id, stages=stage_order)

        while envelope.current_stage != "end" and not envelope.terminated:
            if not self._can_continue(envelope):
                if self.logger:
                    self.logger.warn("pipeline_bounds_exceeded", reason=envelope.terminal_reason)
                break

            if envelope.current_stage in ("clarification", "confirmation"):
                break

            agent = self.agents.get(envelope.current_stage)
            if not agent:
                envelope.terminated = True
                envelope.terminal_reason = f"Unknown stage: {envelope.current_stage}"
                break

            try:
                envelope = await agent.process(envelope)
            except Exception as e:
                if self.logger:
                    self.logger.error("agent_error", agent=agent.name, error=str(e))
                if agent.config.error_next:
                    envelope.current_stage = agent.config.error_next
                else:
                    envelope.terminated = True
                    envelope.terminal_reason = str(e)
                    break

            if self.persistence and thread_id:
                try:
                    await self.persistence.save_state(thread_id, envelope.to_dict())
                except Exception:
                    pass

        if self.logger:
            self.logger.info("pipeline_completed", envelope_id=envelope.envelope_id, terminated=envelope.terminated)

        return envelope

    async def run_streaming(self, envelope: GenericEnvelope, thread_id: str = "") -> AsyncIterator[Tuple[str, Dict[str, Any]]]:
        """Execute pipeline with streaming outputs."""
        envelope.max_iterations = self.config.max_iterations
        envelope.max_llm_calls = self.config.max_llm_calls
        envelope.max_agent_hops = self.config.max_agent_hops

        stage_order = [a.name for a in self.config.agents]
        envelope.stage_order = stage_order
        envelope.current_stage = stage_order[0] if stage_order else "end"

        while envelope.current_stage != "end" and not envelope.terminated:
            if not self._can_continue(envelope):
                break

            if envelope.current_stage in ("clarification", "confirmation"):
                break

            agent = self.agents.get(envelope.current_stage)
            if not agent:
                envelope.terminated = True
                break

            stage_name = envelope.current_stage

            try:
                envelope = await agent.process(envelope)
            except Exception as e:
                envelope.terminated = True
                envelope.terminal_reason = str(e)
                break

            yield (stage_name, envelope.outputs.get(stage_name, {}))

            if self.persistence and thread_id:
                try:
                    await self.persistence.save_state(thread_id, envelope.to_dict())
                except Exception:
                    pass

        yield ("__end__", {"terminated": envelope.terminated})

    async def resume(self, envelope: GenericEnvelope, thread_id: str = "") -> GenericEnvelope:
        """Resume after interrupt.

        Resume stages are determined by PipelineConfig, not hardcoded.
        This allows different capabilities to define their own resume behavior.
        """
        if envelope.clarification_response:
            envelope.clarification_pending = False
            # Use config-defined resume stage (capability determines this)
            envelope.current_stage = self.config.get_clarification_resume_stage()

        if envelope.confirmation_response is not None:
            envelope.confirmation_pending = False
            if envelope.confirmation_response:
                # Use config-defined resume stage (capability determines this)
                envelope.current_stage = self.config.get_confirmation_resume_stage()
            else:
                envelope.terminated = True
                envelope.terminal_reason = "User denied"
                return envelope

        return await self.run(envelope, thread_id)

    async def get_state(self, thread_id: str) -> Optional[Dict[str, Any]]:
        if not self.persistence:
            return None
        return await self.persistence.load_state(thread_id)

    def _can_continue(self, envelope: GenericEnvelope) -> bool:
        """Check bounds - use Go if available."""
        if self._go_client:
            try:
                result = self._go_client.can_continue(envelope)
                if not result.can_continue:
                    envelope.terminal_reason = result.terminal_reason
                return result.can_continue
            except Exception:
                pass

        if envelope.iteration >= envelope.max_iterations:
            envelope.terminal_reason = "max_iterations_exceeded"
            return False
        if envelope.llm_call_count >= envelope.max_llm_calls:
            envelope.terminal_reason = "max_llm_calls_exceeded"
            return False
        if envelope.agent_hop_count >= envelope.max_agent_hops:
            envelope.terminal_reason = "max_agent_hops_exceeded"
            return False
        return True

    def get_agent(self, name: str) -> Optional[UnifiedAgent]:
        return self.agents.get(name)


# =============================================================================
# FACTORY FUNCTIONS
# =============================================================================

def create_runtime_from_config(
    config: PipelineConfig,
    llm_provider_factory: Optional[LLMProviderFactory] = None,
    tool_executor: Optional[ToolExecutor] = None,
    logger: Optional[Logger] = None,
    persistence: Optional[Persistence] = None,
    prompt_registry: Optional[PromptRegistry] = None,
    use_mock: bool = False,
) -> UnifiedRuntime:
    """Factory to create UnifiedRuntime from pipeline config."""
    return UnifiedRuntime(
        config=config,
        llm_factory=llm_provider_factory,
        tool_executor=tool_executor,
        logger=logger,
        persistence=persistence,
        prompt_registry=prompt_registry,
        use_mock=use_mock,
    )


def create_generic_envelope(
    raw_input: str,
    user_id: str = "",
    session_id: str = "",
    request_id: str = "",
    metadata: Optional[Dict[str, Any]] = None,
) -> GenericEnvelope:
    """Factory to create GenericEnvelope - uses Go if available."""
    try:
        from jeeves_protocols.client import GoClient
        client = GoClient()
        if client.is_available():
            return client.create(
                raw_input=raw_input,
                user_id=user_id,
                session_id=session_id,
                request_id=request_id if request_id else None,
                metadata=metadata,
            )
    except Exception:
        pass

    import uuid
    from jeeves_shared.serialization import utc_now

    return GenericEnvelope(
        envelope_id=str(uuid.uuid4()),
        request_id=request_id or str(uuid.uuid4()),
        user_id=user_id,
        session_id=session_id,
        raw_input=raw_input,
        received_at=utc_now(),
        created_at=utc_now(),
        metadata=metadata or {},
    )


# =============================================================================
# NULL LOGGER
# =============================================================================

class _NullLogger:
    def info(self, event: str, **kwargs) -> None: pass
    def warn(self, event: str, **kwargs) -> None: pass
    def error(self, event: str, **kwargs) -> None: pass
    def bind(self, **kwargs) -> "_NullLogger": return self


# =============================================================================
# OPTIONAL CHECKPOINT
# =============================================================================

@dataclass
class OptionalCheckpoint:
    """Checkpoint configuration for time-travel debugging."""
    enabled: bool = False
    checkpoint_id: Optional[str] = None
    stage_order: int = 0
    metadata: Dict[str, Any] = field(default_factory=dict)
