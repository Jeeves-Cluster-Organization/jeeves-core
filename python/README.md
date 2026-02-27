# jeeves-infra

Python infrastructure layer for the Jeeves micro-kernel. Provides LLM providers, gateway, pipeline execution, bootstrap, and protocol types — consumed by capabilities as a library.

## Install

```bash
pip install jeeves-infra
```

## Usage

```python
from jeeves_infra.bootstrap import create_app_context
from jeeves_infra.protocols import PipelineConfig, AgentConfig, Envelope

# Bootstrap provisions: kernel_client, llm_provider_factory, config_registry
app_context = create_app_context()
```

## Prerequisites

- Python 3.11+
- A running [jeeves-core](https://github.com/Jeeves-Cluster-Organization/jeeves-core) kernel (`cargo run --release` on port 50051)

## Modules

| Module | Description |
|--------|-------------|
| `jeeves_infra.kernel_client` | IPC bridge to Rust kernel (TCP+msgpack) |
| `jeeves_infra.gateway` | FastAPI HTTP/WS/SSE server |
| `jeeves_infra.llm` | LLM provider abstraction (OpenAI, LiteLLM, mock) |
| `jeeves_infra.bootstrap` | AppContext creation, composition root |
| `jeeves_infra.protocols` | Type definitions and interfaces |
| `jeeves_infra.runtime` | Agent and pipeline execution |

## License

Apache License 2.0
