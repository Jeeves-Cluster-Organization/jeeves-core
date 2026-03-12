"""Agent PipelineRunner - Kernel-backed pipeline execution.

Architecture:
    Rust kernel          - Envelope state, bounds checking, pipeline graph
    Python (this file)   - Agent execution, LLM calls, tool execution
    Bridge (client.py)   - TCP+msgpack communication
"""
from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Protocol, AsyncIterator, Tuple, TYPE_CHECKING
from jeeves_core.protocols.interfaces import LLMProviderProtocol

if TYPE_CHECKING:
    from jeeves_core.protocols import AgentConfig, AgentContext, PipelineConfig

from jeeves_core.tools.decorator import _INJECTED_DEPS


def _inject_services(params: dict, metadata: dict) -> None:
    """Auto-inject services from envelope.metadata into tool params."""
    for dep in _INJECTED_DEPS:
        if dep not in params and dep in metadata:
            params[dep] = metadata[dep]


# =============================================================================
# PROTOCOLS
# =============================================================================

class AgentToolExecutor(Protocol):
    """Protocol for tool execution (agent-scoped)."""
    async def execute(self, tool_name: str, params: Dict[str, Any]) -> Dict[str, Any]:
        ...


class AgentLogger(Protocol):
    """Protocol for structured logging (agent-scoped)."""
    def info(self, event: str, **kwargs) -> None: ...
    def warn(self, event: str, **kwargs) -> None: ...
    def error(self, event: str, **kwargs) -> None: ...
    def bind(self, **kwargs) -> "AgentLogger": ...


class AgentPersistence(Protocol):
    """Protocol for state persistence (agent-scoped)."""
    async def save_state(self, thread_id: str, state: Dict[str, Any]) -> None: ...
    async def load_state(self, thread_id: str) -> Optional[Dict[str, Any]]: ...


class AgentPromptRegistry(Protocol):
    """Protocol for prompt retrieval (agent-scoped)."""
    def get(self, key: str, **kwargs) -> str: ...


class PromptRegistry:
    """Concrete prompt registry — stores templates, renders with str.format_map.

    Satisfies AgentPromptRegistry protocol. Capabilities register templates
    at init time; the framework renders at call time.

    Subclass and override _render() for alternative template engines (e.g. Jinja2).
    """

    def __init__(self, templates: Optional[Dict[str, str]] = None):
        self._templates: Dict[str, str] = {}
        if templates:
            for key, template in templates.items():
                self.register(key, template)

    def register(self, key: str, template: str) -> None:
        if not template or not template.strip():
            raise ValueError(f"Empty prompt template for key: {key!r}")
        self._templates[key] = template

    def get(self, key: str, **kwargs) -> str:
        if key not in self._templates:
            raise KeyError(
                f"Prompt key not found: {key!r}. "
                f"Available: {sorted(self._templates.keys())}"
            )
        context = kwargs.get("context") or kwargs or {}
        template = self._templates[key]
        if context:
            return self._render(template, context)
        return template

    def list_keys(self) -> List[str]:
        """Return sorted list of registered template keys."""
        return sorted(self._templates.keys())

    def _render(self, template: str, context: Dict[str, Any]) -> str:
        """Render template with context. Override for Jinja2/other engines."""
        from collections import defaultdict
        return template.format_map(defaultdict(str, context))


class AgentEventContext(Protocol):
    """Protocol for event emission (agent-scoped)."""
    async def emit_agent_started(self, agent_name: str) -> None: ...
    async def emit_agent_completed(self, agent_name: str, status: str, **kwargs) -> None: ...


# =============================================================================
# MESSAGE BUILDERS
# =============================================================================

def _build_messages(prompt: str, context) -> List[Dict[str, Any]]:
    """Convert prompt + context into OpenAI-format message list.

    System message carries the agent prompt template (rendered).
    User message carries the raw user input.
    Accepts either Envelope or AgentContext (both have raw_input).
    """
    messages: List[Dict[str, Any]] = [
        {"role": "system", "content": prompt},
        {"role": "user", "content": context.raw_input},
    ]
    return messages


# =============================================================================
# TYPE ALIASES
# =============================================================================

LLMProviderFactory = Callable[[str], LLMProviderProtocol]
PreProcessHook = Callable[["AgentContext", Optional["Agent"]], "AgentContext"]
PostProcessHook = Callable[["AgentContext", Dict[str, Any], Optional["Agent"]], "AgentContext"]
MockHandler = Callable[["AgentContext"], Dict[str, Any]]


def _schema_to_tool(agent_name: str, schema: Dict[str, Any]) -> Dict[str, Any]:
    """Convert an output_schema to an OpenAI-compatible tool definition."""
    return {
        "type": "function",
        "function": {
            "name": f"{agent_name}_output",
            "description": f"Structured output for {agent_name}",
            "parameters": schema,
        },
    }


# =============================================================================
# AGENT CAPABILITY FLAGS
# =============================================================================

class AgentFeatures:
    """Agent capability flags."""
    LLM = "llm"
    TOOLS = "tools"
    POLICIES = "policies"


# =============================================================================
# UNIFIED AGENT
# =============================================================================

@dataclass
class Agent:
    """Unified agent driven by configuration.

    Agents are configuration-driven - no subclassing required.
    Behavior determined by config flags and hooks.
    """
    config: AgentConfig
    logger: AgentLogger
    llm: Optional[LLMProviderProtocol] = None
    tools: Optional[AgentToolExecutor] = None
    prompt_registry: Optional[AgentPromptRegistry] = None
    event_context: Optional[AgentEventContext] = None
    use_mock: bool = False

    pre_process: Optional[PreProcessHook] = None
    post_process: Optional[PostProcessHook] = None
    mock_handler: Optional[MockHandler] = None
    _last_llm_usage: Optional[Dict[str, int]] = field(default=None, init=False, repr=False)
    _llm_calls_this_run: int = field(default=0, init=False, repr=False)

    @property
    def name(self) -> str:
        return self.config.name

    def set_event_context(self, ctx: AgentEventContext) -> None:
        self.event_context = ctx

    async def process(self, context: "AgentContext") -> Tuple[Dict[str, Any], Dict[str, Any]]:
        """Process context through this agent.

        Args:
            context: AgentContext built from enriched kernel instruction.

        Returns:
            Tuple of (output_dict, metadata_updates).
        """
        from contextlib import nullcontext
        from jeeves_core.observability import get_global_otel_adapter

        self.logger.info(f"{self.name}_started", envelope_id=context.envelope_id)
        self._last_llm_usage = None
        self._llm_calls_this_run = 0

        otel = get_global_otel_adapter()
        span_ctx = otel.start_span(f"agent.{self.name}", attributes={
            "envelope_id": context.envelope_id,
            "has_llm": self.config.has_llm,
        }) if otel and otel.enabled else nullcontext()

        with span_ctx:
            if self.event_context:
                await self.event_context.emit_agent_started(self.name)

            # Pre-process hook (mutates context.metadata dict in place)
            if self.pre_process:
                result = self.pre_process(context, self)
                context = await result if asyncio.iscoroutine(result) else result

            # Get output
            if self.use_mock and self.mock_handler:
                output = self.mock_handler(context)
            elif self.config.tool_dispatch == "auto":
                output = await self._dispatch_tool(context)
            elif self.config.has_llm and self.llm:
                output = await self._call_llm(context)
            else:
                output = context.outputs.get(self.config.output_key, {})

            # Tool execution
            if self.config.has_tools and self.tools and output.get("tool_calls"):
                output = await self._execute_tools(context, output)

            # Debug log output structure for diagnostics
            output_keys = list(output.keys()) if isinstance(output, dict) else []
            self.logger.debug(
                f"{self.name}_output_received",
                envelope_id=context.envelope_id,
                output_keys=output_keys,
                output_type=type(output).__name__,
                has_response_key="response" in output_keys,
                has_final_response_key="final_response" in output_keys,
            )

            # Post-process hook
            if self.post_process:
                result = self.post_process(context, output, self)
                context = await result if asyncio.iscoroutine(result) else result

            self.logger.info(f"{self.name}_completed", envelope_id=context.envelope_id)

            if self.event_context:
                await self.event_context.emit_agent_completed(self.name, status="success")

        return output, dict(context.metadata)

    async def _call_llm(self, context) -> Dict[str, Any]:
        """Call LLM with optional structured output via output_schema."""
        from contextlib import nullcontext
        from jeeves_core.observability.otel_adapter import get_global_otel_adapter
        otel = get_global_otel_adapter()

        if not self.prompt_registry:
            raise ValueError(f"Agent {self.name} requires prompt_registry")

        prompt_key = self.config.prompt_key or f"{context.metadata.get('pipeline', 'default')}.{self.name}"
        prompt_ctx = self._build_prompt_context(context)
        prompt = self.prompt_registry.get(prompt_key, context=prompt_ctx)

        # Build messages from prompt context
        messages = _build_messages(prompt, context)

        # Build options
        options: Dict[str, Any] = {}
        if self.config.temperature is not None:
            options["temperature"] = self.config.temperature
        if self.config.max_tokens is not None:
            options["num_predict"] = self.config.max_tokens

        # If output_schema is set, use tool_choice to enforce structured output
        if self.config.output_schema:
            options["tools"] = [_schema_to_tool(self.config.name, self.config.output_schema)]
            options["tool_choice"] = {"type": "function", "function": {"name": f"{self.config.name}_output"}}

        # Call LLM provider's chat() method
        llm_span = otel.start_span("agent.llm_call", attributes={
            "agent": self.name,
            "model_role": self.config.model_role or "",
            "prompt_key": prompt_key,
            "has_tools": bool(options.get("tools")),
        }) if otel and otel.enabled else nullcontext()
        with llm_span:
            result, usage = await self.llm.chat_with_usage(
                model="",
                messages=messages,
                options=options,
            )

        self._last_llm_usage = {
            "tokens_in": usage.prompt_tokens,
            "tokens_out": usage.completion_tokens,
        }
        self._llm_calls_this_run += 1

        # If tool_calls present (structured output via schema or explicit tools)
        if result.tool_calls:
            return result.tool_calls[0].arguments

        # No schema -> text mode: wrap raw content
        return {"response": result.content}

    async def _execute_tools(self, context, output: Dict[str, Any]) -> Dict[str, Any]:
        """Execute tool calls from LLM output."""
        from contextlib import nullcontext
        from jeeves_core.observability.otel_adapter import get_global_otel_adapter
        otel = get_global_otel_adapter()

        tool_calls = output.get("tool_calls", [])
        results = []
        tool_span = otel.start_span("agent.tool_execution", attributes={
            "agent": self.name,
            "tool_count": len(tool_calls),
        }) if otel and otel.enabled else nullcontext()

        with tool_span:
            for call in tool_calls:
                tool_name = call.get("name")
                params = call.get("params", {})

                if self.config.allowed_tools is not None and tool_name not in self.config.allowed_tools:
                    results.append({"tool": tool_name, "error": f"Agent '{self.name}' not allowed to call '{tool_name}'"})
                    continue

                _inject_services(params, context.metadata)

                try:
                    result = await self.tools.execute(tool_name, params)
                    results.append({"tool": tool_name, "result": result})
                except Exception as e:
                    results.append({"tool": tool_name, "error": str(e)})

            output["tool_results"] = results
        return output

    async def _dispatch_tool(self, context: "AgentContext") -> Dict[str, Any]:
        """Deterministic tool dispatch — framework-handled."""
        source_key = self.config.tool_source_agent or ""
        source = context.outputs.get(source_key, {})
        if not source:
            source = context.metadata  # fallback: metadata-based selection

        tool_name = source.get(self.config.tool_name_field)
        params = dict(source.get(self.config.tool_params_field, {}))

        if not tool_name:
            return {"status": "skipped", "message": "No tool selected"}
        if not self.tools:
            return {"status": "error", "error": f"No tool executor for {self.name}"}

        if self.config.allowed_tools is not None and tool_name not in self.config.allowed_tools:
            return {"status": "error", "tool": tool_name,
                    "error": f"Agent '{self.name}' not allowed to call '{tool_name}'"}

        _inject_services(params, context.metadata)

        try:
            result = await self.tools.execute(tool_name, params)
            return {"status": "success", "tool": tool_name, "result": result}
        except Exception as e:
            return {"status": "error", "tool": tool_name, "error": str(e)}

    def get_run_metrics(self) -> Dict[str, Any]:
        """Return metrics from the most recent process()/stream() call."""
        metrics: Dict[str, Any] = {"llm_calls": self._llm_calls_this_run}
        if self._last_llm_usage:
            metrics["tokens_in"] = self._last_llm_usage.get("tokens_in", 0)
            metrics["tokens_out"] = self._last_llm_usage.get("tokens_out", 0)
        return metrics

    def _build_prompt_context(self, context: "AgentContext") -> Dict[str, Any]:
        """Build context for prompt template interpolation."""
        return build_prompt_context(context, self.name)


def build_prompt_context(context: "AgentContext", agent_name: str) -> Dict[str, Any]:
    """Build context for prompt template interpolation.

    Context is built generically from:
    1. Base context fields (raw_input, user_id, session_id)
    2. All prior agent outputs (flattened — both raw and field-level)
    3. Metadata (capability-provided overrides and defaults)
    """
    import os
    repo_path = os.environ.get("REPO_PATH", "/workspace")

    prompt_ctx: Dict[str, Any] = {
        "raw_input": context.raw_input,
        "user_input": context.raw_input,
        "user_id": context.user_id,
        "session_id": context.session_id,
        "user_query": context.raw_input,
        "repo_path": repo_path,
        "session_state": f"Session: {context.session_id}",
        "role_description": f"As the {agent_name} agent in this pipeline stage.",
    }

    # Generically flatten all prior agent outputs into context.
    _base_keys = frozenset(prompt_ctx.keys())
    for output_key, output_value in context.outputs.items():
        prompt_ctx[output_key] = output_value
        if isinstance(output_value, dict):
            for field_key, field_value in output_value.items():
                if field_key not in _base_keys:
                    prompt_ctx[field_key] = field_value

    # Metadata last — capabilities inject defaults and overrides here
    prompt_ctx.update(context.metadata)

    return prompt_ctx


@dataclass
class StreamingAgent(Agent):
    """Agent with streaming token emission.

    Extends Agent with stream(), _call_llm_stream(), and citation extraction.
    Use when pipeline stages have token_stream != OFF.
    """
    _stream_output: Dict[str, Any] = field(default_factory=dict, init=False, repr=False)

    async def stream(self, context: "AgentContext") -> AsyncIterator[Tuple[str, Any]]:
        """Streaming execution (token/event emission).

        Yields:
            Tuple[str, Any]: (event_type, event_data) pairs.
        """
        from jeeves_core.protocols import PipelineEvent
        from jeeves_core.protocols import TokenStreamMode

        self.logger.info(f"{self.name}_stream_started", envelope_id=context.envelope_id)
        self._last_llm_usage = None
        self._llm_calls_this_run = 0
        self._stream_output = {}

        # Pre-process hook
        if self.pre_process:
            result = self.pre_process(context, self)
            context = await result if asyncio.iscoroutine(result) else result

        # Determine if tokens should be authoritative
        is_authoritative = self.config.token_stream == TokenStreamMode.AUTHORITATIVE

        # Stream tokens if enabled
        if self.config.token_stream != TokenStreamMode.OFF and self.config.has_llm and self.llm:
            accumulated = ""
            async for token in self._call_llm_stream(context):
                accumulated += token
                event = PipelineEvent(
                    type="token",
                    stage=self.name,
                    data={"token": token},
                    debug=not is_authoritative,
                )
                yield ("token", event)

            if is_authoritative:
                self._stream_output = {
                    "response": accumulated,
                    "citations": _extract_citations(accumulated),
                }
            else:
                self._stream_output = await self._call_llm(context)

            # Post-process hook
            if self.post_process:
                result = self.post_process(context, self._stream_output, self)
                context = await result if asyncio.iscoroutine(result) else result
        else:
            # No streaming - use regular process
            self._stream_output, _ = await self.process(context)

        self.logger.info(f"{self.name}_stream_completed", envelope_id=context.envelope_id)

    def get_stream_output(self) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        """Get output from last stream() call. Used by worker after streaming."""
        return self._stream_output, {}

    async def _call_llm_stream(self, context: "AgentContext") -> AsyncIterator[str]:
        """Stream authoritative tokens (for TEXT mode with AUTHORITATIVE tokens)."""
        if self.config.output_schema is not None:
            raise ValueError(
                "_call_llm_stream() cannot stream structured output. "
                "Set output_schema=None for streaming agents."
            )

        if not self.prompt_registry:
            raise ValueError(f"Agent {self.name} requires prompt_registry")

        prompt_key = self.config.streaming_prompt_key
        if not prompt_key:
            base_key = self.config.prompt_key or f"{context.metadata.get('pipeline', 'default')}.{self.name}"
            prompt_key = f"{base_key}_streaming"

        prompt_ctx = self._build_prompt_context(context)
        prompt = self.prompt_registry.get(prompt_key, context=prompt_ctx)
        messages = _build_messages(prompt, context)

        options: Dict[str, Any] = {}
        if self.config.temperature is not None:
            options["temperature"] = self.config.temperature
        if self.config.max_tokens is not None:
            options["num_predict"] = self.config.max_tokens
        if self.config.generation:
            options.update(self.config.generation.to_dict())

        async for chunk in self.llm.chat_stream(model="", messages=messages, options=options):
            if chunk.text:
                yield chunk.text

        self._llm_calls_this_run += 1


def _extract_citations(text: str) -> List[str]:
    """Extract inline citations from streaming response.

    Inline citations are v0 best-effort and display-only.
    Not for governance/verification.
    """
    import re
    pattern = r'\[([^\]]+)\]'
    matches = re.findall(pattern, text)
    seen = set()
    citations = []
    for match in matches:
        if match not in seen:
            seen.add(match)
            citations.append(match)
    return citations


# =============================================================================
# RUNTIME
# =============================================================================

@dataclass
class PipelineRunner:
    """Pipeline runtime - orchestrates agent execution.

    Uses Rust kernel for envelope state/bounds when available.
    Uses Python for agent execution, LLM, tools.
    """
    config: PipelineConfig
    llm_factory: Optional[LLMProviderFactory] = None
    tool_executor: Optional[AgentToolExecutor] = None
    logger: Optional[AgentLogger] = None
    persistence: Optional[AgentPersistence] = None
    prompt_registry: Optional[AgentPromptRegistry] = None
    use_mock: bool = False

    agents: Dict[str, Agent] = field(default_factory=dict)
    event_context: Optional[AgentEventContext] = None
    _initialized: bool = field(default=False)

    def __post_init__(self):
        if not self._initialized:
            self._build_agents()
            self._initialized = True

    def _build_agents(self):
        """Build agents from pipeline config."""
        for agent_config in self.config.agents:
            if agent_config.has_llm and not self.llm_factory:
                raise ValueError(
                    f"Agent '{agent_config.name}' requires LLM but no llm_factory provided "
                    f"to pipeline '{self.config.name}'"
                )

            llm = None
            if agent_config.has_llm and self.llm_factory and agent_config.model_role:
                llm = self.llm_factory(agent_config.model_role)

            tools = self.tool_executor if (agent_config.has_tools or agent_config.tool_dispatch) else None

            agent = Agent(
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

    def set_event_context(self, ctx: AgentEventContext) -> None:
        self.event_context = ctx
        for agent in self.agents.values():
            agent.set_event_context(ctx)

    async def get_state(self, thread_id: str) -> Optional[Dict[str, Any]]:
        if not self.persistence:
            return None
        return await self.persistence.load_state(thread_id)

    def get_agent(self, name: str) -> Agent:
        """Get agent by name. Raises ValueError on miss."""
        if name not in self.agents:
            raise ValueError(
                f"Agent '{name}' not found in pipeline '{self.config.name}'. "
                f"Available: {list(self.agents.keys())}"
            )
        return self.agents[name]


# =============================================================================
# FACTORY FUNCTIONS
# =============================================================================

def create_pipeline_runner(
    config: PipelineConfig,
    llm_provider_factory: Optional[LLMProviderFactory] = None,
    tool_executor: Optional[AgentToolExecutor] = None,
    logger: Optional[AgentLogger] = None,
    persistence: Optional[AgentPersistence] = None,
    prompt_registry: Optional[AgentPromptRegistry] = None,
    use_mock: bool = False,
) -> PipelineRunner:
    """Factory to create PipelineRunner from pipeline config."""
    return PipelineRunner(
        config=config,
        llm_factory=llm_provider_factory,
        tool_executor=tool_executor,
        logger=logger,
        persistence=persistence,
        prompt_registry=prompt_registry,
        use_mock=use_mock,
    )


class PipelineRegistry:
    """Dispatches to the right PipelineRunner by pipeline name.

    Eliminates agent name collisions across pipelines by scoping
    lookups to pipeline_name + agent_name.
    """

    def __init__(self):
        self._runners: Dict[str, PipelineRunner] = {}

    def register(self, pipeline_name: str, runner: PipelineRunner) -> None:
        """Register a runner under a pipeline name. Raises on duplicate."""
        if pipeline_name in self._runners:
            raise ValueError(f"Pipeline already registered: {pipeline_name}")
        self._runners[pipeline_name] = runner

    def get_runner(self, pipeline_name: str) -> PipelineRunner:
        """Get runner by pipeline name. Raises ValueError on miss."""
        if pipeline_name not in self._runners:
            raise ValueError(
                f"Unknown pipeline: {pipeline_name}. "
                f"Registered: {list(self._runners.keys())}"
            )
        return self._runners[pipeline_name]

    def get_agent(self, pipeline_name: str, agent_name: str) -> Agent:
        """Get agent from a specific pipeline. Raises ValueError on miss."""
        runner = self.get_runner(pipeline_name)
        agent = runner.agents.get(agent_name)
        if agent is None:
            raise ValueError(
                f"Agent '{agent_name}' not found in pipeline '{pipeline_name}'. "
                f"Available: {list(runner.agents.keys())}"
            )
        return agent

    def has_agent(self, pipeline_name: str, agent_name: str) -> bool:
        """Check if a pipeline has a specific agent (for LLM vs deterministic routing)."""
        runner = self._runners.get(pipeline_name)
        return runner is not None and agent_name in runner.agents

    def list_pipelines(self) -> list:
        return list(self._runners.keys())


# =============================================================================
# NULL LOGGER
# =============================================================================

class _NullLogger:
    def info(self, event: str, **kwargs) -> None: pass
    def warn(self, event: str, **kwargs) -> None: pass
    def error(self, event: str, **kwargs) -> None: pass
    def debug(self, event: str, **kwargs) -> None: pass
    def warning(self, event: str, **kwargs) -> None: pass
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
