"""Evaluation Framework — run test suites against pipeline configs.

Built on TestPipeline + MockKernelClient. Provides declarative eval cases,
a runner, and a report.
"""

import time
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

from jeeves_core.protocols.types import PipelineConfig
from jeeves_core.testing.test_pipeline import TestPipeline


@dataclass
class EvalCase:
    """Declarative test case for pipeline evaluation."""
    name: str
    input: str
    mock_llm: Dict[str, Dict[str, Any]] = field(default_factory=dict)
    mock_tools: Optional[Dict[str, Any]] = None
    metadata: Optional[Dict[str, Any]] = None
    expect_terminated: bool = True
    expect_reason: str = "COMPLETED"
    expect_outputs: Optional[Dict[str, Any]] = None  # partial match
    max_duration_ms: Optional[int] = None


@dataclass
class EvalResult:
    """Result of a single eval case."""
    case: str
    passed: bool
    duration_ms: int
    error: str = ""
    actual_outputs: Optional[Dict[str, Any]] = None
    failure_reasons: List[str] = field(default_factory=list)


@dataclass
class EvalReport:
    """Aggregate report from an evaluation suite."""
    total: int
    passed: int
    failed: int
    results: List[EvalResult]

    @property
    def pass_rate(self) -> float:
        return self.passed / self.total if self.total else 0.0


class EvaluationRunner:
    """Run evaluation suites against a pipeline config."""

    def __init__(self, config: PipelineConfig):
        self._pipeline = TestPipeline(config)

    async def run_suite(self, cases: List[EvalCase]) -> EvalReport:
        results = []
        for case in cases:
            result = await self._run_case(case)
            results.append(result)
        return EvalReport(
            total=len(results),
            passed=sum(1 for r in results if r.passed),
            failed=sum(1 for r in results if not r.passed),
            results=results,
        )

    async def _run_case(self, case: EvalCase) -> EvalResult:
        t0 = time.time()
        try:
            result = await self._pipeline.run(
                case.input,
                mock_llm=case.mock_llm,
                mock_tools=case.mock_tools,
                metadata=case.metadata,
            )
            duration_ms = int((time.time() - t0) * 1000)

            failures = []
            if result.terminated != case.expect_terminated:
                failures.append(
                    f"terminated: expected={case.expect_terminated}, actual={result.terminated}"
                )
            if result.terminal_reason != case.expect_reason:
                failures.append(
                    f"reason: expected='{case.expect_reason}', actual='{result.terminal_reason}'"
                )
            if case.expect_outputs:
                for key, expected in case.expect_outputs.items():
                    actual = result.outputs.get(key, {})
                    if isinstance(expected, dict):
                        for k, v in expected.items():
                            if actual.get(k) != v:
                                failures.append(
                                    f"outputs[{key}][{k}]: expected={v!r}, actual={actual.get(k)!r}"
                                )
                    elif actual != expected:
                        failures.append(
                            f"outputs[{key}]: expected={expected!r}, actual={actual!r}"
                        )
            if case.max_duration_ms and duration_ms > case.max_duration_ms:
                failures.append(
                    f"duration: {duration_ms}ms > max {case.max_duration_ms}ms"
                )

            return EvalResult(
                case=case.name,
                passed=len(failures) == 0,
                duration_ms=duration_ms,
                actual_outputs=result.outputs,
                failure_reasons=failures,
            )

        except Exception as e:
            duration_ms = int((time.time() - t0) * 1000)
            return EvalResult(
                case=case.name,
                passed=False,
                duration_ms=duration_ms,
                error=str(e),
                failure_reasons=[f"exception: {e}"],
            )
