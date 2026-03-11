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
from dataclasses import dataclass, field
from typing import (
    Any, AsyncIterator, Awaitable, Callable, Dict, List, Optional,
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
    """
    from jeeves_core.observability.metrics import (
        record_kernel_instruction,
        record_pipeline_termination,
        record_agent_duration,
    )

    await kernel.initialize_orchestration_session(
        process_id=process_id,
        pipeline_config=pipeline_config,
        envelope=initial_envelope,
    )
    instruction = await kernel.get_next_instruction(process_id)
    record_kernel_instruction(instruction.kind)

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
                instruction = await kernel.report_agent_result(
                    process_id=process_id,
                    agent_name=agent_name,
                    output=output,
                    metrics=metrics,
                    success=success,
                    error=error,
                )
                if instruction.kind == "TERMINATE":
                    record_pipeline_termination(instruction.terminal_reason or "COMPLETED")
                    return instruction
            # After all parallel agents reported, get next instruction
            instruction = await kernel.get_next_instruction(process_id)
            record_kernel_instruction(instruction.kind)
        else:
            # Single agent
            agent_name = instruction.agents[0] if instruction.agents else ""
            _t0 = time.time()
            output, success, error, metrics = await run_agent(instruction)
            record_agent_duration(agent_name, time.time() - _t0)
            instruction = await kernel.report_agent_result(
                process_id=process_id,
                agent_name=agent_name,
                output=output,
                metrics=metrics,
                success=success,
                error=error,
            )
            record_kernel_instruction(instruction.kind)

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
    ):
        self._kernel = kernel_client
        self._agents = agents
        self._logger = logger or logging.getLogger(__name__)
        self._persistence = persistence

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

        if force:
            try:
                await self._kernel.terminate_process(process_id, reason="force_replace")
            except KernelClientError:
                pass  # session might not exist

        otel = get_global_otel_adapter()
        span_ctx = otel.start_span(
            "pipeline.execute",
            attributes={
                "process_id": process_id,
                "pipeline_name": pipeline_config.get("name", ""),
            },
        ) if otel else nullcontext()

        # Accumulated outputs across agent runs
        all_outputs: Dict[str, Dict[str, Any]] = {}
        last_metadata: Dict[str, Any] = {}

        async def _run_agent_dispatch(
            instruction: OrchestratorInstruction,
        ) -> Tuple[Optional[Dict[str, Any]], bool, str, AgentExecutionMetrics]:
            nonlocal last_metadata
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

            if success and output:
                all_outputs[agent_name] = output
            last_metadata = metadata_updates or {}

            return (output, success, error, metrics)

        with span_ctx:
            try:
                terminal = await run_kernel_loop(
                    self._kernel, process_id, pipeline_config,
                    initial_envelope, _run_agent_dispatch,
                )
            except KernelClientError as e:
                self._logger.error("worker_kernel_loop_failed", error=str(e))
                return WorkerResult(
                    outputs=all_outputs,
                    metadata=last_metadata,
                    terminated=True,
                    terminal_reason=str(e),
                )

            # Map terminal instruction to WorkerResult
            if terminal.kind == "TERMINATE":
                self._logger.info(
                    "worker_pipeline_terminated",
                    process_id=process_id,
                    reason=terminal.terminal_reason,
                )
                return WorkerResult(
                    outputs=all_outputs,
                    metadata=last_metadata,
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
                    outputs=all_outputs,
                    metadata=last_metadata,
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
                    outputs=all_outputs,
                    metadata=last_metadata,
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

        Yields (agent_name, output) tuples as agents complete.
        """
        # Initialize session with kernel
        try:
            session_state = await self._kernel.initialize_orchestration_session(
                process_id=process_id,
                pipeline_config=pipeline_config,
                envelope=initial_envelope,
                force=force,
            )
        except KernelClientError as e:
            if e.code == "ALREADY_EXISTS":
                self._logger.error(
                    "worker_session_already_exists",
                    process_id=process_id,
                    hint="Use force=True to replace existing session",
                )
            elif e.code == "TIMEOUT":
                self._logger.error(
                    "worker_session_deadline_exceeded",
                    process_id=process_id,
                )
            else:
                self._logger.error("worker_session_init_failed", error=str(e))
            yield ("__error__", {"error": str(e)})
            return

        # Main execution loop
        while True:
            try:
                instruction = await self._kernel.get_next_instruction(process_id)
            except KernelClientError as e:
                yield ("__error__", {"error": str(e)})
                return

            if instruction.kind == "TERMINATE":
                yield ("__end__", {
                    "terminated": True,
                    "reason": instruction.terminal_reason,
                })
                return

            elif instruction.kind == "WAIT_INTERRUPT":
                interrupt_kind = ""
                if instruction.interrupt:
                    interrupt_kind = instruction.interrupt.get("kind", "")
                yield ("__interrupt__", {
                    "kind": interrupt_kind,
                    "interrupt": instruction.interrupt,
                })
                return

            elif instruction.kind in ("RUN_AGENT", "RUN_AGENTS"):
                agents_to_run = instruction.agents if instruction.agents else []

                for agent_name in agents_to_run:
                    agent = self._agents.get(agent_name)

                    if not agent:
                        await self._kernel.report_agent_result(
                            process_id=process_id,
                            agent_name=agent_name,
                            success=False,
                            error=f"Agent not found: {agent_name}",
                        )
                        continue

                    # Check if agent supports token streaming
                    has_streaming = (
                        hasattr(agent, 'config')
                        and hasattr(agent.config, 'token_stream')
                        and hasattr(agent.config.token_stream, 'value')
                        and agent.config.token_stream.value != "off"
                    )

                    if has_streaming and hasattr(agent, 'stream'):
                        # Streaming path: yield tokens, then report final output
                        start_time = time.time()
                        context = AgentContext.from_instruction(instruction)

                        try:
                            async for event_type, event in agent.stream(context):
                                yield ("__token__", {"agent": agent_name, "event": event})

                            duration_ms = int((time.time() - start_time) * 1000)
                            output, _meta = agent.get_stream_output()
                            run_metrics = agent.get_run_metrics() if hasattr(agent, "get_run_metrics") else {}
                            metrics = AgentExecutionMetrics(
                                llm_calls=run_metrics.get("llm_calls", 0),
                                tokens_in=run_metrics.get("tokens_in"),
                                tokens_out=run_metrics.get("tokens_out"),
                                duration_ms=duration_ms,
                            )
                            await self._kernel.report_agent_result(
                                process_id=process_id,
                                agent_name=agent_name,
                                output=output,
                                metadata_updates=_meta,
                                metrics=metrics,
                                success=True,
                                error="",
                            )
                            if output:
                                yield (agent_name, output)
                        except Exception as e:
                            duration_ms = int((time.time() - start_time) * 1000)
                            await self._kernel.report_agent_result(
                                process_id=process_id,
                                agent_name=agent_name,
                                output=None,
                                metrics=AgentExecutionMetrics(duration_ms=duration_ms),
                                success=False,
                                error=str(e),
                            )
                    else:
                        # Non-streaming path
                        output, metadata_updates, success, error, metrics = await self._run_agent(
                            instruction=instruction,
                            process_id=process_id,
                            agent_name=agent_name,
                            agent=agent,
                            log_errors=False,
                        )

                        await self._kernel.report_agent_result(
                            process_id=process_id,
                            agent_name=agent_name,
                            output=output,
                            metadata_updates=metadata_updates,
                            metrics=metrics,
                            success=success,
                            error=error,
                        )

                        if success and output:
                            yield (agent_name, output)

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

        return output, metadata_updates, success, error, metrics


__all__ = [
    "run_kernel_loop",
    "PipelineWorker",
    "WorkerResult",
    "AgentProtocol",
]
