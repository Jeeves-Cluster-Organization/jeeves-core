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
from jeeves_core.protocols.types import Envelope

if TYPE_CHECKING:
    from jeeves_core.runtime.agents import Agent

logger = logging.getLogger(__name__)


# =============================================================================
# Types
# =============================================================================

class AgentProtocol(Protocol):
    """Protocol for agents that can be executed by the worker."""

    name: str

    async def process(self, envelope: Envelope) -> Envelope:
        """Execute agent on envelope."""
        ...


@dataclass
class WorkerResult:
    """Result from typed Envelope pipeline execution."""
    envelope: Envelope
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
    await kernel.initialize_orchestration_session(
        process_id=process_id,
        pipeline_config=pipeline_config,
        envelope=initial_envelope,
    )
    instruction = await kernel.get_next_instruction(process_id)

    while instruction.kind in ("RUN_AGENT", "RUN_AGENTS"):
        if instruction.kind == "RUN_AGENTS":
            # Parallel fan-out: run all agents concurrently
            async def _dispatch_one(name: str):
                single = OrchestratorInstruction(
                    kind="RUN_AGENT",
                    agents=[name],
                    envelope=instruction.envelope,
                )
                return name, await run_agent(single)

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
                    return instruction
            # After all parallel agents reported, get next instruction
            instruction = await kernel.get_next_instruction(process_id)
        else:
            # Single agent
            output, success, error, metrics = await run_agent(instruction)
            instruction = await kernel.report_agent_result(
                process_id=process_id,
                agent_name=instruction.agents[0] if instruction.agents else "",
                output=output,
                metrics=metrics,
                success=success,
                error=error,
            )

    return instruction


# =============================================================================
# Worker
# =============================================================================

class PipelineWorker:
    """Executes agents under kernel control.

    This worker has NO orchestration logic - it simply:
    1. Initializes a session with the kernel
    2. Gets instructions from the kernel
    3. Executes agents as instructed
    4. Reports results back to the kernel
    5. Repeats until TERMINATE or WAIT_INTERRUPT

    The kernel makes all decisions about:
    - Which agent to run next
    - Whether to continue or terminate
    - Bounds checking
    - Routing rule evaluation
    """

    def __init__(
        self,
        kernel_client: KernelClient,
        agents: Dict[str, AgentProtocol],
        logger: Optional[logging.Logger] = None,
        persistence: Optional[Any] = None,
    ):
        """Initialize the pipeline worker.

        Args:
            kernel_client: Connected KernelClient for IPC calls
            agents: Dict mapping agent names to Agent instances
            logger: Optional logger for structured logging
            persistence: Optional persistence layer for state saving
        """
        self._kernel = kernel_client
        self._agents = agents
        self._logger = logger or logging.getLogger(__name__)
        self._persistence = persistence

    async def execute(
        self,
        process_id: str,
        pipeline_config: Dict[str, Any],
        envelope: Envelope,
        thread_id: str = "",
        force: bool = False,
    ) -> WorkerResult:
        """Execute pipeline under kernel control.

        Args:
            process_id: Unique process identifier
            pipeline_config: Pipeline configuration dict
            envelope: Initial envelope
            thread_id: Optional thread ID for persistence
            force: If True, terminate any existing session before starting.

        Returns:
            WorkerResult with final envelope and status
        """
        if force:
            try:
                await self._kernel.terminate_process(process_id, reason="force_replace")
            except KernelClientError:
                pass  # session might not exist

        async def _run_agent_dispatch(
            instruction: OrchestratorInstruction,
        ) -> Tuple[Optional[Dict[str, Any]], bool, str, AgentExecutionMetrics]:
            nonlocal envelope
            agent_name = instruction.agents[0] if instruction.agents else ""
            agent = self._agents.get(agent_name)

            if not agent:
                self._logger.error(
                    "worker_agent_not_found",
                    process_id=process_id,
                    agent_name=agent_name,
                )
                return (None, False, f"Agent not found: {agent_name}", AgentExecutionMetrics())

            # Execute agent
            envelope, success, error, output, metrics = await self._run_agent(
                envelope=envelope,
                process_id=process_id,
                agent_name=agent_name,
                agent=agent,
            )

            # Persist state if configured
            if self._persistence and thread_id:
                try:
                    await self._persistence.save_state(thread_id, envelope.to_state_dict())
                except Exception as e:
                    self._logger.warning("worker_persistence_failed", error=str(e))

            return (output, success, error, metrics)

        try:
            terminal = await run_kernel_loop(
                self._kernel, process_id, pipeline_config,
                envelope.to_dict(), _run_agent_dispatch,
            )
        except KernelClientError as e:
            self._logger.error("worker_kernel_loop_failed", error=str(e))
            return WorkerResult(
                envelope=envelope,
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
                envelope=envelope,
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
                envelope=envelope,
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
                envelope=envelope,
                terminated=True,
                terminal_reason=f"Unknown instruction kind: {terminal.kind}",
            )

    async def execute_streaming(
        self,
        process_id: str,
        pipeline_config: Dict[str, Any],
        envelope: Envelope,
        thread_id: str = "",
        force: bool = False,
    ) -> AsyncIterator[Tuple[str, Dict[str, Any]]]:
        """Execute pipeline with streaming outputs.

        Yields (agent_name, output) tuples as agents complete.

        Args:
            process_id: Unique process identifier
            pipeline_config: Pipeline configuration dict
            envelope: Initial envelope
            thread_id: Optional thread ID for persistence
            force: If True, replace existing session with same process_id.
                   Default False means error if session already exists.

        Yields:
            Tuple of (agent_name, output_dict)
        """
        # Initialize session with kernel
        try:
            session_state = await self._kernel.initialize_orchestration_session(
                process_id=process_id,
                pipeline_config=pipeline_config,
                envelope=envelope.to_dict(),
                force=force,
            )
        except KernelClientError as e:
            error_msg = str(e)
            if "already exists" in error_msg.lower():
                self._logger.error(
                    "worker_session_already_exists",
                    process_id=process_id,
                    hint="Use force=True to replace existing session",
                )
            elif "deadline" in error_msg.lower():
                self._logger.error(
                    "worker_session_deadline_exceeded",
                    process_id=process_id,
                )
            else:
                self._logger.error("worker_session_init_failed", error=error_msg)
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
                        llm_calls_before = envelope.llm_call_count

                        try:
                            async for event_type, event in agent.stream(envelope):
                                yield ("__token__", {"agent": agent_name, "event": event})

                            duration_ms = int((time.time() - start_time) * 1000)
                            output = envelope.outputs.get(agent_name, {})
                            metrics = AgentExecutionMetrics(
                                llm_calls=envelope.llm_call_count - llm_calls_before,
                                duration_ms=duration_ms,
                            )
                            await self._kernel.report_agent_result(
                                process_id=process_id,
                                agent_name=agent_name,
                                output=output,
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
                        envelope, success, error, output, metrics = await self._run_agent(
                            envelope=envelope,
                            process_id=process_id,
                            agent_name=agent_name,
                            agent=agent,
                            log_errors=False,
                        )

                        await self._kernel.report_agent_result(
                            process_id=process_id,
                            agent_name=agent_name,
                            output=output,
                            metrics=metrics,
                            success=success,
                            error=error,
                        )

                        if success and output:
                            yield (agent_name, output)

                if self._persistence and thread_id:
                    try:
                        await self._persistence.save_state(thread_id, envelope.to_state_dict())
                    except Exception:
                        pass

    async def _run_agent(
        self,
        envelope: Envelope,
        process_id: str,
        agent_name: str,
        agent: AgentProtocol,
        *,
        log_errors: bool = True,
    ) -> Tuple[Envelope, bool, str, Optional[Dict[str, Any]], AgentExecutionMetrics]:
        """Run one agent execution step and return updated envelope/output/metrics."""
        start_time = time.time()
        llm_calls_before = envelope.llm_call_count

        try:
            envelope = await agent.process(envelope)
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
        llm_calls = envelope.llm_call_count - llm_calls_before
        agent_output = envelope.outputs.get(agent_name, {}) if success else {}
        agent_metrics = self._extract_agent_metrics(envelope, agent_name, agent_output)
        metrics = AgentExecutionMetrics(
            llm_calls=llm_calls,
            tool_calls=agent_metrics["tool_calls"],
            tokens_in=agent_metrics["tokens_in"],
            tokens_out=agent_metrics["tokens_out"],
            duration_ms=duration_ms,
        )
        output = agent_output if success else None
        return envelope, success, error, output, metrics

    def _extract_agent_metrics(
        self,
        envelope: Envelope,
        agent_name: str,
        agent_output: Dict[str, Any],
    ) -> Dict[str, Optional[int]]:
        has_tool_calls = False
        tool_calls = 0

        tokens_in: Optional[int] = None
        tokens_out: Optional[int] = None
        meta = envelope.metadata if isinstance(envelope.metadata, dict) else {}
        by_agent = meta.get("_agent_run_metrics", {})
        if isinstance(by_agent, dict):
            run_metrics = by_agent.get(agent_name, {})
            if isinstance(run_metrics, dict):
                if isinstance(run_metrics.get("tool_calls"), int):
                    tool_calls = run_metrics["tool_calls"]
                    has_tool_calls = True
                if isinstance(run_metrics.get("tokens_in"), int):
                    tokens_in = run_metrics["tokens_in"]
                if isinstance(run_metrics.get("tokens_out"), int):
                    tokens_out = run_metrics["tokens_out"]

        if not has_tool_calls and isinstance(agent_output, dict):
            calls = agent_output.get("tool_calls", [])
            if isinstance(calls, list):
                tool_calls = len(calls)

        return {
            "tool_calls": tool_calls,
            "tokens_in": tokens_in,
            "tokens_out": tokens_out,
        }

__all__ = [
    "run_kernel_loop",
    "PipelineWorker",
    "WorkerResult",
    "AgentProtocol",
]
