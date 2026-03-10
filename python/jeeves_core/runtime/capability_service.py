"""CapabilityService — Base class for kernel-driven capability services.

Extracts the ~90 lines of identical service code that every capability
copies: __init__, _build_envelope, _run_pipeline, process_message,
_build_result, handle_dispatch.

Capabilities subclass and override hooks:
- _ensure_ready(): Async init (DB schema, lazy services). Called once.
- _enrich_metadata(): Async. Inject db, event_emitter, session context, etc.
- _on_result(): Post-pipeline side effects (persist messages, etc).
- _build_result(): Override for custom result mapping.

Usage:
    class AssistantService(CapabilityService):
        capability_id = "assistant"

        async def _enrich_metadata(self, meta, message, user_id, session_id):
            if self._db: meta["db"] = self._db
"""

import asyncio
import time
from dataclasses import dataclass, field
from typing import Any, AsyncIterator, Dict, Optional, TYPE_CHECKING
from uuid import uuid4

from jeeves_core.protocols import (
    Envelope,
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
    create_envelope,
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
        """Hook: async initialization (DB schema, lazy services).

        Called once before first pipeline execution. Default no-op.
        """

    async def _enrich_metadata(
        self,
        meta: Dict[str, Any],
        message: str,
        user_id: str,
        session_id: str,
    ) -> None:
        """Hook: inject capability-specific metadata. Default no-op.

        Async to support session loading, DB queries, etc.
        """

    async def _on_result(
        self,
        worker_result: WorkerResult,
        capability_result: "CapabilityResult",
        envelope: Envelope,
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

    def _build_envelope(
        self,
        message: str,
        user_id: str,
        session_id: str,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> Envelope:
        """Create envelope with request context and metadata."""
        request_id = f"req_{uuid4().hex[:16]}"
        request_context = RequestContext(
            request_id=request_id,
            capability=self.capability_id,
            session_id=session_id,
            user_id=user_id,
        )
        meta = metadata or {}
        return create_envelope(
            raw_input=message,
            request_context=request_context,
            metadata=meta,
        )

    async def _run_pipeline(self, envelope: Envelope, thread_id: str) -> WorkerResult:
        """Execute pipeline under kernel control."""
        pipeline_config_dict = self._pipeline_config.to_kernel_dict()
        return await self._worker.execute(
            process_id=envelope.envelope_id,
            pipeline_config=pipeline_config_dict,
            envelope=envelope,
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
        from jeeves_core.observability.metrics import orchestrator_started, orchestrator_completed

        await self._maybe_ensure_ready()

        envelope = self._build_envelope(message, user_id, session_id, metadata)
        await self._enrich_metadata(
            envelope.metadata, message, user_id, session_id,
        )
        request_id = envelope.request_id

        orchestrator_started()
        _start = time.time()

        try:
            result = await self._run_pipeline(envelope, thread_id=session_id)
            capability_result = self._build_result(result, request_id)
            await self._on_result(result, capability_result, envelope)
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
        """Process message with kernel-driven streaming.

        Uses PipelineWorker.execute_streaming() — the kernel controls
        routing, bounds, and interrupts. Yields PipelineEvents.
        """
        await self._maybe_ensure_ready()

        envelope = self._build_envelope(message, user_id, session_id, metadata)
        await self._enrich_metadata(
            envelope.metadata, message, user_id, session_id,
        )
        request_id = envelope.request_id
        pipeline_config_dict = self._pipeline_config.to_kernel_dict()

        try:
            async for agent_name, output in self._worker.execute_streaming(
                process_id=envelope.envelope_id,
                pipeline_config=pipeline_config_dict,
                envelope=envelope,
                thread_id=session_id,
                force=True,
            ):
                event = self._map_stream_event(agent_name, output, request_id, envelope)
                if event is not None:
                    yield event

            # Post-streaming result hook
            final = envelope.outputs.get(self.output_key, {})
            response = final.get("response", "")
            result = CapabilityResult(
                status="success", response=response, request_id=request_id,
            )
            await self._on_result(
                WorkerResult(envelope=envelope, terminated=True, terminal_reason="COMPLETED"),
                result,
                envelope,
            )

        except asyncio.CancelledError:
            self._logger.info(f"{self.capability_id}_streaming_cancelled", request_id=request_id)
            raise
        except Exception as e:
            self._logger.error(f"{self.capability_id}_streaming_error", request_id=request_id, error=str(e))
            yield PipelineEvent("error", "__end__", {"error": str(e), "request_id": request_id})

    def _map_stream_event(
        self,
        agent_name: str,
        output: Dict[str, Any],
        request_id: str,
        envelope: Envelope,
    ) -> Optional[PipelineEvent]:
        """Map a streaming (agent_name, output) tuple to a PipelineEvent.

        Override to customize streaming event mapping.
        """
        if agent_name == "__end__":
            return PipelineEvent("done", "__end__", {
                "final_output": envelope.outputs.get(self.output_key, {}),
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
        """Convert WorkerResult to CapabilityResult.

        Reads termination status from WorkerResult (kernel is sole authority).
        """
        final = worker_result.envelope.outputs.get(self.output_key, {})
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

    async def resume_pipeline(self, thread_id: str) -> Optional[CapabilityResult]:
        """Resume pipeline from persisted state. Returns None if no state found."""
        if not self._persistence:
            return None
        state = await self._persistence.load_state(thread_id)
        if not state:
            return None
        envelope = Envelope.from_dict(state)
        request_id = envelope.request_id
        result = await self._run_pipeline(envelope, thread_id=thread_id)
        return self._build_result(result, request_id)

    async def handle_dispatch(self, envelope: Envelope) -> Envelope:
        """Kernel dispatch handler."""
        await self._maybe_ensure_ready()
        result = await self._run_pipeline(envelope, thread_id=envelope.session_id)
        return result.envelope

    def get_dispatch_handler(self):
        """Return the dispatch handler for kernel registration."""
        return self.handle_dispatch


__all__ = ["CapabilityService", "CapabilityResult"]
