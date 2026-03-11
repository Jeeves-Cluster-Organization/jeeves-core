"""Capability scaffolding — generate a working capability skeleton.

Usage:
    python -m jeeves_core.cli.scaffold --name my_capability --id my_cap
    python -m jeeves_core.cli.scaffold --name my_capability --id my_cap --dry-run
"""

import argparse
import os
import sys
from pathlib import Path


def _pyproject_toml(name: str, cap_id: str) -> str:
    return f'''[project]
name = "jeeves-capability-{cap_id}"
version = "0.0.1"
description = "{name} capability for Jeeves"
requires-python = ">=3.11"
dependencies = ["jeeves-core"]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
'''


def _init_py(name: str, cap_id: str) -> str:
    return f'''"""Jeeves capability: {name}."""

CAPABILITY_ID = "{cap_id}"

from {name}.capability.wiring import register_capability  # noqa: F401, E402

__all__ = ["CAPABILITY_ID", "register_capability"]
'''


def _pipeline_config_py(name: str, cap_id: str) -> str:
    return f'''"""Pipeline configuration for {cap_id}."""

from jeeves_core.protocols import PipelineConfig, AgentConfig


{cap_id.upper()}_PIPELINE = PipelineConfig.chain(
    "{cap_id}",
    [
        AgentConfig(
            name="understand",
            has_llm=True,
            model_role="planner",
            prompt_key="{cap_id}.understand",
            output_key="understanding",
            output_schema={{"type": "object", "properties": {{"intent": {{"type": "string"}}}}, "required": ["intent"]}},
            max_tokens=1000,
            temperature=0.3,
        ),
        AgentConfig(
            name="think",
            has_llm=False,
            output_key="thinking",
        ),
        AgentConfig(
            name="respond",
            has_llm=True,
            model_role="planner",
            prompt_key="{cap_id}.respond",
            output_key="final_response",
            max_tokens=2000,
            temperature=0.5,
        ),
    ],
    max_iterations=1,
    max_llm_calls=2,
    max_agent_hops=3,
    error_next="respond",
)
'''


def _wiring_py(name: str, cap_id: str) -> str:
    return f'''"""Capability registration for {cap_id}."""

from jeeves_core.protocols.capability import get_capability_resource_registry
from {name}.pipeline_config import {cap_id.upper()}_PIPELINE
from {name}.orchestration.service import {_class_name(cap_id)}Service


def register_capability():
    """Register {cap_id} capability with the resource registry."""
    registry = get_capability_resource_registry()
    registry.register(
        capability_id="{cap_id}",
        pipeline_config={cap_id.upper()}_PIPELINE,
        service_class={_class_name(cap_id)}Service,
    )
'''


def _service_py(name: str, cap_id: str) -> str:
    return f'''"""Service for {cap_id} capability."""

from jeeves_core.runtime.capability_service import CapabilityService


class {_class_name(cap_id)}Service(CapabilityService):
    """Kernel-driven service for {cap_id}."""

    capability_id = "{cap_id}"

    async def _enrich_metadata(self, meta, message, user_id, session_id):
        meta["user_message"] = message
'''


def _example_tools_py(name: str, cap_id: str) -> str:
    return f'''"""Example tools for {cap_id}."""

from jeeves_core.tools.decorator import tool


@tool(description="Example tool", category="{cap_id}", risk="read_only/low")
async def example_tool(query: str, *, db=None):
    """Example tool that echoes the query."""
    return {{"status": "success", "result": query}}
'''


def _templates_py(name: str, cap_id: str) -> str:
    return f'''"""Prompt templates for {cap_id}."""

UNDERSTAND_PROMPT = """You are the understand agent for {cap_id}.
Classify the user\'s intent.

User: {{user_message}}

Respond as JSON: {{"intent": "...", "topic": "..."}}"""

RESPOND_PROMPT = """You are the respond agent for {cap_id}.
Generate a helpful response.

User: {{user_message}}
Intent: {{intent}}

Respond as JSON: {{"response": "..."}}"""
'''


def _test_pipeline_py(name: str, cap_id: str) -> str:
    return f'''"""Pipeline smoke test for {cap_id}."""

import asyncio
from jeeves_core.testing import TestPipeline
from {name}.pipeline_config import {cap_id.upper()}_PIPELINE


async def test_pipeline_runs():
    harness = TestPipeline({cap_id.upper()}_PIPELINE)
    result = await harness.run(
        "Hello",
        mock_llm={{
            "understanding": {{"intent": "general", "topic": "greeting"}},
            "final_response": {{"response": "Hello! How can I help?"}},
        }},
    )
    assert result.terminated
    assert result.terminal_reason == "COMPLETED"


if __name__ == "__main__":
    asyncio.run(test_pipeline_runs())
    print("OK: pipeline smoke test passed")
'''


def _class_name(cap_id: str) -> str:
    """Convert cap_id to PascalCase class name."""
    return "".join(word.capitalize() for word in cap_id.split("_"))


# Files to generate: (relative_path, generator_function)
def _get_file_map(name: str, cap_id: str):
    return [
        ("pyproject.toml", _pyproject_toml),
        (f"{name}/__init__.py", _init_py),
        (f"{name}/pipeline_config.py", _pipeline_config_py),
        (f"{name}/capability/__init__.py", lambda n, c: ""),
        (f"{name}/capability/wiring.py", _wiring_py),
        (f"{name}/orchestration/__init__.py", lambda n, c: ""),
        (f"{name}/orchestration/service.py", _service_py),
        (f"{name}/tools/__init__.py", lambda n, c: ""),
        (f"{name}/tools/example_tools.py", _example_tools_py),
        (f"{name}/prompts/__init__.py", lambda n, c: ""),
        (f"{name}/prompts/templates.py", _templates_py),
        ("tests/__init__.py", lambda n, c: ""),
        ("tests/test_pipeline.py", _test_pipeline_py),
    ]


def scaffold(name: str, cap_id: str, output_dir: str, dry_run: bool = False) -> None:
    """Generate capability skeleton."""
    root = Path(output_dir)
    file_map = _get_file_map(name, cap_id)

    for rel_path, generator in file_map:
        full_path = root / rel_path
        content = generator(name, cap_id)

        if dry_run:
            print(f"  {rel_path}")
        else:
            full_path.parent.mkdir(parents=True, exist_ok=True)
            full_path.write_text(content, encoding="utf-8")
            print(f"  created: {rel_path}")

    if dry_run:
        print(f"\n(dry run — {len(file_map)} files would be created in {root})")
    else:
        print(f"\nScaffolded {len(file_map)} files in {root}")


def main():
    parser = argparse.ArgumentParser(description="Scaffold a new Jeeves capability")
    parser.add_argument("--name", required=True, help="Python package name (e.g., my_capability)")
    parser.add_argument("--id", required=True, help="Capability ID (e.g., my_cap)")
    parser.add_argument("--output", default=None, help="Output directory (default: jeeves-capability-<id>/)")
    parser.add_argument("--dry-run", action="store_true", help="Show files without creating")
    args = parser.parse_args()

    output_dir = args.output or f"jeeves-capability-{args.id}"
    scaffold(args.name, args.id, output_dir, dry_run=args.dry_run)


if __name__ == "__main__":
    main()
