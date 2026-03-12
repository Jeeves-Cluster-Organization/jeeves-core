# jeeves-core

Python infrastructure layer for the Jeeves micro-kernel. Provides LLM providers, gateway, pipeline execution, bootstrap, and protocol types — consumed by capabilities as a library.

## Install

```bash
pip install jeeves-core
```

## Usage

```python
from jeeves_core import PipelineConfig, stage, eq, create_app_context

# Bootstrap provisions: kernel_client, llm_provider_factory, config_registry
app_context = create_app_context()
```

## Prerequisites

- Python 3.11+
- A running [jeeves-core](https://github.com/Jeeves-Cluster-Organization/jeeves-core) kernel (`cargo run --release` on port 50051)

## Modules

| Module | Description |
|--------|-------------|
| `jeeves_core` | **Top-level import surface** — pipeline types, routing DSL, registration, runtime |
| `jeeves_core.kernel_client` | IPC bridge to Rust kernel (TCP+msgpack) |
| `jeeves_core.gateway` | FastAPI HTTP/WS/SSE server |
| `jeeves_core.llm` | LLM provider abstraction (OpenAI, LiteLLM, mock) |
| `jeeves_core.bootstrap` | AppContext creation, composition root |
| `jeeves_core.protocols` | Type definitions and interfaces |
| `jeeves_core.runtime` | Agent and pipeline execution |

## Creating a Capability

```python
from jeeves_core import (
    PipelineConfig, stage, eq, always,
    register_capability,
    DeterministicAgent, AgentContext,
)

# 1. Define a deterministic agent (no LLM)
class MyPreprocessor(DeterministicAgent):
    async def execute(self, context: AgentContext) -> dict:
        context.metadata["enriched"] = True
        return {"status": "ready"}

# 2. Define your pipeline
config = PipelineConfig(
    name="my_pipeline",
    agents=[
        stage("preprocess", agent_class=MyPreprocessor, default_next="respond"),
        stage("respond", prompt_key="my.respond"),
    ],
    max_iterations=3, max_llm_calls=2, max_agent_hops=3,
)

# 3. Register — PipelineConfig IS the deployment spec
register_capability("my_capability", config, is_default=True)
```

`jeeves_core` exports everything needed: pipeline types, routing builders (`eq`, `not_`, `always`, `agent`, `meta`, `state`), wiring types, `DeterministicAgent`, and runtime.

## License

Apache License 2.0
