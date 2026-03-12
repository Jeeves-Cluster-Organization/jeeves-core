"""MockKernelClient — Pure-Python kernel for testing.

Replicates routing + bounds logic from the Rust kernel without TCP/IPC.
Sufficient for pipeline unit tests.
"""

from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from uuid import uuid4

from jeeves_core.kernel_client import (
    AgentExecutionMetrics,
    OrchestratorInstruction,
    OrchestrationSessionState,
    ProcessInfo,
)
from jeeves_core.testing.routing_eval import evaluate_routing

__all__ = ["MockKernelClient"]


def _merge_state_field(state: Dict[str, Any], key: str, val: Any, strategy: str) -> None:
    """Merge a value into state according to strategy — mirrors Rust merge_state_field."""
    if strategy == "Append":
        existing = state.get(key)
        if isinstance(existing, list):
            existing.append(val)
        elif existing is not None:
            state[key] = [existing, val]
        else:
            state[key] = [val]
    elif strategy == "MergeDict":
        if isinstance(val, dict):
            existing = state.get(key)
            if isinstance(existing, dict):
                existing.update(val)
            else:
                state[key] = dict(val)
        else:
            state[key] = val
    else:  # Replace (default)
        state[key] = val


def _derive_outcome(reason: str) -> str:
    """Derive outcome from terminal reason — mirrors Rust TerminalReason::outcome()."""
    if reason in ("COMPLETED", "BREAK_REQUESTED"):
        return "completed"
    if "EXCEEDED" in reason:
        return "bounds_exceeded"
    return "failed"


@dataclass
class _SessionState:
    """Internal state for a mock kernel session."""
    pipeline_config: Dict[str, Any]
    envelope: Dict[str, Any]
    stages: Dict[str, Dict[str, Any]]  # name -> stage dict
    stage_order: List[str]
    iteration: int = 0
    llm_calls: int = 0
    agent_hops: int = 0
    stage_visits: Dict[str, int] = field(default_factory=dict)
    terminated: bool = False
    terminal_reason: str = ""
    outputs: Dict[str, Dict] = field(default_factory=dict)
    state: Dict[str, Any] = field(default_factory=dict)
    last_agent_failed: bool = False
    last_agent_name: str = ""
    pending_agent: Optional[str] = None
    pending_agents: Optional[List[str]] = None  # parallel fan-out (Fork)
    fork_node: Optional[str] = None  # name of Fork node for post-join routing
    fork_pending: Optional[set] = None  # branches still awaiting report


class MockKernelClient:
    """Pure-Python kernel for testing — replicates routing + bounds logic."""

    def __init__(self):
        self._sessions: Dict[str, _SessionState] = {}
        self._pending_interrupts: Dict[str, Dict[str, Any]] = {}  # stage -> interrupt

    @staticmethod
    def _check_bounds(session: _SessionState) -> Optional[OrchestratorInstruction]:
        """Check iteration/llm/hop bounds. Returns TERMINATE instruction or None."""
        config = session.pipeline_config
        if session.iteration >= config.get("max_iterations", 100):
            session.terminated = True
            session.terminal_reason = "MAX_ITERATIONS_EXCEEDED"
            return OrchestratorInstruction(kind="TERMINATE", terminal_reason="MAX_ITERATIONS_EXCEEDED", outcome="bounds_exceeded", outputs=dict(session.outputs))
        if session.llm_calls >= config.get("max_llm_calls", 100):
            session.terminated = True
            session.terminal_reason = "MAX_LLM_CALLS_EXCEEDED"
            return OrchestratorInstruction(kind="TERMINATE", terminal_reason="MAX_LLM_CALLS_EXCEEDED", outcome="bounds_exceeded", outputs=dict(session.outputs))
        if session.agent_hops >= config.get("max_agent_hops", 100):
            session.terminated = True
            session.terminal_reason = "MAX_AGENT_HOPS_EXCEEDED"
            return OrchestratorInstruction(kind="TERMINATE", terminal_reason="MAX_AGENT_HOPS_EXCEEDED", outcome="bounds_exceeded", outputs=dict(session.outputs))
        return None

    def _check_max_visits(self, session: _SessionState, target: str) -> Optional[OrchestratorInstruction]:
        """Check max_visits on TARGET stage (entry-guard, mirrors Rust). Returns TERMINATE or None."""
        target_cfg = session.stages.get(target, {})
        max_visits = target_cfg.get("max_visits")
        if max_visits is not None and session.stage_visits.get(target, 0) >= max_visits:
            session.terminated = True
            session.terminal_reason = "MAX_STAGE_VISITS_EXCEEDED"
            return OrchestratorInstruction(kind="TERMINATE", terminal_reason="MAX_STAGE_VISITS_EXCEEDED", outcome="bounds_exceeded", outputs=dict(session.outputs))
        return None

    def inject_interrupt(self, stage: str, interrupt: Dict[str, Any]) -> None:
        """Schedule a WAIT_INTERRUPT after the given stage completes."""
        self._pending_interrupts[stage] = interrupt

    async def initialize_orchestration_session(
        self,
        process_id: str,
        pipeline_config: Dict[str, Any],
        *,
        user_id: str = "test-user",
        session_id: str = "test-session",
        raw_input: str = "",
        metadata: Optional[Dict[str, Any]] = None,
        force: bool = False,
    ) -> OrchestrationSessionState:
        if process_id in self._sessions and not force:
            raise RuntimeError(f"Session already exists: {process_id}")

        stages_list = pipeline_config.get("stages", [])
        stages = {s["name"]: s for s in stages_list}
        stage_order = [s["name"] for s in stages_list]

        # Build minimal envelope internally (mirrors Rust Envelope::new_minimal)
        now_iso = datetime.now(timezone.utc).isoformat()
        envelope = {
            "identity": {
                "envelope_id": f"env_{uuid4().hex[:16]}",
                "request_id": process_id,
                "user_id": user_id,
                "session_id": session_id,
            },
            "raw_input": raw_input,
            "received_at": now_iso,
            "outputs": {},
            "pipeline": {
                "current_stage": "",
                "stage_order": [],
                "iteration": 0,
                "max_iterations": pipeline_config.get("max_iterations", 100),
            },
            "bounds": {
                "llm_call_count": 0,
                "max_llm_calls": pipeline_config.get("max_llm_calls", 100),
                "tool_call_count": 0,
                "agent_hop_count": 0,
                "max_agent_hops": pipeline_config.get("max_agent_hops", 100),
                "tokens_in": 0,
                "tokens_out": 0,
                "terminated": False,
            },
            "interrupts": {"interrupt_pending": False},
            "execution": {
                "completed_stages": [],
                "current_stage_number": 0,
                "max_stages": len(stages_list),
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

        self._sessions[process_id] = _SessionState(
            pipeline_config=pipeline_config,
            envelope=envelope,
            stages=stages,
            stage_order=stage_order,
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
                outcome=_derive_outcome(session.terminal_reason),
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

            # Gate dispatch: evaluate routing without running agent
            stage_cfg = session.stages.get(agent_name, {})
            if stage_cfg.get("node_kind") == "Gate":
                session.iteration += 1
                session.stage_visits[agent_name] = session.stage_visits.get(agent_name, 0) + 1
                # Bounds check after iteration increment (mirrors Rust)
                term = self._check_bounds(session)
                if term:
                    return term
                # Evaluate routing
                routing_rules = stage_cfg.get("routing", [])
                envelope_metadata = (
                    session.envelope.get("audit", {}).get("metadata", {})
                    or session.envelope.get("metadata", {})
                )
                next_stage = evaluate_routing(
                    routing_rules, session.outputs, envelope_metadata,
                    state=session.state,
                )
                if next_stage:
                    session.pending_agent = next_stage
                    return await self.get_next_instruction(process_id)
                default_next = stage_cfg.get("default_next")
                if default_next:
                    session.pending_agent = default_next
                    return await self.get_next_instruction(process_id)
                # No match → terminate
                session.terminated = True
                session.terminal_reason = "COMPLETED"
                return OrchestratorInstruction(
                    kind="TERMINATE", terminal_reason="COMPLETED", outcome="completed",
                    outputs=dict(session.outputs),
                )

            # Fork dispatch: evaluate ALL rules, collect matching targets
            if stage_cfg.get("node_kind") == "Fork":
                session.iteration += 1
                session.stage_visits[agent_name] = session.stage_visits.get(agent_name, 0) + 1
                # Bounds check after iteration increment (mirrors Rust)
                term = self._check_bounds(session)
                if term:
                    return term
                routing_rules = stage_cfg.get("routing", [])
                envelope_metadata = (
                    session.envelope.get("audit", {}).get("metadata", {})
                    or session.envelope.get("metadata", {})
                )
                from jeeves_core.testing.routing_eval import evaluate_expr
                targets: list[str] = []
                for rule in routing_rules:
                    expr = rule.get("expr", {})
                    if evaluate_expr(expr, session.outputs, envelope_metadata,
                                     state=session.state):
                        target = rule.get("target", "")
                        if target and target not in targets:
                            targets.append(target)
                if not targets:
                    default_next = stage_cfg.get("default_next")
                    if default_next:
                        targets.append(default_next)
                if not targets:
                    session.terminated = True
                    session.terminal_reason = "COMPLETED"
                    return OrchestratorInstruction(
                        kind="TERMINATE", terminal_reason="COMPLETED", outcome="completed",
                        outputs=dict(session.outputs),
                    )
                if len(targets) == 1:
                    session.pending_agent = targets[0]
                else:
                    session.pending_agents = targets
                    session.fork_pending = set(targets)
                # Store fork name for post-join routing
                session.fork_node = agent_name
                return await self.get_next_instruction(process_id)

            # Entry-guard: check max_visits on target before dispatching (mirrors Rust)
            term = self._check_max_visits(session, agent_name)
            if term:
                return term

            return OrchestratorInstruction(
                kind="RUN_AGENT",
                agents=[agent_name],
                envelope=session.envelope,
            )

        # First call — start with first stage
        if session.iteration == 0 and session.agent_hops == 0:
            first_stage = session.stage_order[0] if session.stage_order else None
            if not first_stage:
                session.terminated = True
                session.terminal_reason = "COMPLETED"
                return OrchestratorInstruction(kind="TERMINATE", terminal_reason="COMPLETED", outcome="completed")

            session.pending_agent = first_stage
            return await self.get_next_instruction(process_id)

        # Should not reach here — routing happens in report_agent_result
        session.terminated = True
        session.terminal_reason = "COMPLETED"
        return OrchestratorInstruction(kind="TERMINATE", terminal_reason="COMPLETED", outcome="completed")

    async def report_agent_result(
        self,
        process_id: str,
        agent_name: str,
        output: Optional[Dict[str, Any]] = None,
        metrics: Optional[AgentExecutionMetrics] = None,
        success: bool = True,
        error: str = "",
        metadata_updates: Optional[Dict[str, Any]] = None,
        break_loop: bool = False,
    ) -> OrchestratorInstruction:
        session = self._sessions.get(process_id)
        if not session:
            raise RuntimeError(f"No session: {process_id}")

        # Store output
        if output:
            session.outputs[agent_name] = output

        # State merge: write to state[output_key] per state_schema
        if output:
            state_schema = session.pipeline_config.get("state_schema", [])
            stage_cfg = session.stages.get(agent_name, {})
            output_key = stage_cfg.get("output_key") or agent_name
            for sf in state_schema:
                if sf.get("key") == output_key:
                    strategy = sf.get("merge", "Replace")
                    _merge_state_field(session.state, sf["key"], dict(output), strategy)
                    break

        # Accumulate metrics
        if metrics:
            session.llm_calls += metrics.llm_calls
        session.agent_hops += 1
        session.iteration += 1

        # Track visits
        session.stage_visits[agent_name] = session.stage_visits.get(agent_name, 0) + 1
        session.last_agent_failed = not success
        session.last_agent_name = agent_name

        # --- Bounds checking (>= semantics, before routing) ---
        term = self._check_bounds(session)
        if term:
            return term

        stage = session.stages.get(agent_name, {})

        # --- Break check (after bounds, before interrupt/routing) ---
        if break_loop:
            session.terminated = True
            session.terminal_reason = "BREAK_REQUESTED"
            return OrchestratorInstruction(
                kind="TERMINATE", terminal_reason="BREAK_REQUESTED",
                outcome="completed", outputs=dict(session.outputs),
            )

        # --- Fork join: intermediate branches return WAIT_PARALLEL ---
        if session.fork_pending is not None and agent_name in session.fork_pending:
            session.fork_pending.discard(agent_name)
            fork_cfg = session.stages.get(session.fork_node or "", {})
            join = fork_cfg.get("join_strategy", "WaitAll")
            # WaitFirst: advance on first completion; WaitAll: advance when all done
            join_met = (
                len(session.fork_pending) == 0
                if join == "WaitAll"
                else True  # WaitFirst: any single completion triggers join
            )
            if join_met:
                fork_default = fork_cfg.get("default_next")
                session.fork_pending = None
                session.fork_node = None
                if fork_default:
                    session.pending_agent = fork_default
                # else: next get_next_instruction will terminate COMPLETED
            return OrchestratorInstruction(
                kind="WAIT_PARALLEL", outputs=dict(session.outputs),
            )

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
            state=session.state,
        )

        if next_stage:
            session.pending_agent = next_stage
            return await self.get_next_instruction(process_id)

        # 3. default_next fallback
        default_next = stage.get("default_next")
        if default_next:
            session.pending_agent = default_next
            return await self.get_next_instruction(process_id)

        # 4. No match + no default_next → terminate COMPLETED
        session.terminated = True
        session.terminal_reason = "COMPLETED"
        return OrchestratorInstruction(
            kind="TERMINATE", terminal_reason="COMPLETED", outcome="completed",
            outputs=dict(session.outputs),
        )

    async def terminate_process(self, process_id: str, reason: str = "") -> None:
        session = self._sessions.get(process_id)
        if session:
            session.terminated = True
            session.terminal_reason = reason or "USER_CANCELLED"

    # --- Stubs for methods called by PipelineWorker but not needed for routing tests ---

    async def publish_event(
        self, event_type: str, payload: Any, *, source: str = "",
    ) -> int:
        return 0

    async def register_tool(
        self, tool_name: str, description: str = "", **kwargs: Any,
    ) -> None:
        pass

    async def grant_tool_access(
        self, process_id: str, tool_names: List[str],
    ) -> None:
        pass

    async def get_process(self, process_id: str) -> Optional[ProcessInfo]:
        return None

    async def record_usage(
        self, *, pid: str, llm_calls: int = 0, tool_calls: int = 0,
        tokens_in: int = 0, tokens_out: int = 0,
    ) -> None:
        pass
