"""TestPipeline — High-level pipeline test runner.

Runs a full pipeline with mock LLM/tools using MockKernelClient.
No Rust kernel required.
"""

from typing import Any, Callable, Dict, Optional
from uuid import uuid4

from jeeves_core.protocols.types import PipelineConfig
from jeeves_core.pipeline_worker import PipelineWorker, WorkerResult
from jeeves_core.testing.mock_kernel import MockKernelClient
from jeeves_core.testing.helpers import make_envelope
from jeeves_core.runtime.agents import (
    Agent,
    PipelineRunner,
    _NullLogger,
)


class _MockToolExecutor:
    """Tool executor that returns canned responses."""

    def __init__(self, mock_tools: Dict[str, Any]):
        self._tools = mock_tools

    async def execute(self, tool_name: str, params: Dict[str, Any]) -> Dict[str, Any]:
        result = self._tools.get(tool_name)
        if result is None:
            raise ValueError(f"Unknown mock tool: {tool_name}")
        if callable(result):
            return result(params)
        return result


class _MockLLM:
    """LLM provider that returns canned output dicts."""

    def __init__(self, outputs: Dict[str, Dict]):
        self._outputs = outputs

    async def chat(self, model: str, messages: list = None, options: dict = None) -> dict:
        import json
        for key, output in self._outputs.items():
            return {"content": json.dumps(output), "tool_calls": []}
        return {"content": "{}", "tool_calls": []}

    async def chat_with_usage(self, model: str, messages: list = None, options: dict = None):
        result = await self.chat(model, messages, options)
        return result, {"prompt_tokens": 10, "completion_tokens": 10}


class _MockLLMByAgent:
    """LLM that dispatches canned responses by agent output_key."""

    def __init__(self, agent_name: str, outputs: Dict[str, Dict]):
        self._agent_name = agent_name
        self._outputs = outputs

    async def chat(self, model: str, messages: list = None, options: dict = None) -> dict:
        import json
        output = self._outputs.get(self._agent_name, {})
        return {"content": json.dumps(output), "tool_calls": []}

    async def chat_with_usage(self, model: str, messages: list = None, options: dict = None):
        result = await self.chat(model, messages, options)
        return result, {"prompt_tokens": 10, "completion_tokens": 10}


class _MockPromptRegistry:
    """Prompt registry that returns a generic prompt."""

    def get(self, key: str, **kwargs) -> str:
        return f"Test prompt for {key}"


class TestPipeline:
    """High-level test runner for pipeline configurations."""

    def __init__(self, config: PipelineConfig):
        self.config = config

    async def run(
        self,
        message: str = "test",
        *,
        mock_llm: Optional[Dict[str, Dict]] = None,
        mock_tools: Optional[Dict[str, Any]] = None,
        metadata: Optional[Dict[str, Any]] = None,
        user_id: str = "test-user",
        session_id: str = "test-session",
    ) -> WorkerResult:
        """Run full pipeline with mocks. Returns WorkerResult for assertions."""
        mock_llm = mock_llm or {}
        mock_tools = mock_tools or {}

        # Build agents with mock LLM/tools
        tool_executor = _MockToolExecutor(mock_tools) if mock_tools else None
        logger = _NullLogger()
        prompt_registry = _MockPromptRegistry()

        agents: Dict[str, Agent] = {}
        for agent_config in self.config.agents:
            llm = None
            if agent_config.has_llm:
                llm = _MockLLMByAgent(agent_config.output_key, mock_llm)

            tools = None
            if agent_config.has_tools or agent_config.tool_dispatch:
                tools = tool_executor

            agent = Agent(
                config=agent_config,
                logger=logger,
                llm=llm,
                tools=tools,
                prompt_registry=prompt_registry,
                pre_process=agent_config.pre_process,
                post_process=agent_config.post_process,
                mock_handler=agent_config.mock_handler,
            )
            agents[agent_config.name] = agent

        # Create mock kernel + worker
        kernel = MockKernelClient()
        worker = PipelineWorker(
            kernel_client=kernel,
            agents=agents,
            logger=logger,
        )

        # Build envelope
        envelope = make_envelope(
            message=message,
            user_id=user_id,
            session_id=session_id,
            **(metadata or {}),
        )

        # Execute
        process_id = f"test-{uuid4().hex[:8]}"
        pipeline_config_dict = self.config.to_kernel_dict()

        return await worker.execute(
            process_id=process_id,
            pipeline_config=pipeline_config_dict,
            envelope=envelope,
        )
