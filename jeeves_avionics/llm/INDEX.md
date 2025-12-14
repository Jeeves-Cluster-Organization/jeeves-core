# Jeeves Avionics - LLM Module

**Parent:** [Avionics Index](../INDEX.md)

---

## Overview

LLM provider abstraction layer supporting multiple backends: OpenAI, Anthropic, LlamaCpp, Azure, and local llamaserver. Implements `LLMProviderProtocol` from `jeeves_protocols`.

---

## Files

| File | Description |
|------|-------------|
| `__init__.py` | Package exports |
| `factory.py` | `create_llm_provider()` factory for provider instantiation |
| `gateway.py` | LLM Gateway for centralized API management |
| `providers/base.py` | Base `LLMProvider` class and common utilities |
| `cost_calculator.py` | Token cost calculation utilities |
| `providers/` | Provider implementations (subdirectory) |

## Subdirectories

| Directory | Description |
|-----------|-------------|
| `providers/` | Concrete provider implementations |

---

## Key Components

### Provider Factory (`factory.py`)
```python
def create_llm_provider(
    provider_type: str,
    settings: Settings,
    agent_name: Optional[str] = None,
) -> LLMProviderProtocol
```

Supported provider types:
- `llamaserver` - Local LlamaServer instance
- `llamacpp` - Direct LlamaCpp integration
- `openai` - OpenAI API
- `anthropic` - Anthropic API
- `azure` - Azure OpenAI
- `mock` - Mock provider for testing

### LLM Gateway (`gateway.py`)
Centralized LLM request routing with:
- Rate limiting
- Request queuing
- Load balancing (future)
- Metrics collection

---

## Protocol Implementation

Implements `LLMProviderProtocol` from `jeeves_protocols`:
```python
class LLMProviderProtocol(Protocol):
    async def generate(self, prompt: str, ...) -> str
    async def generate_structured(self, prompt: str, schema: Type[T], ...) -> T
    async def health_check(self) -> bool
```

---

*Last updated: 2025-12-14*
