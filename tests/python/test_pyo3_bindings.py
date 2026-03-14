"""PyO3 binding integration tests for jeeves_core.

Tests PipelineRunner, EventIterator, @tool decorator, and metadata threading.
All tests use deterministic (no-LLM) pipelines with Python tool functions.

Run: pytest tests/python/test_pyo3_bindings.py -v
"""

import os
from pathlib import Path

import pytest

from jeeves_core import PipelineRunner, tool

FIXTURES = Path(__file__).parent
PIPELINE_JSON = str(FIXTURES / "pipeline_fixture.json")
TWO_STAGE_JSON = str(FIXTURES / "two_stage_fixture.json")
PROMPTS_DIR = str(FIXTURES / "prompts")


# ---------------------------------------------------------------------------
# Tool fixtures
# ---------------------------------------------------------------------------

@tool(name="process", description="Echo tool for testing")
def echo_tool(params):
    """Returns raw_input from params."""
    return {"echo": params.get("raw_input", ""), "processed": True}


@tool(
    name="process",
    description="Metadata-aware tool",
    parameters={"type": "object", "properties": {"raw_input": {"type": "string"}}},
)
def metadata_tool(params):
    """Returns metadata fields threaded through from runner."""
    meta = params.get("metadata", {})
    return {
        "saw_session_id": meta.get("session_id", ""),
        "saw_custom": meta.get("custom_key", ""),
        "raw_input": params.get("raw_input", ""),
    }


@tool(name="step_one", description="First stage tool")
def step_one_tool(params):
    return {"stage": "one", "input": params.get("raw_input", "")}


@tool(name="step_two", description="Second stage tool")
def step_two_tool(params):
    return {"stage": "two", "from_one": True}


# ---------------------------------------------------------------------------
# @tool decorator tests
# ---------------------------------------------------------------------------

class TestToolDecorator:
    """Tests for the @tool decorator."""

    def test_tool_sets_name_attribute(self):
        assert echo_tool._tool_name == "process"

    def test_tool_sets_description_attribute(self):
        assert echo_tool._tool_description == "Echo tool for testing"

    def test_tool_without_parameters_has_no_param_attr(self):
        # When parameters=None (default), no _tool_parameters attr is set
        assert not hasattr(echo_tool, "_tool_parameters") or echo_tool._tool_parameters is None

    def test_tool_with_parameters_sets_schema(self):
        assert hasattr(metadata_tool, "_tool_parameters")
        schema = metadata_tool._tool_parameters
        assert schema["type"] == "object"
        assert "raw_input" in schema["properties"]

    def test_tool_remains_callable(self):
        result = echo_tool({"raw_input": "hello"})
        assert result == {"echo": "hello", "processed": True}


# ---------------------------------------------------------------------------
# PipelineRunner construction tests
# ---------------------------------------------------------------------------

class TestPipelineRunnerConstruction:
    """Tests for PipelineRunner.from_json()."""

    def test_from_json_creates_runner(self):
        runner = PipelineRunner.from_json(
            pipeline_path=PIPELINE_JSON,
            prompts_dir=PROMPTS_DIR,
        )
        assert runner is not None

    def test_from_json_with_bad_path_raises(self):
        with pytest.raises(OSError):
            PipelineRunner.from_json(
                pipeline_path="/nonexistent/pipeline.json",
                prompts_dir=PROMPTS_DIR,
            )

    def test_from_json_with_invalid_json_raises(self, tmp_path):
        bad_file = tmp_path / "bad.json"
        bad_file.write_text("not valid json")
        with pytest.raises(ValueError):
            PipelineRunner.from_json(
                pipeline_path=str(bad_file),
                prompts_dir=PROMPTS_DIR,
            )

    def test_context_manager_protocol(self):
        with PipelineRunner.from_json(
            pipeline_path=PIPELINE_JSON,
            prompts_dir=PROMPTS_DIR,
        ) as runner:
            assert runner is not None


# ---------------------------------------------------------------------------
# PipelineRunner.run() tests
# ---------------------------------------------------------------------------

class TestPipelineRunnerRun:
    """Tests for buffered pipeline execution."""

    @pytest.fixture
    def runner(self):
        r = PipelineRunner.from_json(
            pipeline_path=PIPELINE_JSON,
            prompts_dir=PROMPTS_DIR,
        )
        r.register_tool(echo_tool)
        yield r
        r.shutdown()

    def test_run_returns_dict(self, runner):
        result = runner.run("hello world")
        assert isinstance(result, dict)

    def test_run_has_required_keys(self, runner):
        result = runner.run("hello")
        assert "outputs" in result
        assert "terminated" in result
        assert "terminal_reason" in result
        assert "process_id" in result

    def test_run_completes_successfully(self, runner):
        result = runner.run("test input")
        assert result["terminated"] is True
        assert result["terminal_reason"] in (None, "Completed")

    def test_run_tool_receives_context(self, runner):
        result = runner.run("specific input")
        outputs = result["outputs"]
        # The McpDelegatingAgent passes full context to the tool
        # Tool output stored under stage name "process"
        assert "process" in outputs

    def test_run_with_user_id(self, runner):
        result = runner.run("hello", user_id="test_user_42")
        assert result["terminated"] is True

    def test_run_with_session_id(self, runner):
        result = runner.run("hello", session_id="sess_123")
        assert result["terminated"] is True

    def test_run_with_metadata(self, runner):
        result = runner.run(
            "hello",
            metadata={"custom_key": "custom_value", "number": 42},
        )
        assert result["terminated"] is True


# ---------------------------------------------------------------------------
# Multi-stage pipeline tests
# ---------------------------------------------------------------------------

class TestMultiStagePipeline:
    """Tests for pipelines with multiple stages."""

    @pytest.fixture
    def runner(self):
        r = PipelineRunner.from_json(
            pipeline_path=TWO_STAGE_JSON,
            prompts_dir=PROMPTS_DIR,
        )
        r.register_tool(step_one_tool)
        r.register_tool(step_two_tool)
        yield r
        r.shutdown()

    def test_two_stage_pipeline_completes(self, runner):
        result = runner.run("multi stage test")
        assert result["terminated"] is True
        assert result["terminal_reason"] in (None, "Completed")

    def test_both_stages_produce_output(self, runner):
        result = runner.run("multi stage test")
        outputs = result["outputs"]
        assert "step_one" in outputs
        assert "step_two" in outputs


# ---------------------------------------------------------------------------
# PipelineRunner.stream() tests
# ---------------------------------------------------------------------------

class TestPipelineRunnerStream:
    """Tests for streaming pipeline execution."""

    @pytest.fixture
    def runner(self):
        r = PipelineRunner.from_json(
            pipeline_path=PIPELINE_JSON,
            prompts_dir=PROMPTS_DIR,
        )
        r.register_tool(echo_tool)
        yield r
        r.shutdown()

    def test_stream_returns_iterator(self, runner):
        it = runner.stream("hello")
        assert hasattr(it, "__iter__")
        assert hasattr(it, "__next__")

    def test_stream_yields_events(self, runner):
        events = list(runner.stream("hello"))
        assert len(events) > 0

    def test_stream_events_have_type_field(self, runner):
        events = list(runner.stream("hello"))
        for event in events:
            assert "type" in event, f"Event missing 'type': {event}"

    def test_stream_contains_stage_events(self, runner):
        events = list(runner.stream("hello"))
        event_types = [e["type"] for e in events]
        assert "stage_started" in event_types
        assert "stage_completed" in event_types

    def test_stream_contains_done_event(self, runner):
        events = list(runner.stream("hello"))
        done_events = [e for e in events if e["type"] == "done"]
        assert len(done_events) == 1

    def test_stream_done_carries_outputs(self, runner):
        events = list(runner.stream("hello"))
        done = next(e for e in events if e["type"] == "done")
        assert "outputs" in done
        assert done["outputs"] is not None

    def test_stream_tool_events_have_stage(self, runner):
        events = list(runner.stream("hello"))
        tool_starts = [e for e in events if e["type"] == "tool_call_start"]
        tool_results = [e for e in events if e["type"] == "tool_result"]
        for ev in tool_starts + tool_results:
            assert "stage" in ev


# ---------------------------------------------------------------------------
# Error handling tests
# ---------------------------------------------------------------------------

class TestErrorHandling:
    """Tests for error paths."""

    def test_register_tool_without_decorator_raises(self):
        runner = PipelineRunner.from_json(
            pipeline_path=PIPELINE_JSON,
            prompts_dir=PROMPTS_DIR,
        )
        with pytest.raises(AttributeError):
            runner.register_tool(lambda params: params)
        runner.shutdown()

    def test_run_unknown_pipeline_raises(self):
        runner = PipelineRunner.from_json(
            pipeline_path=PIPELINE_JSON,
            prompts_dir=PROMPTS_DIR,
        )
        with pytest.raises(KeyError):
            runner.run("hello", pipeline_name="nonexistent_pipeline")
        runner.shutdown()
