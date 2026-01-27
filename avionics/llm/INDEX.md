# Jeeves Avionics - LLM Module

**Parent:** [Avionics Index](../INDEX.md)

---

## Overview

LLM provider abstraction layer with pluggable adapters. Supports OpenAI-compatible
servers (llama-server, vLLM, SGLang) via direct HTTP, with optional LiteLLM for
100+ cloud providers. Implements `LLMProviderProtocol` from `protocols`.

---

## Configuration

Environment variables (preferred):
```bash
JEEVES_LLM_ADAPTER=openai_http   # or: litellm, mock
JEEVES_LLM_BASE_URL=http://localhost:8080/v1
JEEVES_LLM_MODEL=qwen2.5-7b-instruct
JEEVES_LLM_API_KEY=...          # optional for local servers
```

---

## Files

| File | Description |
|------|-------------|
| `__init__.py` | Package exports |
| `factory.py` | `create_llm_provider()` factory with lazy adapter loading |
| `gateway.py` | LLM Gateway for centralized API management |
| `providers/base.py` | Base `LLMProvider` class and `TokenChunk` |
| `providers/mock.py` | Mock provider for testing |
| `providers/openai_http_provider.py` | Direct HTTP adapter (zero deps) |
| `providers/litellm_provider.py` | LiteLLM adapter (optional) |
| `cost_calculator.py` | Token cost calculation utilities |

---

## Adapters

| Adapter | Description | Install |
|---------|-------------|---------|
| `openai_http` | Direct HTTP to OpenAI-compatible endpoints (default) | Built-in |
| `mock` | Testing provider | Built-in |
| `litellm` | LiteLLM for 100+ cloud providers | `pip install avionics[litellm]` |

### Supported Backends (via openai_http)
- llama-server (llama.cpp)
- vLLM
- SGLang
- Any OpenAI API-compatible server

---

## Factory (`factory.py`)

```python
from avionics.llm import create_llm_provider
from avionics.settings import get_settings

provider = create_llm_provider(get_settings())
response = await provider.generate("model", "Hello!")
```

Lazy-loads adapters on first use. Raises `ImportError` if requested adapter unavailable.

---

## Protocol Implementation

Implements `LLMProviderProtocol` from `protocols`:
```python
class LLMProviderProtocol(Protocol):
    async def generate(model: str, prompt: str, options: dict) -> str
    async def generate_stream(model: str, prompt: str, options: dict) -> AsyncIterator
    async def health_check() -> bool
```

---

*Last updated: 2026-01-27*
