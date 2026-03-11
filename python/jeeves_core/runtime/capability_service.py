"""CapabilityService — Base class for kernel-driven capability services.

Capabilities subclass and override hooks:
- _ensure_ready(): Async init (DB schema, lazy services). Called once.
- _enrich_metadata(): Async. Inject db, event_emitter, session context, etc.
- _on_result(): Post-pipeline side effects (persist messages, etc).
- _build_result(): Override for custom result mapping.
"""

import asyncio
import time
from dataclasses import dataclass, field
from typing import Any, AsyncIterator, Dict, Optional, TYPE_CHECKING
from uuid import uuid4

from jeeves_core.protocols import (
    PipelineConfig,
    PipelineEvent,
    RequestContext,
)
from jeeves_core.runtime.agents import (
    AgentLogger,
    AgentPromptRegistry,
    AgentToolExecutor,
    LLMProviderFactory,
    create_pipeline_runner,
)
from jeeves_core.pipeline_worker import PipelineWorker, WorkerResult

if TYPE_CHECKING:
    from jeeves_core.kernel_client import KernelClient


@dataclass
class CapabilityResult:
    """Generic result from capability processing."""
    status: str  # "success" | "error"
    response: Optional[str] = None
    error: Optional[str] = None
    request_id: Optional[str] = None
    metadata: Dict[str, Any] = field(default_factory=dict)


class CapabilityService:
    """Base for kernel-driven capability services.

    Subclass MUST set capability_id as a class variable.
    """
    capability_id: str = ""
    output_key: str = "final_response"

    def __init__(
        self,
        *,
        llm_provider_factory: LLMProviderFactory,
        tool_executor: AgentToolExecutor,
        logger: AgentLogger,
        pipeline_config: PipelineConfig,
        kernel_client: "KernelClient",
        prompt_registry: AgentPromptRegistry,
        persistence=None,
        use_mock: bool = False,
    ):
        if not self.capability_id:
            raise ValueError(
                f"{type(self).__name__} must set capability_id class variable"
            )
        self._logger = logger
        self._kernel_client = kernel_client
        self._pipeline_config = pipeline_config
        self._persistence = persistence
        self._ready = False

        self._runtime = create_pipeline_runner(
            config=pipeline_config,
            llm_provider_factory=llm_provider_factory,
            tool_executor=tool_executor,
            logger=logger,
            prompt_registry=prompt_registry,
            persistence=persistence,
            use_mock=use_mock,
        )

        self._worker = PipelineWorker(
            kernel_client=kernel_client,
            agents=self._runtime.agents,
            logger=logger,
            persistence=persistence,
        )

    # =========================================================================
    # HOOKS (override in subclasses)
    # =========================================================================

    async def _ensure_ready(self) -> None:
        """Hook: async initialization (DB schema, lazy services). Called once."""

    async def _enrich_metadata(
        self,
        meta: Dict[str, Any],
        message: str,
        user_id: str,
        session_id: str,
    ) -> None:
        """Hook: inject capability-specific metadata. Default no-op."""

    async def _on_result(
        self,
        worker_result: WorkerResult,
        capability_result: "CapabilityResult",
        *,
        raw_input: str = "",
        session_id: str = "",
    ) -> None:
        """Hook: post-pipeline side effects. Default no-op."""

    # =========================================================================
    # CORE METHODS
    # =========================================================================

    async def _maybe_ensure_ready(self) -> None:
        """Call _ensure_ready() once."""
        if not self._ready:
            await self._ensure_ready()
            self._ready = True

    def _build_initial_envelope(
        self,
        message: str,
        user_id: str,
        session_id: str,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Build initial envelope dict for kernel initialization."""
        from datetime import datetime, timezone
        request_id = f"req_{uuid4().hex[:16]}"
        now_iso = datetime.now(timezone.utc).isoformat()
        pipeline = self._pipeline_config

        return {
            "identity": {
                "envelope_id": str(uuid4()),
                "request_id": request_id,
                "user_id": user_id,
                "session_id": session_id,
            },
            "raw_input": message,
            "received_at": now_iso,
            "outputs": {},
            "pipeline": {
                "current_stage": "",
                "stage_order": [],
                "iteration": 0,
                "max_iterations": pipeline.max_iterations,
            },
            "bounds": {
                "llm_call_count": 0,
                "max_llm_calls": pipeline.max_llm_calls,
                "tool_call_count": 0,
                "agent_hop_count": 0,
                "max_agent_hops": pipeline.max_agent_hops,
                "tokens_in": 0,
                "tokens_out": 0,
                "terminated": False,
            },
            "interrupts": {"interrupt_pending": False},
            "execution": {
                "completed_stages": [],
                "current_stage_number": 0,
                "max_stages": len(pipeline.agents),
                "all_goals": [],
                "remaining_goals": [],
                "goal_completion_status": {},
                "prior_plans": [],
                "loop_feedback": [],
            },
            "audit": {
                "processing_history": [],
                "errors": [],
                "created_at": now_iso,
                "metadata": metadata or {},
            },
        }

    async def _run_pipeline(
        self,
        initial_envelope: Dict[str, Any],
        thread_id: str,
    ) -> WorkerResult:
        """Execute pipeline under kernel control."""
        pipeline_config_dict = self._pipeline_config.to_kernel_dict()
        process_id = initial_envelope["identity"]["envelope_id"]
        return await self._worker.execute(
            process_id=process_id,
            pipeline_config=pipeline_config_dict,
            initial_envelope=initial_envelope,
            thread_id=thread_id,
        )

    async def process_message(
        self,
        *,
        user_id: str,
        session_id: str,
        message: str,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> CapabilityResult:
        """Process a user message through the kernel-driven pipeline."""
        from contextlib import nullcontext
        from jeeves_core.observability.metrics import orchestrator_started, orchestrator_completed
        from jeeves_core.observability.otel_adapter import get_global_otel_adapter

        await self._maybe_ensure_ready()

        meta = metadata or {}
        await self._enrich_metadata(meta, message, user_id, session_id)
        initial_envelope = self._build_initial_envelope(message, user_id, session_id, meta)
        request_id = initial_envelope["identity"]["request_id"]

        otel = get_global_otel_adapter()
        span_ctx = otel.start_span(
            "capability.process_message",
            attributes={
                "capability_id": self.capability_id,
                "request_id": request_id,
                "session_id": session_id,
            },
        ) if otel else nullcontext()

        orchestrator_started()
        _start = time.time()

        with span_ctx:
            try:
                result = await self._run_pipeline(initial_envelope, thread_id=session_id)
                capability_result = self._build_result(result, request_id)
                await self._on_result(
                    result, capability_result,
                    raw_input=message, session_id=session_id,
                )
                if capability_result.status == "error" and capability_result.error:
                    self._logger.error(
                        f"{self.capability_id}_pipeline_error",
                        request_id=request_id,
                        error=capability_result.error,
                    )
                orchestrator_completed(
                    "success" if capability_result.status == "success" else "error",
                    (time.time() - _start) * 1000,
                )
                return capability_result
            except Exception as e:
                orchestrator_completed("error", (time.time() - _start) * 1000)
                self._logger.error(
                    f"{self.capability_id}_processing_error",
                    request_id=request_id,
                    error=str(e),
                    exc_info=True,
                )
                raise

    async def process_message_stream(
        self,
        *,
        user_id: str,
        session_id: str,
        message: str,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> AsyncIterator[PipelineEvent]:
        """Process message with kernel-driven streaming."""
        from jeeves_core.observability.metrics import orchestrator_started, orchestrator_completed

        await self._maybe_ensure_ready()

        meta = metadata or {}
        await self._enrich_metadata(meta, message, user_id, session_id)
        initial_envelope = self._build_initial_envelope(message, user_id, session_id, meta)
        request_id = initial_envelope["identity"]["request_id"]
        pipeline_config_dict = self._pipeline_config.to_kernel_dict()
        process_id = initial_envelope["identity"]["envelope_id"]

        orchestrator_started()
        _start = time.time()

        # Collect outputs from streaming for post-stream hook
        collected_outputs: Dict[str, Dict[str, Any]] = {}

        try:
            async for agent_name, output in self._worker.execute_streaming(
                process_id=process_id,
                pipeline_config=pipeline_config_dict,
                initial_envelope=initial_envelope,
                thread_id=session_id,
                force=True,
            ):
                if agent_name not in ("__end__", "__interrupt__", "__error__", "__token__"):
                    collected_outputs[agent_name] = output

                event = self._map_stream_event(agent_name, output, request_id, collected_outputs)
                if event is not None:
                    yield event

            # Post-streaming result hook
            final = collected_outputs.get(self.output_key, {})
            response = final.get("response", "")
            cap_result = CapabilityResult(
                status="success", response=response, request_id=request_id,
            )
            await self._on_result(
                WorkerResult(outputs=collected_outputs, metadata={}, terminated=True, terminal_reason="COMPLETED"),
                cap_result,
                raw_input=message, session_id=session_id,
            )
            orchestrator_completed("success", (time.time() - _start) * 1000)

        except asyncio.CancelledError:
            orchestrator_completed("cancelled", (time.time() - _start) * 1000)
            self._logger.info(f"{self.capability_id}_streaming_cancelled", request_id=request_id)
            raise
        except Exception as e:
            orchestrator_completed("error", (time.time() - _start) * 1000)
            self._logger.error(f"{self.capability_id}_streaming_error", request_id=request_id, error=str(e))
            yield PipelineEvent("error", "__end__", {"error": str(e), "request_id": request_id})

    def _map_stream_event(
        self,
        agent_name: str,
        output: Dict[str, Any],
        request_id: str,
        collected_outputs: Dict[str, Dict[str, Any]],
    ) -> Optional[PipelineEvent]:
        """Map a streaming (agent_name, output) tuple to a PipelineEvent."""
        if agent_name == "__end__":
            return PipelineEvent("done", "__end__", {
                "final_output": collected_outputs.get(self.output_key, {}),
                "request_id": request_id,
                **output,
            })
        elif agent_name == "__interrupt__":
            return PipelineEvent("interrupt", "__interrupt__", output)
        elif agent_name == "__error__":
            return PipelineEvent("error", "__error__", output)
        elif agent_name == "__token__":
            return PipelineEvent(
                "token",
                output.get("agent", "respond"),
                output.get("event", {}).data if hasattr(output.get("event", {}), "data") else output,
                debug=False,
            )
        else:
            return PipelineEvent("stage", agent_name, {"status": "completed", **(output or {})})

    def _build_result(self, worker_result: WorkerResult, request_id: str) -> CapabilityResult:
        """Convert WorkerResult to CapabilityResult."""
        final = worker_result.outputs.get(self.output_key, {})
        reason = worker_result.terminal_reason

        if not worker_result.terminated or reason in ("", "COMPLETED"):
            response = final.get("response")
            if not response:
                return CapabilityResult(
                    status="error",
                    error="Pipeline completed but no response generated",
                    request_id=request_id,
                )
            return CapabilityResult(
                status="success",
                response=response,
                request_id=request_id,
            )

        bounds_reasons = {
            "MAX_ITERATIONS_EXCEEDED",
            "MAX_LLM_CALLS_EXCEEDED",
            "MAX_AGENT_HOPS_EXCEEDED",
            "MAX_STAGE_VISITS_EXCEEDED",
        }
        if reason in bounds_reasons:
            partial = final.get("response")
            if partial:
                return CapabilityResult(
                    status="success",
                    response=partial,
                    request_id=request_id,
                )
            return CapabilityResult(
                status="error",
                error=f"Pipeline stopped: {reason}",
                request_id=request_id,
            )

        return CapabilityResult(
            status="error",
            error=reason or "Pipeline failed",
            request_id=request_id,
        )


__all__ = ["CapabilityService", "CapabilityResult"]
