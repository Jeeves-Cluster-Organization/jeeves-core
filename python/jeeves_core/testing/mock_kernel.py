"""MockKernelClient — Pure-Python kernel for testing.

Replicates routing + bounds logic from the Rust kernel without TCP/IPC.
Sufficient for pipeline unit tests.
"""

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

from jeeves_core.kernel_client import (
    AgentExecutionMetrics,
    OrchestratorInstruction,
    OrchestrationSessionState,
)
from jeeves_core.testing.routing_eval import evaluate_routing


@dataclass
class _SessionState:
    """Internal state for a mock kernel session."""
    pipeline_config: Dict[str, Any]
    envelope: Dict[str, Any]
    stages: Dict[str, Dict[str, Any]]  # name -> stage dict
    stage_order: List[str]
    current_stage_idx: int = 0
    iteration: int = 0
    llm_calls: int = 0
    agent_hops: int = 0
    stage_visits: Dict[str, int] = field(default_factory=dict)
    terminated: bool = False
    terminal_reason: str = ""
    outputs: Dict[str, Dict] = field(default_factory=dict)
    last_agent_failed: bool = False
    last_agent_name: str = ""
    pending_agent: Optional[str] = None
    pending_agents: Optional[List[str]] = None  # parallel fan-out


class MockKernelClient:
    """Pure-Python kernel for testing — replicates routing + bounds logic."""

    def __init__(self):
        self._sessions: Dict[str, _SessionState] = {}
        self._pending_interrupts: Dict[str, Dict[str, Any]] = {}  # stage -> interrupt

    def inject_interrupt(self, stage: str, interrupt: Dict[str, Any]) -> None:
        """Schedule a WAIT_INTERRUPT after the given stage completes."""
        self._pending_interrupts[stage] = interrupt

    async def initialize_orchestration_session(
        self,
        process_id: str,
        pipeline_config: Dict[str, Any],
        envelope: Dict[str, Any],
        force: bool = False,
    ) -> OrchestrationSessionState:
        if process_id in self._sessions and not force:
            raise RuntimeError(f"Session already exists: {process_id}")

        stages_list = pipeline_config.get("stages", [])
        stages = {s["name"]: s for s in stages_list}
        stage_order = [s["name"] for s in stages_list]

        self._sessions[process_id] = _SessionState(
            pipeline_config=pipeline_config,
            envelope=envelope,
            stages=stages,
            stage_order=stage_order,
            outputs=dict(envelope.get("outputs", {})),
        )

        return OrchestrationSessionState(
            process_id=process_id,
            current_stage=stage_order[0] if stage_order else "",
            stage_order=stage_order,
            envelope=envelope,
        )

    async def get_next_instruction(self, process_id: str) -> OrchestratorInstruction:
        session = self._sessions.get(process_id)
        if not session:
            raise RuntimeError(f"No session: {process_id}")

        if session.terminated:
            return OrchestratorInstruction(
                kind="TERMINATE",
                terminal_reason=session.terminal_reason,
            )

        # Parallel fan-out: return RUN_AGENTS if multiple pending
        if session.pending_agents and len(session.pending_agents) > 1:
            agents = session.pending_agents
            session.pending_agents = None
            return OrchestratorInstruction(
                kind="RUN_AGENTS",
                agents=agents,
                envelope=session.envelope,
            )

        if session.pending_agents:
            # Single agent from pending_agents list
            agent_name = session.pending_agents[0]
            session.pending_agents = None
            return OrchestratorInstruction(
                kind="RUN_AGENT",
                agents=[agent_name],
                envelope=session.envelope,
            )

        if session.pending_agent:
            agent_name = session.pending_agent
            session.pending_agent = None
            return OrchestratorInstruction(
                kind="RUN_AGENT",
                agents=[agent_name],
                envelope=session.envelope,
            )

        # First call — start with first stage (check for parallel group)
        if session.iteration == 0 and session.agent_hops == 0:
            first_stage = session.stage_order[0] if session.stage_order else None
            if not first_stage:
                session.terminated = True
                session.terminal_reason = "COMPLETED"
                return OrchestratorInstruction(kind="TERMINATE", terminal_reason="COMPLETED")

            # Check if first stage has a parallel_group
            first_stage_cfg = session.stages.get(first_stage, {})
            pg = first_stage_cfg.get("parallel_group")
            if pg:
                parallel = [
                    name for name, s in session.stages.items()
                    if s.get("parallel_group") == pg
                ]
                if len(parallel) > 1:
                    session.pending_agents = parallel
                    return await self.get_next_instruction(process_id)

            session.pending_agent = first_stage
            return await self.get_next_instruction(process_id)

        # Should not reach here — routing happens in report_agent_result
        session.terminated = True
        session.terminal_reason = "COMPLETED"
        return OrchestratorInstruction(kind="TERMINATE", terminal_reason="COMPLETED")

    async def report_agent_result(
        self,
        process_id: str,
        agent_name: str,
        output: Optional[Dict[str, Any]] = None,
        metrics: Optional[AgentExecutionMetrics] = None,
        success: bool = True,
        error: str = "",
    ) -> OrchestratorInstruction:
        session = self._sessions.get(process_id)
        if not session:
            raise RuntimeError(f"No session: {process_id}")

        # Store output
        if output:
            session.outputs[agent_name] = output

        # Accumulate metrics
        if metrics:
            session.llm_calls += metrics.llm_calls
        session.agent_hops += 1
        session.iteration += 1

        # Track visits
        session.stage_visits[agent_name] = session.stage_visits.get(agent_name, 0) + 1
        session.last_agent_failed = not success
        session.last_agent_name = agent_name

        config = session.pipeline_config

        # --- Bounds checking (>= semantics, before routing) ---
        max_iterations = config.get("max_iterations", 100)
        if session.iteration >= max_iterations:
            session.terminated = True
            session.terminal_reason = "MAX_ITERATIONS_EXCEEDED"
            return OrchestratorInstruction(kind="TERMINATE", terminal_reason="MAX_ITERATIONS_EXCEEDED", outputs=dict(session.outputs))

        max_llm_calls = config.get("max_llm_calls", 100)
        if session.llm_calls >= max_llm_calls:
            session.terminated = True
            session.terminal_reason = "MAX_LLM_CALLS_EXCEEDED"
            return OrchestratorInstruction(kind="TERMINATE", terminal_reason="MAX_LLM_CALLS_EXCEEDED", outputs=dict(session.outputs))

        max_agent_hops = config.get("max_agent_hops", 100)
        if session.agent_hops >= max_agent_hops:
            session.terminated = True
            session.terminal_reason = "MAX_AGENT_HOPS_EXCEEDED"
            return OrchestratorInstruction(kind="TERMINATE", terminal_reason="MAX_AGENT_HOPS_EXCEEDED", outputs=dict(session.outputs))

        # Check per-stage visit limit
        stage = session.stages.get(agent_name, {})
        max_visits = stage.get("max_visits")
        if max_visits is not None and session.stage_visits[agent_name] >= max_visits:
            session.terminated = True
            session.terminal_reason = "MAX_STAGE_VISITS_EXCEEDED"
            return OrchestratorInstruction(kind="TERMINATE", terminal_reason="MAX_STAGE_VISITS_EXCEEDED", outputs=dict(session.outputs))

        # --- Schema validation (output_schema.required check) ---
        output_schema = stage.get("output_schema")
        if output_schema and output and success:
            required = output_schema.get("required", [])
            missing = [r for r in required if r not in output]
            if missing:
                session.last_agent_failed = True
                success = False

        # --- Interrupt injection ---
        if agent_name in self._pending_interrupts:
            interrupt = self._pending_interrupts.pop(agent_name)
            return OrchestratorInstruction(
                kind="WAIT_INTERRUPT",
                interrupt=interrupt,
                outputs=dict(session.outputs),
            )

        # --- Routing ---
        # 1. Agent failed + error_next
        if not success and stage.get("error_next"):
            next_stage = stage["error_next"]
            session.pending_agent = next_stage
            return await self.get_next_instruction(process_id)

        # 2. Evaluate routing rules (first match wins)
        routing_rules = stage.get("routing", [])
        current_output = output or {}
        envelope_metadata = (
            session.envelope.get("audit", {}).get("metadata", {})
            or session.envelope.get("metadata", {})  # flat fallback for raw test dicts
        )
        next_stage = evaluate_routing(
            routing_rules,
            session.outputs,
            envelope_metadata,
            current_agent_output=current_output,
        )

        if next_stage:
            next_stage = self._resolve_parallel_group(session, next_stage)
            return await self.get_next_instruction(process_id)

        # 3. default_next fallback
        default_next = stage.get("default_next")
        if default_next:
            default_next = self._resolve_parallel_group(session, default_next)
            return await self.get_next_instruction(process_id)

        # 4. No match + no default_next → terminate COMPLETED
        session.terminated = True
        session.terminal_reason = "COMPLETED"
        return OrchestratorInstruction(
            kind="TERMINATE", terminal_reason="COMPLETED",
            outputs=dict(session.outputs),
        )

    def _resolve_parallel_group(self, session: _SessionState, target: str) -> str:
        """If target has a parallel_group, set pending_agents for the whole group.
        Otherwise set pending_agent. Returns target for chaining."""
        target_cfg = session.stages.get(target, {})
        pg = target_cfg.get("parallel_group")
        if pg:
            parallel = [
                name for name, s in session.stages.items()
                if s.get("parallel_group") == pg
            ]
            if len(parallel) > 1:
                session.pending_agents = parallel
                return target
        session.pending_agent = target
        return target

    async def terminate_process(self, process_id: str, reason: str = "") -> None:
        session = self._sessions.get(process_id)
        if session:
            session.terminated = True
            session.terminal_reason = reason or "USER_CANCELLED"
