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
    from jeeves_core.protocols import Envelope, AgentConfig, PipelineConfig

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
# TYPE ALIASES
# =============================================================================

LLMProviderFactory = Callable[[str], LLMProviderProtocol]
PreProcessHook = Callable[["Envelope", Optional["Agent"]], "Envelope"]
PostProcessHook = Callable[["Envelope", Dict[str, Any], Optional["Agent"]], "Envelope"]
MockHandler = Callable[["Envelope"], Dict[str, Any]]


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

    async def process(self, envelope: Envelope) -> Envelope:
        """Process envelope through this agent."""
        from contextlib import nullcontext
        from jeeves_core.observability import get_global_otel_adapter

        self.logger.info(f"{self.name}_started", envelope_id=envelope.envelope_id)
        self._last_llm_usage = None
        self._llm_calls_this_run = 0

        otel = get_global_otel_adapter()
        span_ctx = otel.start_span(f"agent.{self.name}", attributes={
            "envelope_id": envelope.envelope_id,
            "has_llm": self.config.has_llm,
        }) if otel and otel.enabled else nullcontext()

        with span_ctx:
            if self.event_context:
                await self.event_context.emit_agent_started(self.name)

            # Pre-process hook
            if self.pre_process:
                result = self.pre_process(envelope, self)
                envelope = await result if asyncio.iscoroutine(result) else result

            # Get output
            if self.use_mock and self.mock_handler:
                output = self.mock_handler(envelope)
            elif self.config.tool_dispatch == "auto":
                output = await self._dispatch_tool(envelope)
            elif self.config.has_llm and self.llm:
                output = await self._call_llm(envelope)
            else:
                output = envelope.outputs.get(self.config.output_key, {})

            # Tool execution
            if self.config.has_tools and self.tools and output.get("tool_calls"):
                output = await self._execute_tools(envelope, output)

            # Debug log output structure for diagnostics
            output_keys = list(output.keys()) if isinstance(output, dict) else []
            self.logger.debug(
                f"{self.name}_output_received",
                envelope_id=envelope.envelope_id,
                output_keys=output_keys,
                output_type=type(output).__name__,
                has_response_key="response" in output_keys,
                has_final_response_key="final_response" in output_keys,
            )

            # Validate required output fields
            if self.config.required_output_fields and isinstance(output, dict):
                missing_fields = [f for f in self.config.required_output_fields if f not in output]
                if missing_fields:
                    self.logger.warning(
                        f"{self.name}_missing_required_fields",
                        envelope_id=envelope.envelope_id,
                        missing_fields=missing_fields,
                        required_fields=self.config.required_output_fields,
                        actual_fields=output_keys,
                    )

            # Post-process hook
            if self.post_process:
                result = self.post_process(envelope, output, self)
                envelope = await result if asyncio.iscoroutine(result) else result

            # Store output
            envelope.outputs[self.config.output_key] = output
            # Note: agent_hop_count and current_stage are now managed by the kernel orchestrator

            self.logger.info(f"{self.name}_completed", envelope_id=envelope.envelope_id)

            if self.event_context:
                await self.event_context.emit_agent_completed(self.name, status="success")

        return envelope

    async def _call_llm(self, envelope: Envelope) -> Dict[str, Any]:
        """Call LLM with prompt from registry."""
        if not self.prompt_registry:
            raise ValueError(f"Agent {self.name} requires prompt_registry")

        prompt_key = self.config.prompt_key or f"{envelope.metadata.get('pipeline', 'default')}.{self.name}"

        context = self._build_prompt_context(envelope)

        prompt = self.prompt_registry.get(prompt_key, context=context)

        # Build options dict for LLM provider
        options = {}
        if self.config.temperature is not None:
            options["temperature"] = self.config.temperature
        if self.config.max_tokens is not None:
            options["num_predict"] = self.config.max_tokens  # llama-server uses num_predict

        # Call LLM provider's generate() method
        # model is empty string since llama-server loads a single model
        response: str
        usage: Optional[Dict[str, int]] = None
        if hasattr(self.llm, "generate_with_usage"):
            response, usage = await self.llm.generate_with_usage(
                model="",
                prompt=prompt,
                options=options,
            )
        else:
            response = await self.llm.generate(model="", prompt=prompt, options=options)

        if isinstance(usage, dict):
            prompt_tokens = usage.get("prompt_tokens")
            completion_tokens = usage.get("completion_tokens")
            if isinstance(prompt_tokens, int) and isinstance(completion_tokens, int):
                self._last_llm_usage = {
                    "tokens_in": prompt_tokens,
                    "tokens_out": completion_tokens,
                }
        else:
            if usage is not None:
                self.logger.debug("agent_usage_parse_skipped", agent=self.config.name, usage_type=type(usage).__name__)
        self._llm_calls_this_run += 1

        # Use JSONRepairKit for robust parsing of LLM output
        # Handles: code fences, text + embedded JSON, trailing commas, single quotes, etc.
        # This ensures P1 (Accuracy First) by properly extracting structured output
        from jeeves_core.utils import JSONRepairKit

        result = JSONRepairKit.parse_lenient(response)
        if result is not None:
            return result
        self.logger.warning("agent_json_parse_failed", agent=self.config.name, envelope_id=envelope.envelope_id, response_preview=response[:200])
        return {"response": response}

    async def _execute_tools(self, envelope: Envelope, output: Dict[str, Any]) -> Dict[str, Any]:
        """Execute tool calls from LLM output."""
        tool_calls = output.get("tool_calls", [])
        results = []
        denied_tools = []

        for call in tool_calls:
            tool_name = call.get("name")
            params = call.get("params", {})

            if not self._can_access_tool(tool_name):
                self.logger.warning(
                    "tool_access_denied",
                    agent=self.name,
                    tool=tool_name,
                    envelope_id=envelope.envelope_id,
                )
                denied_tools.append(tool_name)
                results.append({"tool": tool_name, "error": f"Access denied for {self.name}"})
                continue

            _inject_services(params, envelope.metadata)

            try:
                result = await self.tools.execute(tool_name, params)
                results.append({"tool": tool_name, "result": result})
            except Exception as e:
                results.append({"tool": tool_name, "error": str(e)})

        output["tool_results"] = results

        if denied_tools:
            raise PermissionError(
                f"Agent '{self.name}' denied access to tools: {denied_tools}"
            )

        return output

    async def _dispatch_tool(self, envelope: Envelope) -> Dict[str, Any]:
        """Deterministic tool dispatch — framework-handled."""
        source_key = self.config.tool_source_agent or ""
        source = envelope.outputs.get(source_key, {})
        if not source:
            source = envelope.metadata  # fallback: metadata-based selection

        tool_name = source.get(self.config.tool_name_field)
        params = dict(source.get(self.config.tool_params_field, {}))

        if not tool_name:
            return {"status": "skipped", "message": "No tool selected"}
        if not self.tools:
            return {"status": "error", "error": f"No tool executor for {self.name}"}

        _inject_services(params, envelope.metadata)

        try:
            result = await self.tools.execute(tool_name, params)
            return {"status": "success", "tool": tool_name, "result": result}
        except Exception as e:
            return {"status": "error", "tool": tool_name, "error": str(e)}

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

    def get_run_metrics(self) -> Dict[str, Any]:
        """Return metrics from the most recent process()/stream() call."""
        metrics: Dict[str, Any] = {"llm_calls": self._llm_calls_this_run}
        if self._last_llm_usage:
            metrics["tokens_in"] = self._last_llm_usage.get("tokens_in", 0)
            metrics["tokens_out"] = self._last_llm_usage.get("tokens_out", 0)
        return metrics

    async def stream(self, envelope: Envelope) -> AsyncIterator[Tuple[str, Any]]:
        """Streaming execution (token/event emission).

        Behavior depends on config:
        - token_stream=OFF: No token events
        - token_stream=DEBUG: Emit debug tokens (debug=True)
        - token_stream=AUTHORITATIVE: Emit authoritative tokens (debug=False)

        Yields:
            Tuple[str, Any]: (event_type, event_data) pairs
        """
        from jeeves_core.protocols import PipelineEvent
        from jeeves_core.protocols import TokenStreamMode

        self.logger.info(f"{self.name}_stream_started", envelope_id=envelope.envelope_id)
        self._last_llm_usage = None
        self._llm_calls_this_run = 0

        # Pre-process hook
        if self.pre_process:
            result = self.pre_process(envelope, self)
            envelope = await result if asyncio.iscoroutine(result) else result

        # Determine if tokens should be authoritative
        is_authoritative = self.config.token_stream == TokenStreamMode.AUTHORITATIVE

        # Stream tokens if enabled
        if self.config.token_stream != TokenStreamMode.OFF and self.config.has_llm and self.llm:
            accumulated = ""
            async for token in self._call_llm_stream(envelope):
                accumulated += token
                # Emit token event
                event = PipelineEvent(
                    type="token",
                    stage=self.name,
                    data={"token": token},
                    debug=not is_authoritative  # Debug if not authoritative
                )
                yield ("token", event)

            # Finalize after streaming completes
            if is_authoritative:
                await self.finalize_stream(envelope, accumulated)
            else:
                # For debug mode, still call regular _call_llm for canonical output
                output = await self._call_llm(envelope)
                envelope.outputs[self.config.output_key] = output

            # Post-process hook (same as process()) — required for routing
            # rule evaluation after streaming completes
            if self.post_process:
                output = envelope.outputs.get(self.config.output_key, {})
                result = self.post_process(envelope, output, self)
                envelope = await result if asyncio.iscoroutine(result) else result
        else:
            # No streaming - use regular process (includes post_process)
            await self.process(envelope)

        # Note: agent_hop_count and current_stage are now managed by the kernel orchestrator
        self.logger.info(f"{self.name}_stream_completed", envelope_id=envelope.envelope_id)

    async def _call_llm_stream(self, envelope: Envelope) -> AsyncIterator[str]:
        """Stream authoritative tokens (for TEXT mode with AUTHORITATIVE tokens)."""
        from jeeves_core.protocols import AgentOutputMode

        if self.config.output_mode != AgentOutputMode.TEXT:
            raise ValueError("_call_llm_stream() requires output_mode=TEXT")

        if not self.prompt_registry:
            raise ValueError(f"Agent {self.name} requires prompt_registry")

        # Use streaming prompt key if provided, otherwise append "_streaming"
        prompt_key = self.config.streaming_prompt_key
        if not prompt_key:
            base_key = self.config.prompt_key or f"{envelope.metadata.get('pipeline', 'default')}.{self.name}"
            prompt_key = f"{base_key}_streaming"

        # Build context (same as _call_llm)
        context = self._build_prompt_context(envelope)
        prompt = self.prompt_registry.get(prompt_key, context=context)

        # Build options
        options = {}
        if self.config.temperature is not None:
            options["temperature"] = self.config.temperature
        if self.config.max_tokens is not None:
            options["num_predict"] = self.config.max_tokens

        # Merge GenerationParams (K8s-style spec)
        if self.config.generation:
            options.update(self.config.generation.to_dict())

        # Stream tokens directly from LLM
        async for chunk in self.llm.generate_stream(model="", prompt=prompt, options=options):
            if chunk.text:
                yield chunk.text

        self._llm_calls_this_run += 1

    def _build_prompt_context(self, envelope: Envelope) -> Dict[str, Any]:
        """Build context for prompt template interpolation.

        Context is built generically from:
        1. Base envelope fields (raw_input, user_id, session_id)
        2. All prior agent outputs (flattened — both raw and field-level)
        3. Envelope metadata (capability-provided overrides and defaults)

        Capabilities control prompt templates and agent outputs, so they
        align template placeholders with actual output keys. No hardcoded
        agent names or domain-specific defaults live here.
        """
        import os
        repo_path = os.environ.get("REPO_PATH", "/workspace")

        context: Dict[str, Any] = {
            "raw_input": envelope.raw_input,
            "user_input": envelope.raw_input,
            "user_id": envelope.user_id,
            "session_id": envelope.session_id,
            "user_query": envelope.raw_input,
            "repo_path": repo_path,
            "session_state": f"Session: {envelope.session_id}",
            "role_description": f"As the {self.name} agent in this pipeline stage.",
        }

        # Generically flatten all prior agent outputs into context.
        # Each output is available both as its raw key (e.g., context["planner"])
        # and, if it's a dict, its fields are promoted to top-level
        # (e.g., context["normalized_query"] from planner output).
        _base_keys = frozenset(context.keys())
        for output_key, output_value in envelope.outputs.items():
            context[output_key] = output_value
            if isinstance(output_value, dict):
                for field_key, field_value in output_value.items():
                    if field_key not in _base_keys:
                        context[field_key] = field_value

        # Metadata last — capabilities inject defaults and overrides here
        context.update(envelope.metadata)

        return context

    async def finalize_stream(self, envelope: Envelope, accumulated_text: str):
        """Write canonical output after streaming completes."""
        envelope.outputs[self.config.output_key] = {
            "response": accumulated_text,
            "citations": self._extract_citations(accumulated_text),
        }

    def _extract_citations(self, text: str) -> List[str]:
        """Extract inline citations from streaming response.

        Inline citations are v0 best-effort and display-only.
        Not for governance/verification.
        """
        import re
        # Match [Source Name] pattern
        pattern = r'\[([^\]]+)\]'
        matches = re.findall(pattern, text)
        # Deduplicate while preserving order
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
        seen_output_keys: Dict[str, str] = {}
        for agent_config in self.config.agents:
            ok = agent_config.output_key
            if ok in seen_output_keys:
                raise ValueError(
                    f"Duplicate output_key '{ok}': agents '{seen_output_keys[ok]}' and "
                    f"'{agent_config.name}' in pipeline '{self.config.name}'"
                )
            seen_output_keys[ok] = agent_config.name
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


def create_envelope(
    raw_input: str,
    request_context: "RequestContext",
    metadata: Optional[Dict[str, Any]] = None,
) -> "Envelope":
    """Factory to create Envelope."""
    import uuid
    from jeeves_core.utils import utc_now
    from jeeves_core.protocols import Envelope  # Runtime import to avoid circular dependency

    # Import RequestContext at runtime to avoid circular dependency
    from jeeves_core.protocols import RequestContext as RC
    if not isinstance(request_context, RC):
        raise TypeError("request_context must be a RequestContext instance")

    return Envelope(
        request_context=request_context,
        envelope_id=str(uuid.uuid4()),
        request_id=request_context.request_id,
        user_id=request_context.user_id or "",
        session_id=request_context.session_id or "",
        raw_input=raw_input,
        received_at=utc_now(),
        metadata=metadata or {},
    )


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
