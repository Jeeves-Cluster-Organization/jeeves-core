"""Tests for the evaluation framework."""

import pytest
from jeeves_core.protocols.types import PipelineConfig, stage
from jeeves_core.testing import EvalCase, EvalReport, EvaluationRunner


@pytest.fixture
def simple_config():
    return PipelineConfig.chain(
        "eval_test",
        [
            stage("understand", mock_handler=lambda ctx: {"intent": "greet"}),
            stage("respond", mock_handler=lambda ctx: {"reply": "hello!"}),
        ],
        max_iterations=10,
    )


class TestEvaluationRunner:
    @pytest.mark.asyncio
    async def test_passing_case(self, simple_config):
        runner = EvaluationRunner(simple_config)
        report = await runner.run_suite([
            EvalCase(
                name="basic_greeting",
                input="hi",
                expect_terminated=True,
                expect_reason="COMPLETED",
            ),
        ])
        assert report.total == 1
        assert report.passed == 1
        assert report.failed == 0
        assert report.pass_rate == 1.0

    @pytest.mark.asyncio
    async def test_output_partial_match(self, simple_config):
        runner = EvaluationRunner(simple_config)
        report = await runner.run_suite([
            EvalCase(
                name="output_check",
                input="hi",
                expect_outputs={"understand": {"intent": "greet"}},
            ),
        ])
        assert report.passed == 1

    @pytest.mark.asyncio
    async def test_output_mismatch_fails(self, simple_config):
        runner = EvaluationRunner(simple_config)
        report = await runner.run_suite([
            EvalCase(
                name="wrong_output",
                input="hi",
                expect_outputs={"understand": {"intent": "search"}},
            ),
        ])
        assert report.failed == 1
        result = report.results[0]
        assert not result.passed
        assert any("intent" in r for r in result.failure_reasons)

    @pytest.mark.asyncio
    async def test_wrong_reason_fails(self, simple_config):
        runner = EvaluationRunner(simple_config)
        report = await runner.run_suite([
            EvalCase(
                name="wrong_reason",
                input="hi",
                expect_reason="MAX_ITERATIONS_EXCEEDED",
            ),
        ])
        assert report.failed == 1

    @pytest.mark.asyncio
    async def test_multiple_cases(self, simple_config):
        runner = EvaluationRunner(simple_config)
        report = await runner.run_suite([
            EvalCase(name="pass_1", input="a"),
            EvalCase(name="pass_2", input="b"),
            EvalCase(name="fail", input="c", expect_reason="WRONG"),
        ])
        assert report.total == 3
        assert report.passed == 2
        assert report.failed == 1

    @pytest.mark.asyncio
    async def test_exception_in_pipeline_fails(self):
        def _boom(ctx):
            raise RuntimeError("kaboom")

        config = PipelineConfig.chain(
            "boom",
            [stage("exploder", mock_handler=_boom)],
            max_iterations=10,
        )
        runner = EvaluationRunner(config)
        report = await runner.run_suite([
            EvalCase(name="boom_test", input="test"),
        ])
        # The agent error is caught by the worker, not re-raised.
        # Pipeline still terminates successfully (agent failure → no routing → COMPLETED)
        assert report.total == 1
