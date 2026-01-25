# mission_system.config - Configuration Types

**Package:** `mission_system.config`  
**Purpose:** Generic configuration mechanisms and agent profiles  
**Updated:** 2026-01-23

---

## Overview

The `config` package provides generic configuration types and mechanisms. The Mission System provides the **mechanisms**, while capabilities **own domain-specific configs**.

### Ownership Model

| Layer | Owns |
|-------|------|
| **Capability** | Domain configs (LanguageConfig, ToolAccess, InferenceEndpoints, Identity) |
| **Mission System** | Generic mechanisms (ConfigRegistry, AgentProfile, constants) |
| **Protocols** | ConfigRegistryProtocol |

---

## Module Index

| Module | Description |
|--------|-------------|
| `__init__.py` | Package exports |
| `agent_profiles.py` | `AgentProfile`, `LLMProfile`, `ThresholdProfile` |
| `constants.py` | Operational constants |
| `registry.py` | `ConfigRegistry`, `ConfigKeys` |

---

## Agent Profiles (`agent_profiles.py`)

### `LLMProfile`

LLM configuration for an agent role.

```python
@dataclass
class LLMProfile:
    """LLM configuration for an agent role."""
    model_name: str = "qwen2.5-7b-instruct-q4_k_m"
    temperature: float = 0.3
    max_tokens: int = 2000
    context_window: int = 16384
    timeout_seconds: int = 120
```

### `ThresholdProfile`

Confidence thresholds for an agent.

```python
@dataclass
class ThresholdProfile:
    """Confidence thresholds for an agent."""
    clarification_threshold: float = 0.7
    approval_threshold: float = 0.8
    high_confidence: float = 0.85
    medium_confidence: float = 0.75
    low_confidence: float = 0.6
    default_confidence: float = 0.5
```

### `AgentProfile`

Complete configuration profile for an agent.

```python
@dataclass
class AgentProfile:
    """Complete configuration profile for an agent."""
    role: str
    llm: Optional[LLMProfile] = None
    thresholds: ThresholdProfile = field(default_factory=ThresholdProfile)
    latency_budget_ms: int = 30000
    retry_limit: int = 2
    
    @property
    def has_llm(self) -> bool:
        return self.llm is not None
```

**Usage:**

```python
from mission_system.config import AgentProfile, LLMProfile, ThresholdProfile

AGENT_PROFILES = {
    "planner": AgentProfile(
        role="planner",
        llm=LLMProfile(temperature=0.3, max_tokens=2500),
        thresholds=ThresholdProfile(clarification_threshold=0.7),
        latency_budget_ms=45000,
    ),
    "critic": AgentProfile(
        role="critic",
        llm=LLMProfile(temperature=0.2, max_tokens=1500),
        thresholds=ThresholdProfile(approval_threshold=0.85),
    ),
}
```

### Helper Functions

```python
def get_agent_profile(profiles: Dict[str, AgentProfile], role: str) -> Optional[AgentProfile]
def get_llm_profile(profiles: Dict[str, AgentProfile], role: str) -> Optional[LLMProfile]
def get_thresholds(profiles: Dict[str, AgentProfile], role: str) -> ThresholdProfile
def get_latency_budget(profiles: Dict[str, AgentProfile], role: str) -> int
```

---

## Config Registry (`registry.py`)

### `ConfigRegistry`

Thread-safe configuration registry for dependency injection.

```python
class ConfigRegistry:
    """
    Implementation of ConfigRegistryProtocol.
    
    Capabilities register their configs at bootstrap time.
    """
    
    def register(self, key: str, config: Any) -> None:
        """Register a configuration by key."""
    
    def get(self, key: str, default: Any = None) -> Any:
        """Retrieve a configuration by key."""
    
    def has(self, key: str) -> bool:
        """Check if a configuration is registered."""
    
    def keys(self) -> List[str]:
        """List all registered configuration keys."""
    
    def to_dict(self) -> Dict[str, Any]:
        """Export all configs as dictionary."""
```

**Usage:**

```python
from mission_system.config import ConfigRegistry, ConfigKeys

# At capability bootstrap
registry = ConfigRegistry()
registry.register(ConfigKeys.LANGUAGE_CONFIG, language_config)

# In tools (receive registry via DI)
def some_tool(registry: ConfigRegistryProtocol):
    config = registry.get(ConfigKeys.LANGUAGE_CONFIG)
    if config:
        extensions = config.code_extensions
```

### `ConfigKeys`

Standard configuration key constants.

```python
class ConfigKeys:
    LANGUAGE_CONFIG = "language_config"
    # Future keys:
    # AGENT_CONFIG = "agent_config"
    # TOOL_ACCESS_CONFIG = "tool_access_config"
    # MEMORY_CONFIG = "memory_config"
```

### Global Registry Functions

```python
def get_config_registry() -> ConfigRegistry:
    """Get the global config registry instance."""

def set_config_registry(registry: ConfigRegistry) -> None:
    """Set the global config registry instance."""

def reset_config_registry() -> None:
    """Reset the global config registry (for testing)."""
```

---

## Constants (`constants.py`)

### Platform Identity

```python
PLATFORM_NAME = "Jeeves"
PLATFORM_VERSION = "4.0.0"
PLATFORM_DESCRIPTION = "AI-powered agent platform with centralized architecture"
AGENT_ARCHITECTURE = "7-agent pipeline (Agent)"
AGENT_COUNT = 7
```

### Fuzzy Matching

```python
FUZZY_MATCH_THRESHOLD = 0.8  # from avionics/thresholds.py
FUZZY_MATCH_CONFIDENCE_THRESHOLD = 0.7
FUZZY_MATCH_SUBSTRING_WEIGHT = 1.0
FUZZY_MATCH_WORD_OVERLAP_WEIGHT = 0.9
FUZZY_MATCH_CHAR_SIMILARITY_WEIGHT = 0.7
FUZZY_MATCH_SECONDARY_WEIGHT = 0.8
```

### Task Management

```python
TASK_PRIORITY_HIGH = 0
TASK_PRIORITY_MEDIUM = 1
TASK_PRIORITY_LOW = 2
TASK_DEFAULT_PRIORITY = TASK_PRIORITY_MEDIUM
TASK_DEFAULT_STATUS = "pending"
TASK_CANDIDATE_LIMIT = 5
TASK_QUERY_LIMIT = 100
```

### Database Query Limits

```python
DB_DEFAULT_LIMIT = 100
DB_RECENT_TASKS_LIMIT = 5
DB_CONVERSATION_HISTORY_LIMIT = 10
```

### Timeout Values (seconds)

```python
LLM_REQUEST_TIMEOUT = 30
DB_QUERY_TIMEOUT = 10
TOOL_EXECUTION_TIMEOUT = 60
```

### Error Handling

```python
MAX_RETRY_ATTEMPTS = 3  # from avionics/thresholds.py
RETRY_BACKOFF_MULTIPLIER = 2.0
RETRY_INITIAL_DELAY = 1.0
```

### Response Limits

```python
VALIDATOR_RESPONSE_WORD_LIMIT = 150
```

---

## Constitutional Layering

```
Capability Layer                  ← OWNS domain configs (tool_access, etc.)
         ↓ registers via
MissionRuntime.config_registry    ← Generic injection mechanism
         ↓ implements
ConfigRegistryProtocol            ← Defined in core_engine

Mission System                    ← OWNS generic config types (AgentProfile, etc.)
```

### Correct Import Pattern

```python
# ✅ CORRECT - Import domain config from capability
from my_capability.config import (
    DomainConfig,
    AgentToolAccess,
    PROFILES,
)

# ❌ INCORRECT - Domain configs do not belong in mission_system
from mission_system.config.domain_config import DomainConfig  # WRONG
```

---

## All Exports

```python
from mission_system.config import (
    # Platform Identity
    PLATFORM_NAME,
    PLATFORM_VERSION,
    PLATFORM_DESCRIPTION,
    AGENT_ARCHITECTURE,
    AGENT_COUNT,
    
    # Fuzzy Matching
    FUZZY_MATCH_THRESHOLD,
    FUZZY_MATCH_CONFIDENCE_THRESHOLD,
    FUZZY_MATCH_SUBSTRING_WEIGHT,
    FUZZY_MATCH_WORD_OVERLAP_WEIGHT,
    FUZZY_MATCH_CHAR_SIMILARITY_WEIGHT,
    FUZZY_MATCH_SECONDARY_WEIGHT,
    
    # Task Management
    TASK_PRIORITY_LOW,
    TASK_PRIORITY_MEDIUM,
    TASK_PRIORITY_HIGH,
    TASK_DEFAULT_PRIORITY,
    TASK_DEFAULT_STATUS,
    TASK_CANDIDATE_LIMIT,
    TASK_QUERY_LIMIT,
    
    # Database Query Limits
    DB_DEFAULT_LIMIT,
    DB_RECENT_TASKS_LIMIT,
    DB_CONVERSATION_HISTORY_LIMIT,
    
    # Timeout Values
    LLM_REQUEST_TIMEOUT,
    DB_QUERY_TIMEOUT,
    TOOL_EXECUTION_TIMEOUT,
    
    # Error Handling
    MAX_RETRY_ATTEMPTS,
    RETRY_BACKOFF_MULTIPLIER,
    RETRY_INITIAL_DELAY,
    
    # Response Limits
    VALIDATOR_RESPONSE_WORD_LIMIT,
    
    # Agent Profile Types
    LLMProfile,
    ThresholdProfile,
    AgentProfile,
    get_agent_profile,
    get_llm_profile,
    get_thresholds,
    get_latency_budget,
    
    # Config Registry
    ConfigRegistry,
    ConfigKeys,
    get_config_registry,
    set_config_registry,
    reset_config_registry,
)
```

---

## Navigation

- [← Back to README](README.md)
- [→ Contracts Documentation](contracts.md)
- [→ Prompts Documentation](prompts.md)
