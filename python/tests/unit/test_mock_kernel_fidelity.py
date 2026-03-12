"""MockKernelClient fidelity tests — gaps not covered by test_harness_scenarios.py.

Tests: max_llm_calls, max_agent_hops bounds, Gate nodes, WaitFirst join,
break_loop, state merge strategies.
"""

import pytest
from jeeves_core.testing.mock_kernel import MockKernelClient
from jeeves_core.kernel_client import AgentExecutionMetrics


# =============================================================================
# Helpers
# =============================================================================

def _pipeline(name, stages, max_iterations=100, max_llm_calls=100, max_agent_hops=100, **kw):
    return {
        "name": name,
        "stages": stages,
        "max_iterations": max_iterations,
        "max_llm_calls": max_llm_calls,
        "max_agent_hops": max_agent_hops,
        **kw,
    }

def _stage(name, **kw):
    return {"name": name, "agent": name, **kw}


# =============================================================================
# Bounds: max_llm_calls
# =============================================================================

class TestMaxLlmCallsBounds:
    @pytest.mark.asyncio
    async def test_terminates_at_limit(self):
        kernel = MockKernelClient()
        config = _pipeline("llm_bounds", [
            _stage("a", default_next="b"),
            _stage("b"),
        ], max_llm_calls=1)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        result = await kernel.report_agent_result(
            "p1", "a", output={"x": 1},
            metrics=AgentExecutionMetrics(llm_calls=1),
        )
        assert result.kind == "TERMINATE"
        assert result.terminal_reason == "MAX_LLM_CALLS_EXCEEDED"

    @pytest.mark.asyncio
    async def test_under_limit_proceeds(self):
        kernel = MockKernelClient()
        config = _pipeline("llm_ok", [
            _stage("a", default_next="b"),
            _stage("b"),
        ], max_llm_calls=2)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        # report_agent_result routes through default_next and returns next instruction
        result = await kernel.report_agent_result(
            "p1", "a", output={"x": 1},
            metrics=AgentExecutionMetrics(llm_calls=1),
        )
        assert result.kind == "RUN_AGENT"
        assert result.agents == ["b"]


# =============================================================================
# Bounds: max_agent_hops
# =============================================================================

class TestMaxAgentHopsBounds:
    @pytest.mark.asyncio
    async def test_terminates_at_limit(self):
        kernel = MockKernelClient()
        config = _pipeline("hop_bounds", [
            _stage("a", default_next="b"),
            _stage("b"),
        ], max_agent_hops=1)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        result = await kernel.report_agent_result("p1", "a", output={"x": 1})
        assert result.kind == "TERMINATE"
        assert result.terminal_reason == "MAX_AGENT_HOPS_EXCEEDED"

    @pytest.mark.asyncio
    async def test_under_limit_proceeds(self):
        kernel = MockKernelClient()
        config = _pipeline("hop_ok", [
            _stage("a", default_next="b"),
            _stage("b"),
        ], max_agent_hops=5)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        result = await kernel.report_agent_result("p1", "a", output={"x": 1})
        assert result.kind == "RUN_AGENT"
        assert result.agents == ["b"]


# =============================================================================
# Gate nodes
# =============================================================================

class TestGateNode:
    @pytest.mark.asyncio
    async def test_routes_without_running_agent(self):
        """Gate evaluates routing and dispatches target — no agent execution."""
        kernel = MockKernelClient()
        config = _pipeline("gate_test", [
            _stage("a", default_next="gate"),
            _stage("gate", node_kind="Gate", routing=[
                {"expr": {"op": "Eq", "field": {"scope": "Current", "key": "intent"}, "value": "search"}, "target": "search"},
            ], default_next="fallback"),
            _stage("search"),
            _stage("fallback"),
        ], max_iterations=10)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        # report_agent_result for "a" → routes to gate → gate evaluates → routes to "search"
        result = await kernel.report_agent_result("p1", "a", output={"intent": "search"})
        assert result.kind == "RUN_AGENT"
        assert result.agents == ["search"]

    @pytest.mark.asyncio
    async def test_default_next_on_no_match(self):
        kernel = MockKernelClient()
        config = _pipeline("gate_default", [
            _stage("a", default_next="gate"),
            _stage("gate", node_kind="Gate", routing=[
                {"expr": {"op": "Eq", "field": {"scope": "Current", "key": "intent"}, "value": "search"}, "target": "search"},
            ], default_next="fallback"),
            _stage("search"),
            _stage("fallback"),
        ], max_iterations=10)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        result = await kernel.report_agent_result("p1", "a", output={"intent": "chat"})
        assert result.kind == "RUN_AGENT"
        assert result.agents == ["fallback"]

    @pytest.mark.asyncio
    async def test_no_match_no_default_terminates(self):
        kernel = MockKernelClient()
        config = _pipeline("gate_term", [
            _stage("a", default_next="gate"),
            _stage("gate", node_kind="Gate", routing=[
                {"expr": {"op": "Eq", "field": {"scope": "Current", "key": "intent"}, "value": "search"}, "target": "search"},
            ]),
            _stage("search"),
        ], max_iterations=10)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        result = await kernel.report_agent_result("p1", "a", output={"intent": "chat"})
        assert result.kind == "TERMINATE"
        assert result.terminal_reason == "COMPLETED"


# =============================================================================
# Fork: WaitFirst join
# =============================================================================

class TestForkWaitFirst:
    @pytest.mark.asyncio
    async def test_advances_after_first_branch(self):
        """WaitFirst: first branch completion triggers join to default_next."""
        kernel = MockKernelClient()
        config = _pipeline("fork_first", [
            _stage("a", default_next="fork"),
            _stage("fork", node_kind="Fork", join_strategy="WaitFirst",
                   routing=[
                       {"expr": {"op": "Always"}, "target": "b1"},
                       {"expr": {"op": "Always"}, "target": "b2"},
                   ], default_next="done"),
            _stage("b1"),
            _stage("b2"),
            _stage("done"),
        ], max_iterations=10, max_agent_hops=10)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        # report "a" → routes through fork → dispatches [b1, b2]
        fan_out = await kernel.report_agent_result("p1", "a", output={"x": 1})
        assert fan_out.kind == "RUN_AGENTS"
        assert set(fan_out.agents) == {"b1", "b2"}
        # Report first branch → WAIT_PARALLEL (WaitFirst join met)
        wait = await kernel.report_agent_result("p1", "b1", output={"r": 1})
        assert wait.kind == "WAIT_PARALLEL"
        # Next instruction should route to "done"
        instr = await kernel.get_next_instruction("p1")
        assert instr.kind == "RUN_AGENT"
        assert instr.agents == ["done"]


# =============================================================================
# Break loop
# =============================================================================

class TestBreakLoop:
    @pytest.mark.asyncio
    async def test_terminates(self):
        kernel = MockKernelClient()
        config = _pipeline("break_test", [
            _stage("a", default_next="b"),
            _stage("b"),
        ], max_iterations=10)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        result = await kernel.report_agent_result("p1", "a", output={"x": 1}, break_loop=True)
        assert result.kind == "TERMINATE"
        assert result.terminal_reason == "BREAK_REQUESTED"

    @pytest.mark.asyncio
    async def test_outputs_preserved(self):
        kernel = MockKernelClient()
        config = _pipeline("break_out", [_stage("a")], max_iterations=10)
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        result = await kernel.report_agent_result(
            "p1", "a", output={"result": 42}, break_loop=True,
        )
        assert result.outputs["a"]["result"] == 42


# =============================================================================
# State merge strategies
# =============================================================================

class TestStateMerge:
    @pytest.mark.asyncio
    async def test_replace_strategy(self):
        kernel = MockKernelClient()
        config = _pipeline("state_replace", [
            _stage("a", output_key="data", default_next="b"),
            _stage("b", output_key="data"),
        ], max_iterations=10, state_schema=[{"key": "data", "merge": "Replace"}])
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        await kernel.report_agent_result("p1", "a", output={"v": 1})
        await kernel.get_next_instruction("p1")
        await kernel.report_agent_result("p1", "b", output={"v": 2})
        session = kernel._sessions["p1"]
        assert session.state["data"] == {"v": 2}

    @pytest.mark.asyncio
    async def test_append_strategy(self):
        kernel = MockKernelClient()
        config = _pipeline("state_append", [
            _stage("a", output_key="items", default_next="b"),
            _stage("b", output_key="items"),
        ], max_iterations=10, state_schema=[{"key": "items", "merge": "Append"}])
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        await kernel.report_agent_result("p1", "a", output={"v": 1})
        await kernel.get_next_instruction("p1")
        await kernel.report_agent_result("p1", "b", output={"v": 2})
        session = kernel._sessions["p1"]
        assert session.state["items"] == [{"v": 1}, {"v": 2}]

    @pytest.mark.asyncio
    async def test_merge_dict_strategy(self):
        kernel = MockKernelClient()
        config = _pipeline("state_merge", [
            _stage("a", output_key="ctx", default_next="b"),
            _stage("b", output_key="ctx"),
        ], max_iterations=10, state_schema=[{"key": "ctx", "merge": "MergeDict"}])
        await kernel.initialize_orchestration_session("p1", config)
        await kernel.get_next_instruction("p1")
        await kernel.report_agent_result("p1", "a", output={"x": 1})
        await kernel.get_next_instruction("p1")
        await kernel.report_agent_result("p1", "b", output={"y": 2})
        session = kernel._sessions["p1"]
        assert session.state["ctx"] == {"x": 1, "y": 2}
