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

## License

Apache License 2.0
