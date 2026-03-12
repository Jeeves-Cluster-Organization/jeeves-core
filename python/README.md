# jeeves-core

Python infrastructure layer for the Jeeves micro-kernel. Provides LLM providers, gateway, pipeline execution, bootstrap, and protocol types — consumed by capabilities as a library.

## Install

```bash
pip install jeeves-core
```

## Usage

```python
from jeeves_core.bootstrap import create_app_context
from jeeves_core.api import PipelineConfig, AgentConfig, stage, eq

# Bootstrap provisions: kernel_client, llm_provider_factory, config_registry
app_context = create_app_context()
```

## Prerequisites

- Python 3.11+
- A running [jeeves-core](https://github.com/Jeeves-Cluster-Organization/jeeves-core) kernel (`cargo run --release` on port 50051)

## Modules

| Module | Description |
|--------|-------------|
| `jeeves_core.api` | **Single-import surface** for capability definition and registration |
| `jeeves_core.kernel_client` | IPC bridge to Rust kernel (TCP+msgpack) |
| `jeeves_core.gateway` | FastAPI HTTP/WS/SSE server |
| `jeeves_core.llm` | LLM provider abstraction (OpenAI, LiteLLM, mock) |
| `jeeves_core.bootstrap` | AppContext creation, composition root |
| `jeeves_core.protocols` | Type definitions and interfaces |
| `jeeves_core.runtime` | Agent and pipeline execution |

## Creating a Capability

```python
from jeeves_core.api import (
    PipelineConfig, stage, eq, always,
    DomainServiceConfig, register_capability,
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

# 3. Register
register_capability(
    capability_id="my_capability",
    service_config=DomainServiceConfig(
        service_id="my_service", service_type="flow",
        capabilities=["my_pipeline"], max_concurrent=5,
    ),
)
```

`jeeves_core.api` re-exports everything needed: pipeline types, routing builders (`eq`, `not_`, `always`, `agent`, `meta`, `state`), wiring types, `DeterministicAgent`, and test helpers.

## License

Apache License 2.0
