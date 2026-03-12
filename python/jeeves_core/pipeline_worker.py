"""Pipeline Worker - Kernel-Driven Agent Execution.

PipelineWorker executes agents under kernel control. The kernel owns
orchestration (loop, routing, bounds); Python just runs agents.

The kernel loop is extracted as `run_kernel_loop()` — a free function
that any consumer can call with its own agent dispatch closure.
"""

from __future__ import annotations

import asyncio
import logging
import time
from dataclasses import dataclass
from typing import (
    Any, AsyncIterator, Awaitable, Callable, Dict, Optional,
    Protocol, Tuple, TYPE_CHECKING,
)

from jeeves_core.kernel_client import (
    KernelClient,
    AgentExecutionMetrics,
    OrchestratorInstruction,
    OrchestrationSessionState,
    KernelClientError,
)
from jeeves_core.protocols.types import AgentContext

if TYPE_CHECKING:
    from jeeves_core.runtime.agents import Agent

from jeeves_core.runtime.agents import StreamingAgent
from jeeves_core.protocols.types import TokenStreamMode

logger = logging.getLogger(__name__)


# =============================================================================
# Types
# =============================================================================

class AgentProtocol(Protocol):
    """Protocol for agents that can be executed by the worker."""

    name: str

    async def process(self, context: AgentContext) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        """Execute agent on context. Returns (output, metadata_updates)."""
        ...


@dataclass
class WorkerResult:
    """Result from pipeline execution."""
    outputs: Dict[str, Dict[str, Any]]
    metadata: Dict[str, Any]
    terminated: bool
    terminal_reason: str = ""
    interrupted: bool = False
    interrupt_kind: str = ""


# =============================================================================
# Kernel Orchestration Loop
# =============================================================================


async def run_kernel_loop(
    kernel: KernelClient,
    process_id: str,
    pipeline_config: Dict[str, Any],
    initial_envelope: Dict[str, Any],
    run_agent: Callable[
        [OrchestratorInstruction],
        Awaitable[Tuple[Optional[Dict[str, Any]], bool, str, AgentExecutionMetrics]],
    ],
    *,
    force: bool = False,
    on_event: Optional[Callable[[str, Dict[str, Any]], Awaitable[None]]] = None,
) -> OrchestratorInstruction:
    """Kernel orchestration loop — single source of truth.

    Initializes a session, then loops: get instruction → run agent → report result.
    Caller supplies ``run_agent`` closure containing all domain logic (validation,
    dispatch, metrics). Returns the terminal instruction (TERMINATE or WAIT_INTERRUPT).

    Args:
        kernel: Connected KernelClient.
        process_id: Unique process identifier.
        pipeline_config: Pipeline configuration dict.
        initial_envelope: Initial envelope dict (passed to kernel on init).
        run_agent: async (instruction) → (output_dict, success, error, metrics).
        force: If True, replace any existing session.
        on_event: Optional async callback invoked after each successful agent run.
    """
    from contextlib import nullcontext
    from jeeves_core.observability.metrics import (
        record_kernel_instruction,
        record_pipeline_termination,
        record_agent_duration,
    )
    from jeeves_core.observability.otel_adapter import get_global_otel_adapter
    otel = get_global_otel_adapter()

    def _ipc_span(method: str):
        return otel.start_span("kernel.ipc", attributes={
            "method": method, "process_id": process_id,
        }) if otel and otel.enabled else nullcontext()

    await kernel.initialize_orchestration_session(
        process_id=process_id,
        pipeline_config=pipeline_config,
        envelope=initial_envelope,
        force=force,
    )
    _t_ipc = time.time()
    with _ipc_span("get_next_instruction"):
        instruction = await kernel.get_next_instruction(process_id)
    record_kernel_instruction(instruction.kind, time.time() - _t_ipc)

    while instruction.kind in ("RUN_AGENT", "RUN_AGENTS"):
        if instruction.kind == "RUN_AGENTS":
            # Parallel fan-out: run all agents concurrently
            async def _dispatch_one(name: str):
                single = OrchestratorInstruction(
                    kind="RUN_AGENT",
                    agents=[name],
                    envelope=instruction.envelope,
                    agent_config=instruction.agent_config,
                )
                _t0 = time.time()
                result = await run_agent(single)
                record_agent_duration(name, time.time() - _t0)
                return name, result

            results = await asyncio.gather(
                *(_dispatch_one(name) for name in instruction.agents)
            )
            # Report each result sequentially (kernel expects ordered reports)
            for agent_name, (output, success, error, metrics) in results:
                _t_ipc = time.time()
                with _ipc_span("report_agent_result"):
                    instruction = await kernel.report_agent_result(
                        process_id=process_id,
                        agent_name=agent_name,
                        output=output,
                        metrics=metrics,
                        success=success,
                        error=error,
                    )
                record_kernel_instruction("report_agent_result", time.time() - _t_ipc)
                if instruction.kind == "TERMINATE":
                    record_pipeline_termination(instruction.terminal_reason or "COMPLETED")
                    return instruction
            # After all parallel agents reported, get next instruction
            _t_ipc = time.time()
            with _ipc_span("get_next_instruction"):
                instruction = await kernel.get_next_instruction(process_id)
            record_kernel_instruction(instruction.kind, time.time() - _t_ipc)
        else:
            # Single agent
            agent_name = instruction.agents[0] if instruction.agents else ""
            _t0 = time.time()
            output, success, error, metrics = await run_agent(instruction)
            record_agent_duration(agent_name, time.time() - _t0)
            _t_ipc = time.time()
            with _ipc_span("report_agent_result"):
                instruction = await kernel.report_agent_result(
                    process_id=process_id,
                    agent_name=agent_name,
                    output=output,
                    metrics=metrics,
                    success=success,
                    error=error,
                )
            record_kernel_instruction(instruction.kind, time.time() - _t_ipc)

    # Terminal instruction
    if instruction.kind == "TERMINATE":
        record_pipeline_termination(instruction.terminal_reason or "COMPLETED")

    return instruction


# =============================================================================
# Worker
# =============================================================================

class PipelineWorker:
    """Executes agents under kernel control.

    This worker has NO orchestration logic - it simply:
    1. Initializes a session with the kernel
    2. Gets instructions from the kernel
    3. Builds AgentContext from enriched instructions
    4. Executes agents as instructed
    5. Reports results (output + metadata_updates) back to the kernel
    6. Repeats until TERMINATE or WAIT_INTERRUPT
    """

    def __init__(
        self,
        kernel_client: KernelClient,
        agents: Dict[str, AgentProtocol],
        logger: Optional[logging.Logger] = None,
        persistence: Optional[Any] = None,
        service_registry: Optional[Dict[str, Any]] = None,
    ):
        self._kernel = kernel_client
        self._agents = agents
        self._logger = logger or logging.getLogger(__name__)
        self._persistence = persistence
        self._services = service_registry or {}

    async def execute(
        self,
        process_id: str,
        pipeline_config: Dict[str, Any],
        initial_envelope: Dict[str, Any],
        thread_id: str = "",
        force: bool = False,
    ) -> WorkerResult:
        """Execute pipeline under kernel control.

        Args:
            process_id: Unique process identifier
            pipeline_config: Pipeline configuration dict
            initial_envelope: Initial envelope dict for kernel initialization
            thread_id: Optional thread ID for persistence
            force: If True, terminate any existing session before starting.

        Returns:
            WorkerResult with collected outputs and termination status
        """
        from contextlib import nullcontext
        from jeeves_core.observability.otel_adapter import get_global_otel_adapter

        otel = get_global_otel_adapter()
        span_ctx = otel.start_span(
            "pipeline.execute",
            attributes={
                "process_id": process_id,
                "pipeline_name": pipeline_config.get("name", ""),
            },
        ) if otel else nullcontext()

        async def _run_agent_dispatch(
            instruction: OrchestratorInstruction,
        ) -> Tuple[Optional[Dict[str, Any]], bool, str, AgentExecutionMetrics]:
            agent_name = instruction.agents[0] if instruction.agents else ""
            agent = self._agents.get(agent_name)

            if not agent:
                self._logger.error(
                    "worker_agent_not_found",
                    process_id=process_id,
                    agent_name=agent_name,
                )
                return (None, False, f"Agent not found: {agent_name}", AgentExecutionMetrics())

            output, metadata_updates, success, error, metrics = await self._run_agent(
                instruction=instruction,
                process_id=process_id,
                agent_name=agent_name,
                agent=agent,
            )

            return (output, success, error, metrics)

        with span_ctx:
            try:
                terminal = await run_kernel_loop(
                    self._kernel, process_id, pipeline_config,
                    initial_envelope, _run_agent_dispatch,
                    force=force,
                )
            except KernelClientError as e:
                self._logger.error("worker_kernel_loop_failed", error=str(e))
                return WorkerResult(
                    outputs={},
                    metadata={},
                    terminated=True,
                    terminal_reason=str(e),
                )

            # Read outputs from terminal instruction (populated by kernel)
            terminal_outputs = terminal.outputs or {}

            # Map terminal instruction to WorkerResult
            if terminal.kind == "TERMINATE":
                self._logger.info(
                    "worker_pipeline_terminated",
                    process_id=process_id,
                    reason=terminal.terminal_reason,
                )
                return WorkerResult(
                    outputs=terminal_outputs,
                    metadata={},
                    terminated=True,
                    terminal_reason=terminal.terminal_reason,
                )

            elif terminal.kind == "WAIT_INTERRUPT":
                interrupt_kind = ""
                if terminal.interrupt:
                    interrupt_kind = terminal.interrupt.get("kind", "")
                self._logger.info(
                    "worker_waiting_interrupt",
                    process_id=process_id,
                    interrupt_kind=interrupt_kind,
                )
                return WorkerResult(
                    outputs=terminal_outputs,
                    metadata={},
                    terminated=False,
                    interrupted=True,
                    interrupt_kind=interrupt_kind,
                )

            else:
                self._logger.warning(
                    "worker_unknown_instruction",
                    process_id=process_id,
                    kind=terminal.kind,
                )
                return WorkerResult(
                    outputs=terminal_outputs,
                    metadata={},
                    terminated=True,
                    terminal_reason=f"Unknown instruction kind: {terminal.kind}",
                )

    async def execute_streaming(
        self,
        process_id: str,
        pipeline_config: Dict[str, Any],
        initial_envelope: Dict[str, Any],
        thread_id: str = "",
        force: bool = False,
    ) -> AsyncIterator[Tuple[str, Dict[str, Any]]]:
        """Execute pipeline with streaming outputs.

        Yields (event_name, data) tuples as agents complete.
        Uses run_kernel_loop() — the single orchestration loop.
        """
        events: asyncio.Queue[Optional[Tuple[str, Dict[str, Any]]]] = asyncio.Queue()

        async def _on_event(name: str, data: Dict[str, Any]) -> None:
            await events.put((name, data))

        async def _streaming_dispatch(
            instruction: OrchestratorInstruction,
        ) -> Tuple[Optional[Dict[str, Any]], bool, str, AgentExecutionMetrics]:
            agent_name = instruction.agents[0] if instruction.agents else ""
            agent = self._agents.get(agent_name)

            if not agent:
                return (None, False, f"Agent not found: {agent_name}", AgentExecutionMetrics())

            # Check if agent supports token streaming
            has_streaming = (
                isinstance(agent, StreamingAgent)
                and agent.config.token_stream != TokenStreamMode.OFF
            )

            if has_streaming:
                return await self._run_streaming_agent(
                    instruction, agent, agent_name, _on_event,
                )
            else:
                output, metadata_updates, success, error, metrics = await self._run_agent(
                    instruction=instruction,
                    process_id=process_id,
                    agent_name=agent_name,
                    agent=agent,
                    log_errors=False,
                )
                if success and output:
                    await _on_event(agent_name, output)
                return (output, success, error, metrics)

        async def _run_loop() -> None:
            try:
                terminal = await run_kernel_loop(
                    self._kernel, process_id, pipeline_config,
                    initial_envelope, _streaming_dispatch,
                    force=force,
                )
                if terminal.kind == "TERMINATE":
                    await events.put(("__end__", {
                        "terminated": True,
                        "reason": terminal.terminal_reason,
                    }))
                elif terminal.kind == "WAIT_INTERRUPT":
                    ik = terminal.interrupt.get("kind", "") if terminal.interrupt else ""
                    await events.put(("__interrupt__", {
                        "kind": ik,
                        "interrupt": terminal.interrupt,
                    }))
            except KernelClientError as e:
                await events.put(("__error__", {"error": str(e)}))
            finally:
                await events.put(None)  # sentinel

        task = asyncio.create_task(_run_loop())
        while True:
            item = await events.get()
            if item is None:
                break
            yield item
        await task  # propagate exceptions

    async def _run_streaming_agent(
        self,
        instruction: OrchestratorInstruction,
        agent: AgentProtocol,
        agent_name: str,
        on_event: Callable[[str, Dict[str, Any]], Awaitable[None]],
    ) -> Tuple[Optional[Dict[str, Any]], bool, str, AgentExecutionMetrics]:
        """Run a streaming agent, emitting token events via on_event callback."""
        start_time = time.time()
        context = AgentContext.from_instruction(instruction)
        try:
            async for event_type, event in agent.stream(context):
                await on_event("__token__", {"agent": agent_name, "event": event})
            duration_ms = int((time.time() - start_time) * 1000)
            output, _meta = agent.get_stream_output()
            run_metrics = agent.get_run_metrics() if hasattr(agent, "get_run_metrics") else {}
            metrics = AgentExecutionMetrics(
                llm_calls=run_metrics.get("llm_calls", 0),
                tokens_in=run_metrics.get("tokens_in"),
                tokens_out=run_metrics.get("tokens_out"),
                duration_ms=duration_ms,
            )
            if output:
                await on_event(agent_name, output)
            return (output, True, "", metrics)
        except Exception as e:
            duration_ms = int((time.time() - start_time) * 1000)
            return (None, False, str(e), AgentExecutionMetrics(duration_ms=duration_ms))

    async def _run_agent(
        self,
        instruction: OrchestratorInstruction,
        process_id: str,
        agent_name: str,
        agent: AgentProtocol,
        *,
        log_errors: bool = True,
    ) -> Tuple[Optional[Dict[str, Any]], Optional[Dict[str, Any]], bool, str, AgentExecutionMetrics]:
        """Run one agent execution step.

        Returns: (output, metadata_updates, success, error, metrics)
        """
        start_time = time.time()
        output: Optional[Dict[str, Any]] = None
        metadata_updates: Optional[Dict[str, Any]] = None

        # Build AgentContext from enriched instruction
        context = AgentContext.from_instruction(instruction)

        # Auto-inject retrieval results if agent has RetrievalConfig
        if hasattr(agent, 'config') and hasattr(agent.config, 'retrieval'):
            rc = agent.config.retrieval
            if rc and rc.retriever_key:
                retriever = self._services.get(f"retriever:{rc.retriever_key}")
                if retriever:
                    results = await retriever.retrieve(
                        context.raw_input, limit=rc.limit, filters=rc.filters,
                    )
                    context.metadata[rc.inject_as] = [
                        {"content": r.content, "source": r.source, "score": r.score}
                        for r in results
                    ]

        # Auto-inject conversation history if agent has ConversationConfig
        if hasattr(agent, 'config') and hasattr(agent.config, 'conversation'):
            cc = agent.config.conversation
            if cc and cc.history_key:
                history = self._services.get(f"history:{cc.history_key}")
                if history:
                    turns = await history.get_history(
                        context.session_id, limit=cc.limit,
                    )
                    context.metadata[cc.inject_as] = turns

        try:
            output, metadata_updates = await agent.process(context)
            success = True
            error = ""

        except Exception as e:
            if log_errors:
                self._logger.error(
                    "worker_agent_error",
                    process_id=process_id,
                    agent_name=agent_name,
                    error=str(e),
                )
            success = False
            error = str(e)

        duration_ms = int((time.time() - start_time) * 1000)

        # Get metrics directly from agent if it supports get_run_metrics()
        if hasattr(agent, "get_run_metrics"):
            run_metrics = agent.get_run_metrics()
            tool_calls = 0
            if isinstance(output, dict):
                calls = output.get("tool_calls", [])
                if isinstance(calls, list):
                    tool_calls = len(calls)
            metrics = AgentExecutionMetrics(
                llm_calls=run_metrics.get("llm_calls", 0),
                tool_calls=tool_calls,
                tokens_in=run_metrics.get("tokens_in"),
                tokens_out=run_metrics.get("tokens_out"),
                duration_ms=duration_ms,
            )

            # Wire Prometheus metrics
            from jeeves_core.observability.metrics import record_llm_call, record_llm_tokens
            if metrics.llm_calls > 0:
                record_llm_call(
                    "pipeline", agent_name,
                    "success" if success else "error",
                    duration_ms / 1000,
                )
            if metrics.tokens_in and metrics.tokens_out:
                record_llm_tokens("pipeline", agent_name, metrics.tokens_in, metrics.tokens_out)
        else:
            metrics = AgentExecutionMetrics(duration_ms=duration_ms)
            self._logger.warning("worker_agent_missing_run_metrics", agent_name=agent_name)

        return output, metadata_updates, success, error, metrics


__all__ = [
    "run_kernel_loop",
    "PipelineWorker",
    "WorkerResult",
    "AgentProtocol",
]
