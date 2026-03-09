# jeeves-core

Python infrastructure layer for the Jeeves micro-kernel. Provides LLM providers, gateway, pipeline execution, bootstrap, and protocol types — consumed by capabilities as a library.

## Install

```bash
pip install jeeves-core
```

## Usage

```python
from jeeves_core.bootstrap import create_app_context
from jeeves_core.protocols import PipelineConfig, AgentConfig, Envelope

# Bootstrap provisions: kernel_client, llm_provider_factory, config_registry
app_context = create_app_context()
```

## Prerequisites

- Python 3.11+
- A running [jeeves-core](https://github.com/Jeeves-Cluster-Organization/jeeves-core) kernel (`cargo run --release` on port 50051)

## Modules

| Module | Description |
|--------|-------------|
| `jeeves_core.kernel_client` | IPC bridge to Rust kernel (TCP+msgpack) |
| `jeeves_core.gateway` | FastAPI HTTP/WS/SSE server |
| `jeeves_core.llm` | LLM provider abstraction (OpenAI, LiteLLM, mock) |
| `jeeves_core.bootstrap` | AppContext creation, composition root |
| `jeeves_core.protocols` | Type definitions and interfaces |
| `jeeves_core.runtime` | Agent and pipeline execution |

## Creating a Capability

```python
from jeeves_core import (
    PipelineConfig, AgentConfig, RoutingRule,
    DomainServiceConfig, create_app_context, CapabilityService,
)
from jeeves_core.protocols.routing import eq, always

# 1. Define your pipeline
config = PipelineConfig(
    name="my_pipeline",
    agents=[
        AgentConfig(
            name="understand",
            agent="understand_agent",
            routing=[RoutingRule(expr=always(), target="respond")],
        ),
        AgentConfig(
            name="respond",
            agent="respond_agent",
        ),
    ],
)

# 2. Register your capability
service_config = DomainServiceConfig(
    name="my_capability",
    pipeline_configs={"default": config},
)

# 3. Bootstrap and run
app_context = create_app_context()
```

See `jeeves_core.protocols.routing` for expression builders (`eq()`, `not_()`, `and_()`, `agent()`, `meta()`).

## License

Apache License 2.0
