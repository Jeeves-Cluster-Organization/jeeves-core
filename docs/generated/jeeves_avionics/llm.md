# LLM Module

LLM provider abstraction layer for modular model execution.

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

Avionics LLM providers **delegate to jeeves-airframe** adapters for backend protocol handling.

### Layering

```
┌─────────────────────────────────────────┐
│  Avionics LLM Providers (L3)            │
│  - Cost tracking (token → USD)          │
│  - Settings integration                 │
│  - Telemetry enrichment                 │
│  - Model routing                        │
└─────────────┬───────────────────────────┘
              │ delegates to
┌─────────────▼───────────────────────────┐
│  Airframe Backend Adapters (L1)         │
│  - HTTP requests & SSE parsing          │
│  - Retry logic & error categorization   │
│  - Backend-specific protocol quirks     │
└─────────────────────────────────────────┘
```

### Delegation Pattern

LlamaServerProvider (and future OpenAI/Anthropic) wrap Airframe adapters:

```python
from airframe.adapters import LlamaServerAdapter
from airframe.endpoints import EndpointSpec

class LlamaServerProvider(LLMProvider):
    def __init__(self, base_url: str, ...):
        # Airframe handles transport
        self._adapter = LlamaServerAdapter(...)
        self._endpoint = EndpointSpec(...)

        # Avionics adds orchestration
        self._cost_calculator = get_cost_calculator()

    async def generate(self, ...):
        # Delegate to Airframe
        async for event in self._adapter.stream_infer(...):
            ...
        # Add cost tracking
        cost = self._cost_calculator.calculate_cost(...)
        return result
```

**Benefits:**
- Single source of truth for backend implementations (no duplication)
- 73% code reduction (LlamaServer: 440 → 260 lines)
- Cleaner separation: transport vs orchestration
- Easier testing: mock at adapter boundary

---

## Supported Providers

| Provider | Description | Local/Cloud |
|----------|-------------|-------------|
| `LlamaServerProvider` | llama.cpp server (default) | Local |
| `LlamaCppProvider` | llama.cpp in-process (C++ binding) | Local |
| `OpenAIProvider` | OpenAI API (GPT-4, etc.) | Cloud |
| `AnthropicProvider` | Anthropic API (Claude) | Cloud |
| `AzureAIFoundryProvider` | Azure OpenAI | Cloud |
| `MockProvider` | Testing mock | N/A |

### Exports

```python
from jeeves_avionics.llm import (
    LLMProvider,
    LlamaServerProvider,
    OpenAIProvider,
    AnthropicProvider,
    AzureAIFoundryProvider,
    MockProvider,
    create_llm_provider,
)
```

Note: `LlamaCppProvider` is available via factory but not directly exported from `llm/__init__.py`.

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

## llm/providers/llamaserver_provider.py

llama-server provider for distributed deployment.

**Architecture:** Delegates to `jeeves-airframe` LlamaServerAdapter for backend protocol handling.

### Class: LlamaServerProvider

```python
class LlamaServerProvider(LLMProvider):
    """llama-server (OpenAI-compatible) provider.

    Delegates to Airframe's LlamaServerAdapter for HTTP/SSE handling.
    Avionics adds: cost tracking, settings integration, telemetry enrichment.
    Airframe handles: HTTP requests, SSE parsing, retries, error categorization.

    Supports both native llama.cpp and OpenAI-compatible endpoints.
    """

    def __init__(
        self,
        base_url: str,
        timeout: float = 120.0,
        max_retries: int = 3,
        api_type: str = "native",  # "native" or "openai"
        logger: Optional[LoggerProtocol] = None,
    ):
        """
        Initialize LlamaServer provider.

        Internally creates:
        - Airframe LlamaServerAdapter (handles backend protocol)
        - Airframe EndpointSpec (endpoint configuration)
        - CostCalculator (token cost tracking)
        """
        ...
```

#### Architecture Pattern

```python
from airframe.adapters import LlamaServerAdapter
from airframe.endpoints import EndpointSpec
from airframe.types import InferenceRequest, Message, StreamEventType

class LlamaServerProvider(LLMProvider):
    def __init__(self, base_url: str, ...):
        # Airframe handles backend protocol
        self._adapter = LlamaServerAdapter(timeout=timeout, max_retries=max_retries)
        self._endpoint = EndpointSpec(base_url=base_url, backend_kind=BackendKind.LLAMA_SERVER, ...)

        # Avionics adds orchestration features
        self._cost_calculator = get_cost_calculator()

    async def generate(self, model: str, prompt: str, options: Dict) -> str:
        # Convert Avionics API → Airframe API
        request = InferenceRequest(messages=[Message(role="user", content=prompt)], ...)

        # Delegate to Airframe
        async for event in self._adapter.stream_infer(self._endpoint, request):
            if event.type == StreamEventType.MESSAGE:
                result = event.content

        # Track cost (Avionics-specific)
        cost = self._cost_calculator.calculate_cost(...)
        return result
```

#### Methods

```python
async def generate(self, model: str, prompt: str, options: Optional[Dict] = None) -> str:
    """Generate text completion via llama-server.

    Delegates to Airframe adapter for HTTP/SSE handling,
    then tracks cost and logs metrics.
    """

async def generate_stream(self, model: str, prompt: str, options: Optional[Dict] = None) -> AsyncIterator[TokenChunk]:
    """Generate text with streaming via SSE.

    Delegates to Airframe adapter, converts InferenceStreamEvent → TokenChunk.
    """

async def health_check(self) -> bool:
    """Check if llama-server is healthy.

    Delegates health check to Airframe adapter.
    """

def get_stats(self) -> Dict[str, Any]:
    """Get provider statistics (includes Airframe adapter config)."""
```

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `temperature` | `0.7` | Sampling temperature |
| `num_predict` | `512` | Max tokens to generate |
| `stop` | `[]` | Stop sequences |
| `top_p` | `0.95` | Top-p sampling |
| `top_k` | `40` | Top-k sampling (native only) |
| `repeat_penalty` | `1.1` | Repetition penalty (native only) |

---

## llm/providers/openai.py

OpenAI API provider.

### Class: OpenAIProvider

```python
class OpenAIProvider(LLMProvider):
    """Provider for OpenAI API (GPT-4, GPT-3.5, etc.)."""
    
    def __init__(
        self,
        api_key: Optional[str] = None,
        timeout: int = 60,
        max_retries: int = 3,
    ):
        ...
    
    supports_streaming = True
```

---

## llm/providers/anthropic.py

Anthropic API provider.

### Class: AnthropicProvider

```python
class AnthropicProvider(LLMProvider):
    """Provider for Anthropic API (Claude models)."""
    
    def __init__(
        self,
        api_key: Optional[str] = None,
        timeout: int = 60,
        max_retries: int = 3,
    ):
        ...
    
    supports_streaming = True
```

---

## llm/providers/azure.py

Azure OpenAI provider.

### Class: AzureAIFoundryProvider

```python
class AzureAIFoundryProvider(LLMProvider):
    """Provider for Azure OpenAI API."""
    
    def __init__(
        self,
        endpoint: Optional[str] = None,
        api_key: Optional[str] = None,
        deployment_name: Optional[str] = None,
        api_version: str = "2024-02-01",
        timeout: int = 60,
        max_retries: int = 3,
    ):
        ...
    
    supports_streaming = True
```

---

## llm/factory.py

Factory for creating LLM providers.

### Functions

```python
def create_llm_provider(
    provider_type: str,
    settings: Settings,
    agent_name: Optional[str] = None,
) -> LLMProvider:
    """Create an LLM provider based on configuration.
    
    Args:
        provider_type: "llamaserver", "openai", "anthropic", "azure", "llamacpp", "mock"
        settings: Application settings
        agent_name: Optional agent name for per-agent configuration
    """

def create_agent_provider(
    settings: Settings,
    agent_name: str,
    override_provider: Optional[str] = None,
) -> LLMProvider:
    """Create a provider for a specific agent with override support."""

def create_agent_provider_with_node_awareness(
    settings: Settings,
    agent_name: str,
    node_profiles: Optional[InferenceEndpointsProtocol] = None,
) -> LLMProvider:
    """Create provider for agent with node-aware routing (distributed mode)."""
```

### Class: LLMFactory

```python
class LLMFactory:
    """Centralized LLM provider factory with caching."""
    
    def __init__(
        self,
        settings: Settings,
        node_profiles: Optional[InferenceEndpointsProtocol] = None,
    ):
        ...
    
    def get_provider_for_agent(self, agent_name: str, use_cache: bool = True) -> LLMProvider:
        """Get provider for agent (with optional caching)."""
    
    def clear_cache(self) -> None:
        """Clear provider cache."""
    
    def mark_node_healthy(self, node_name: str, is_healthy: bool) -> None:
        """Mark node health status (for load balancing)."""
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
    
    def __init__(
        self,
        settings: Settings,
        fallback_providers: Optional[List[str]] = None,
        logger: Optional[LoggerProtocol] = None,
        resource_callback: Optional[ResourceTrackingCallback] = None,
        streaming_callback: Optional[StreamingEventCallback] = None,
    ):
        ...
```

#### Methods

```python
async def complete(
    self,
    prompt: str,
    system: Optional[str] = None,
    model: Optional[str] = None,
    agent_name: str = "unknown",
    tools: Optional[List[Dict]] = None,
    temperature: Optional[float] = None,
    max_tokens: Optional[int] = None,
) -> LLMResponse:
    """Generate LLM completion with automatic fallback and cost tracking."""

async def complete_stream(
    self,
    prompt: str,
    # ... same parameters
) -> AsyncIterator[StreamingChunk]:
    """Generate LLM completion with streaming output."""

def get_stats(self) -> Dict[str, Any]:
    """Get gateway statistics."""

def set_resource_callback(self, callback: Optional[ResourceTrackingCallback]) -> None:
    """Set the resource tracking callback."""

def set_streaming_callback(self, callback: Optional[StreamingEventCallback]) -> None:
    """Set the streaming event callback."""
```

---

## llm/cost_calculator.py

Token cost calculation for LLM API usage.

### Dataclass: CostMetrics

```python
@dataclass
class CostMetrics:
    provider: str
    model: str
    prompt_tokens: int
    completion_tokens: int
    total_tokens: int
    cost_usd: float
    timestamp: datetime
    
    @property
    def tokens_per_dollar(self) -> float:
        """Calculate efficiency: tokens per dollar."""
```

### Class: CostCalculator

```python
class CostCalculator:
    """Calculate costs for LLM API usage across providers.
    
    Pricing (per 1K tokens):
    - LlamaServer: Free (local)
    - OpenAI GPT-4: $0.03/$0.06
    - OpenAI GPT-3.5: $0.0015/$0.002
    - Anthropic Sonnet: $0.003/$0.015
    """
    
    def calculate_cost(
        self,
        provider: str,
        model: str,
        prompt_tokens: int,
        completion_tokens: int
    ) -> CostMetrics:
        """Calculate cost for an LLM request."""
    
    def estimate_cost(
        self,
        provider: str,
        model: str,
        text: str,
        estimated_tokens_per_char: float = 0.25
    ) -> float:
        """Estimate cost before sending to LLM."""
    
    def get_pricing(self, provider: str, model: str) -> tuple[float, float]:
        """Get pricing for a specific provider and model."""
```

### Global Functions

```python
def get_cost_calculator() -> CostCalculator:
    """Get global cost calculator instance."""

def calculate_cost(provider: str, model: str, prompt_tokens: int, completion_tokens: int) -> CostMetrics:
    """Convenience function for cost calculation."""
```

---

## Usage Examples

### Basic Generation

```python
from jeeves_avionics.llm import create_llm_provider
from jeeves_avionics.settings import get_settings

settings = get_settings()
provider = create_llm_provider("llamaserver", settings)

response = await provider.generate(
    model="qwen2.5-7b",
    prompt="What is 2+2?",
    options={"temperature": 0.3, "num_predict": 100}
)
print(response)
```

### Using LLM Gateway

```python
from jeeves_avionics.llm.gateway import LLMGateway
from jeeves_avionics.settings import get_settings

settings = get_settings()
gateway = LLMGateway(settings, fallback_providers=["openai"])

response = await gateway.complete(
    prompt="Analyze this code",
    system="You are a code reviewer",
    agent_name="planner",
    temperature=0.3,
)

print(f"Response: {response.text}")
print(f"Cost: ${response.cost_usd}")
print(f"Tokens: {response.tokens_used}")
```

### Streaming

```python
async for chunk in gateway.complete_stream(
    prompt="Explain Python decorators",
    agent_name="synthesizer",
):
    print(chunk.text, end="", flush=True)
    if chunk.is_final:
        print(f"\n\nTotal tokens: {chunk.cumulative_tokens}")
```
