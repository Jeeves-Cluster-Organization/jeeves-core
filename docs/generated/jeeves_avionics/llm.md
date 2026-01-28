# LLM Module

LLM provider abstraction layer with pluggable adapters.

## Navigation

- [README](./README.md) - Overview
- [Settings](./settings.md)
- [Gateway](./gateway.md)
- [Database](./database.md)
- **LLM** (this file)
- [Observability](./observability.md)
- [Tools](./tools.md)
- [Infrastructure](./infrastructure.md)

---

## Architecture Overview

The LLM module uses a **lazy-loading adapter pattern** for provider flexibility.

### Adapter Architecture

```
┌─────────────────────────────────────────┐
│  Factory (factory.py)                    │
│  - Lazy-loads adapters on first use      │
│  - Reads JEEVES_LLM_* env vars           │
│  - Caches provider instances             │
└─────────────┬───────────────────────────┘
              │ creates
┌─────────────▼───────────────────────────┐
│  Adapters (providers/)                   │
│  - OpenAIHTTPProvider (default, zero deps)│
│  - LiteLLMProvider (optional)            │
│  - MockProvider (testing)                │
└─────────────────────────────────────────┘
```

### Configuration

Environment variables (preferred):
```bash
JEEVES_LLM_ADAPTER=openai_http   # or: litellm, mock
JEEVES_LLM_BASE_URL=http://localhost:8080/v1
JEEVES_LLM_MODEL=qwen2.5-7b-instruct
JEEVES_LLM_API_KEY=...          # optional for local servers
JEEVES_LLM_TIMEOUT=120          # seconds
JEEVES_LLM_MAX_RETRIES=3
```

---

## Adapters

| Adapter | Description | Install |
|---------|-------------|---------|
| `openai_http` | Direct HTTP to OpenAI-compatible endpoints (default) | Built-in |
| `mock` | Testing provider | Built-in |
| `litellm` | LiteLLM for 100+ cloud providers | `pip install avionics[litellm]` |

### Supported Backends (via openai_http)

- **llama-server** (llama.cpp) - local inference
- **vLLM** - high-throughput serving
- **SGLang** - fast structured generation
- **Any OpenAI API-compatible server**

---

## llm/providers/base.py

Abstract base class for LLM providers.

### Dataclass: TokenChunk

```python
@dataclass
class TokenChunk:
    """A chunk of streamed tokens from an LLM."""
    text: str
    is_final: bool = False
    token_count: int = 0
    metadata: Optional[Dict[str, Any]] = None
```

### Class: LLMProvider

```python
class LLMProvider(ABC):
    """Abstract base class for LLM providers."""

    @abstractmethod
    async def generate(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> str:
        """Generate text from a prompt."""

    async def generate_stream(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> AsyncIterator[TokenChunk]:
        """Generate text with streaming output (default: fallback to generate())."""

    @abstractmethod
    async def health_check(self) -> bool:
        """Check if the provider is available and responding."""

    @property
    def supports_streaming(self) -> bool:
        """Check if this provider supports true streaming."""
        return False
```

---

## llm/providers/openai_http_provider.py

Direct HTTP adapter for OpenAI-compatible endpoints.

### Class: OpenAIHTTPProvider

```python
class OpenAIHTTPProvider(LLMProvider):
    """LLM provider using direct OpenAI-compatible HTTP calls.

    This is the zero-dependency default adapter. Works with any server
    that implements the OpenAI chat completions API:
    - llama-server (llama.cpp)
    - vLLM
    - SGLang
    - OpenAI API
    """

    def __init__(
        self,
        model: str,
        api_base: Optional[str] = None,  # Default: http://localhost:8080/v1
        api_key: Optional[str] = None,
        timeout: float = 120.0,
        max_retries: int = 3,
        logger: Optional[LoggerProtocol] = None,
    ):
        ...

    supports_streaming = True
```

#### Methods

```python
async def generate(self, model: str, prompt: str, options: Optional[Dict] = None) -> str:
    """Generate text completion via OpenAI-compatible API."""

async def generate_stream(self, model: str, prompt: str, options: Optional[Dict] = None) -> AsyncIterator[TokenChunk]:
    """Generate text with streaming via SSE."""

async def health_check(self) -> bool:
    """Check if endpoint is healthy (calls /models endpoint)."""
```

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `temperature` | - | Sampling temperature |
| `max_tokens` | - | Max tokens to generate |
| `top_p` | - | Top-p sampling |
| `stop` | - | Stop sequences |
| `presence_penalty` | - | Presence penalty |
| `frequency_penalty` | - | Frequency penalty |

---

## llm/providers/litellm_provider.py

LiteLLM adapter for 100+ cloud providers.

**Installation:** `pip install avionics[litellm]`

### Class: LiteLLMProvider

```python
class LiteLLMProvider(LLMProvider):
    """LLM provider using LiteLLM for multi-provider support.

    Supports OpenAI, Anthropic, Azure, Bedrock, Vertex, and 100+ more.
    """

    def __init__(
        self,
        model: str,
        api_base: Optional[str] = None,
        api_key: Optional[str] = None,
        timeout: float = 120.0,
        max_retries: int = 3,
        logger: Optional[LoggerProtocol] = None,
    ):
        ...

    supports_streaming = True
```

---

## llm/factory.py

Factory for creating LLM providers with lazy adapter loading.

### Functions

```python
def create_llm_provider(
    settings: Settings,
    agent_name: Optional[str] = None,
) -> LLMProviderProtocol:
    """Create an LLM provider based on configuration.

    Priority:
    1. MOCK_LLM_ENABLED=true -> MockProvider
    2. JEEVES_LLM_ADAPTER env var -> specified adapter
    3. Default: openai_http (zero external deps)

    Raises:
        ImportError: If requested adapter is not available
    """

def get_available_adapters() -> list[str]:
    """Return list of available adapter names."""

def create_llm_provider_factory(settings: Settings) -> Callable[[str], LLMProviderProtocol]:
    """Create a factory function for LLM providers."""
```

### Class: LLMFactory

```python
class LLMFactory:
    """Centralized LLM provider factory with caching."""

    def __init__(self, settings: Settings):
        ...

    def get_provider_for_agent(self, agent_name: str, use_cache: bool = True) -> LLMProviderProtocol:
        """Get provider for agent (with optional caching)."""

    def clear_cache(self) -> None:
        """Clear provider cache."""
```

---

## llm/gateway.py

Unified LLM gateway with cost tracking and provider fallback.

### Dataclass: StreamingChunk

```python
@dataclass
class StreamingChunk:
    text: str
    is_final: bool
    request_id: str
    agent_name: str
    provider: str
    model: str
    cumulative_tokens: int
    timestamp: datetime
```

### Dataclass: LLMResponse

```python
@dataclass
class LLMResponse:
    text: str
    tool_calls: List[Dict[str, Any]]
    tokens_used: int
    prompt_tokens: int
    completion_tokens: int
    latency_ms: float
    provider: str
    model: str
    cost_usd: float
    timestamp: datetime
    metadata: Dict[str, Any]
    streamed: bool = False
```

### Class: LLMGateway

```python
class LLMGateway:
    """Unified gateway for all LLM interactions.

    Features:
    - Automatic cost tracking per request
    - Provider fallback on failures
    - Performance monitoring
    - Resource tracking callbacks
    - Streaming support
    """
```

---

## Usage Examples

### Basic Generation

```python
from jeeves_infra.llm import create_llm_provider
from avionics.settings import get_settings

settings = get_settings()
provider = create_llm_provider(settings)

response = await provider.generate(
    model="qwen2.5-7b",
    prompt="What is 2+2?",
    options={"temperature": 0.3, "max_tokens": 100}
)
print(response)
```

### Streaming

```python
async for chunk in provider.generate_stream(
    model="qwen2.5-7b",
    prompt="Explain Python decorators",
):
    print(chunk.text, end="", flush=True)
    if chunk.is_final:
        print("\n\nDone!")
```

### Check Available Adapters

```python
from jeeves_infra.llm import get_available_adapters

adapters = get_available_adapters()
print(f"Available: {adapters}")
# Output: ['mock', 'openai_http'] or ['mock', 'openai_http', 'litellm']
```

---

*Last updated: 2026-01-27*
